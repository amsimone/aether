//! UI-independent validated document commands and save/replace decisions.

use rocket_core::{validation::ValidationError, Component, ComponentId, Rocket};
use rocket_io::Project;
use rocket_sim::SimulationCase;

/// Target used to combine repeated pointer updates into a single command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Component(ComponentId),
    Case,
}

#[derive(Debug)]
struct Edit {
    before: Project,
    after: Project,
    target: Option<Target>,
}

/// A document owns the accepted inputs; widgets can only read them.
/// Full snapshots keep ordering, IDs and cross-references intact during undo.
/// History is bounded to 256 commands for this initial desktop implementation.
#[derive(Debug)]
pub struct DesignDocument {
    project: Project,
    saved: Project,
    revision: u64,
    undo: Vec<Edit>,
    redo: Vec<Edit>,
    gesture: Option<Target>,
}

impl DesignDocument {
    /// Callers load through rocket-io, which validates the project before replacement.
    pub fn from_project(project: Project) -> Self {
        Self {
            saved: project.clone(),
            project,
            revision: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            gesture: None,
        }
    }
    pub fn rocket(&self) -> &Rocket {
        &self.project.rocket
    }
    pub fn project(&self) -> &Project {
        &self.project
    }
    pub fn case(&self) -> &SimulationCase {
        &self.project.case
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn is_dirty(&self) -> bool {
        self.project.rocket != self.saved.rocket
            || self.project.case != self.saved.case
            || self.project.runs.len() != self.saved.runs.len()
            || self
                .project
                .runs
                .iter()
                .zip(&self.saved.runs)
                .any(|(a, b)| !std::sync::Arc::ptr_eq(a, b))
    }
    pub fn mark_saved(&mut self) {
        self.saved = self.project.clone();
        self.end_gesture();
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn end_gesture(&mut self) {
        self.gesture = None;
    }

    pub fn edit_component(
        &mut self,
        updated: Component,
        pointer: bool,
    ) -> Result<bool, ValidationError> {
        let id = updated.id();
        let mut project = self.project.clone();
        let existing = project
            .rocket
            .root
            .components
            .iter_mut()
            .find(|c| c.id() == id)
            .ok_or_else(|| ValidationError(format!("Component {id} was not found")))?;
        if std::mem::discriminant(existing) != std::mem::discriminant(&updated) {
            return Err(ValidationError(
                "An edit cannot change a component's type".into(),
            ));
        }
        *existing = updated;
        self.commit(project, pointer.then_some(Target::Component(id)))
    }

    pub fn add_component(&mut self, component: Component) -> Result<bool, ValidationError> {
        let mut project = self.project.clone();
        project.rocket.root.push(component);
        self.commit(project, None)
    }

    pub fn delete_component(&mut self, id: ComponentId) -> Result<bool, ValidationError> {
        if self.rocket().root.find(id).is_none() {
            return Err(ValidationError(format!("Component {id} was not found")));
        }
        let mut project = self.project.clone();
        project.rocket.root.components.retain(|c| c.id() != id);
        if project
            .rocket
            .root
            .motor_mount
            .as_ref()
            .is_some_and(|mount| mount.tube == id)
        {
            project.rocket.root.motor_mount = None;
        }
        // A motor-mount assignment belongs to its deleted tube. Other components
        // (such as mounted fins) must be removed or remounted explicitly first.
        self.commit(project, None)
    }

    pub fn duplicate_component(
        &mut self,
        id: ComponentId,
        new_id: ComponentId,
    ) -> Result<bool, ValidationError> {
        let mut component = self
            .rocket()
            .root
            .find(id)
            .cloned()
            .ok_or_else(|| ValidationError(format!("Component {id} was not found")))?;
        match &mut component {
            Component::NoseCone(c) => c.id = new_id,
            Component::BodyTube(c) => c.id = new_id,
            Component::Transition(c) => c.id = new_id,
            Component::Coupler(c) => c.id = new_id,
            Component::FinSet(c) => c.id = new_id,
            Component::LaunchLug(c) => c.id = new_id,
            Component::Pod(c) => c.id = new_id,
            Component::MassObject(c) => c.id = new_id,
        }
        self.add_component(component)
    }

    pub fn edit_case(
        &mut self,
        case: SimulationCase,
        pointer: bool,
    ) -> Result<bool, ValidationError> {
        let mut project = self.project.clone();
        project.case = case;
        self.commit(project, pointer.then_some(Target::Case))
    }

    /// Retaining a run changes persisted content without becoming a vehicle edit command.
    pub fn retain_run(&mut self, run: rocket_sim::RunRecord) -> Result<(), ValidationError> {
        run.validate()?;
        if self.project.runs.iter().any(|r| r.id == run.id) {
            return Err(ValidationError("Duplicate run ID".into()));
        }
        self.project.runs.push(std::sync::Arc::new(run));
        self.revision += 1;
        self.end_gesture();
        Ok(())
    }

    fn commit(
        &mut self,
        project: Project,
        target: Option<Target>,
    ) -> Result<bool, ValidationError> {
        project.validate_inputs()?;
        if project == self.project {
            return Ok(false);
        }
        if target.is_some()
            && self.gesture == target
            && self
                .undo
                .last()
                .is_some_and(|e| e.target == target && e.after == self.project)
        {
            if let Some(edit) = self.undo.last_mut() {
                edit.after = project.clone();
            }
            if self.undo.last().is_some_and(|e| e.before == e.after) {
                self.undo.pop();
            }
        } else {
            self.undo.push(Edit {
                before: self.project.clone(),
                after: project.clone(),
                target,
            });
            if self.undo.len() > 256 {
                self.undo.remove(0);
            }
        }
        self.project = project;
        self.redo.clear();
        self.gesture = target;
        self.revision += 1;
        Ok(true)
    }
    pub fn undo(&mut self) -> bool {
        self.end_gesture();
        let Some(edit) = self.undo.pop() else {
            return false;
        };
        self.project.rocket = edit.before.rocket.clone();
        self.project.case = edit.before.case.clone();
        self.redo.push(edit);
        self.revision += 1;
        true
    }
    pub fn redo(&mut self) -> bool {
        self.end_gesture();
        let Some(edit) = self.redo.pop() else {
            return false;
        };
        self.project.rocket = edit.after.rocket.clone();
        self.project.case = edit.after.case.clone();
        self.undo.push(edit);
        self.revision += 1;
        true
    }
}

/// Destructive document actions use one save/discard/cancel decision path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentAction {
    New,
    Reference,
    Open,
    Quit,
}
#[derive(Debug, Clone, Copy)]
pub enum Decision {
    Save,
    Discard,
    Cancel,
}

/// Pending intent survives a cancelled or failed save; the current document stays open.
#[derive(Debug, Default)]
pub struct ReplacementGuard {
    pending: Option<DocumentAction>,
}
impl ReplacementGuard {
    pub fn pending(&self) -> Option<DocumentAction> {
        self.pending
    }
    pub fn request(&mut self, action: DocumentAction, dirty: bool) -> Option<DocumentAction> {
        if self.pending.is_some() {
            return None;
        }
        if dirty {
            self.pending = Some(action);
            None
        } else {
            Some(action)
        }
    }
    pub fn decide(&mut self, decision: Decision) -> Option<DocumentAction> {
        match decision {
            Decision::Discard => self.pending.take(),
            Decision::Cancel => {
                self.pending = None;
                None
            }
            Decision::Save => None,
        }
    }
    pub fn saved(&mut self, succeeded: bool) -> Option<DocumentAction> {
        if succeeded {
            self.pending.take()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_core::{Length, Mass, MassObject, MassObjectKind};

    fn document() -> DesignDocument {
        DesignDocument::from_project(crate::simulate::reference_project(
            crate::reference::reference_rocket(),
        ))
    }
    fn body(doc: &DesignDocument, length: f64) -> Component {
        let Component::BodyTube(mut body) = doc
            .rocket()
            .root
            .find(ComponentId::new(2))
            .expect("body")
            .clone()
        else {
            panic!("body");
        };
        body.length = Length::millimeters(length);
        Component::BodyTube(body)
    }

    #[test]
    fn mixed_commands_restore_order_ids_references_and_saved_baselines() {
        let mut doc = document();
        let initial = doc.project().clone();
        doc.edit_component(body(&doc, 220.0), false)
            .expect("body edit");
        let edited = doc.project().clone();
        doc.duplicate_component(ComponentId::new(1), ComponentId::new(100))
            .expect("duplicate");
        let duplicated = doc.project().clone();
        doc.delete_component(ComponentId::new(3))
            .expect("remove fins");
        let removed = doc.project().clone();
        let mut case = doc.case().clone();
        case.config.wind = (4.0, 2.0, 0.0);
        doc.edit_case(case, false).expect("case edit");
        let final_state = doc.project().clone();
        doc.mark_saved();
        for expected in [&removed, &duplicated, &edited, &initial] {
            assert!(doc.undo());
            assert_eq!(doc.project(), expected);
            assert!(doc.is_dirty());
        }
        assert!(!doc.can_undo());
        for expected in [&edited, &duplicated, &removed, &final_state] {
            assert!(doc.redo());
            assert_eq!(doc.project(), expected);
        }
        assert!(!doc.is_dirty());
        assert_eq!(doc.revision(), 12);
    }

    #[test]
    fn rejected_commands_and_noops_preserve_the_model_revision_and_redo() {
        let mut doc = document();
        doc.edit_component(body(&doc, 220.0), false).expect("edit");
        doc.undo();
        let original = doc.project().clone();
        let revision = doc.revision();
        for invalid in [f64::NAN, f64::INFINITY, -1.0, 0.0] {
            assert!(doc.edit_component(body(&doc, invalid), false).is_err());
        }
        assert!(doc.delete_component(ComponentId::new(2)).is_err()); // referenced by fins
        assert!(doc
            .duplicate_component(ComponentId::new(1), ComponentId::new(2))
            .is_err());
        assert!(doc.delete_component(ComponentId::new(999)).is_err());
        let replacement = Component::MassObject(MassObject {
            id: ComponentId::new(2),
            position: Length::ZERO,
            mass: Mass::grams(1.0),
            kind: MassObjectKind::Other,
        });
        assert!(doc.edit_component(replacement, false).is_err());
        let mut case = doc.case().clone();
        case.config.time_step = 0.0;
        assert!(doc.edit_case(case, false).is_err());
        assert!(!doc.edit_component(body(&doc, 180.0), false).expect("no-op"));
        assert_eq!(doc.project(), &original);
        assert_eq!(doc.revision(), revision);
        assert!(doc.can_redo());
        assert!(!doc.is_dirty());
    }

    #[test]
    fn drags_coalesce_separately_from_other_operations_and_branching_clears_redo() {
        let mut doc = document();
        let initial = doc.project().clone();
        for length in [190.0, 200.0, 210.0] {
            doc.edit_component(body(&doc, length), true).expect("drag");
        }
        doc.end_gesture();
        doc.edit_component(body(&doc, 230.0), true)
            .expect("second drag");
        doc.undo();
        assert_eq!(
            doc.rocket().root.find(ComponentId::new(2)),
            Some(&body(&doc, 210.0))
        );
        doc.undo();
        assert_eq!(doc.project(), &initial);
        doc.edit_component(body(&doc, 240.0), false)
            .expect("branch");
        assert!(!doc.can_redo());
        doc.undo();
        doc.edit_component(body(&doc, 200.0), true).expect("away");
        doc.edit_component(body(&doc, 180.0), true).expect("back");
        assert!(!doc.can_undo());
        assert!(!doc.is_dirty());
    }

    #[test]
    fn additions_and_deletions_preserve_history_and_mount_integrity() {
        let mut doc = document();
        let original = doc.project().clone();
        let added = Component::MassObject(MassObject {
            id: ComponentId::new(100),
            position: Length::ZERO,
            mass: Mass::grams(10.0),
            kind: MassObjectKind::Other,
        });
        doc.add_component(added).expect("insert");
        doc.delete_component(ComponentId::new(100)).expect("delete");
        assert_eq!(doc.project(), &original);
        assert!(!doc.is_dirty());
        assert!(doc.undo());
        assert!(doc.rocket().root.find(ComponentId::new(100)).is_some());
        assert!(doc.undo());
        assert_eq!(doc.project(), &original);
        doc.delete_component(ComponentId::new(3))
            .expect("remove dependent fins");
        doc.delete_component(ComponentId::new(2))
            .expect("remove body");
        doc.undo();
        doc.undo();
        assert_eq!(doc.project(), &original);
    }

    #[test]
    fn replacement_requires_successful_save_or_explicit_discard_for_each_intent() {
        for action in [
            DocumentAction::New,
            DocumentAction::Reference,
            DocumentAction::Open,
            DocumentAction::Quit,
        ] {
            let mut guard = ReplacementGuard::default();
            assert_eq!(guard.request(action, false), Some(action));
            assert_eq!(guard.request(action, true), None);
            assert_eq!(guard.pending(), Some(action));
            assert_eq!(guard.request(DocumentAction::Quit, true), None);
            assert_eq!(guard.decide(Decision::Save), None);
            assert_eq!(guard.saved(false), None); // cancelled picker or write failure
            assert_eq!(guard.pending(), Some(action));
            assert_eq!(guard.saved(true), Some(action));
            assert_eq!(guard.pending(), None);
            guard.request(action, true);
            guard.decide(Decision::Cancel);
            assert_eq!(guard.pending(), None);
            guard.request(action, true);
            assert_eq!(guard.decide(Decision::Discard), Some(action));
        }
    }
}

#[cfg(test)]
mod retained_run_tests {
    use super::*;
    #[test]
    fn undoing_inputs_preserves_completed_runs_and_save_baselines_include_results() {
        let project = crate::simulate::reference_project(crate::reference::reference_rocket());
        let mut doc = DesignDocument::from_project(project);
        let mut case = doc.case().clone();
        case.maximum_duration_s = 0.1;
        doc.edit_case(case, false).expect("case edit");
        let result = doc.case().run(doc.rocket()).expect("run");
        let run = rocket_sim::RunRecord::capture(
            doc.rocket().clone(),
            doc.case().clone(),
            result,
            doc.revision(),
        );
        let id = run.id.clone();
        doc.retain_run(run).expect("retain");
        assert!(doc.undo());
        assert_eq!(doc.project().runs[0].id, id);
        assert!(doc.is_dirty());
        doc.mark_saved();
        assert!(!doc.is_dirty());
        assert!(doc.redo());
        assert!(doc.is_dirty());
        assert!(doc.undo());
        assert!(!doc.is_dirty());
        assert_eq!(doc.project().runs.len(), 1);
    }
}
