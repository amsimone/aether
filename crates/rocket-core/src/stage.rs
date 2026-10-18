//! A stage of a rocket.
//!
//! A stage is an ordered list of components forming a single airframe.
//! Multi-stage rockets chain stages via [`Stage::upper_stage`].

use serde::{Deserialize, Serialize};

use crate::components::Component;
use crate::id::ComponentId;
use crate::units::Length;

/// A stage of a rocket.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stage {
    /// Components in axial order (nose → tail).
    pub components: Vec<Component>,
    /// Optional motor mount specification (filled in Phase 2).
    #[serde(default)]
    pub motor_mount: Option<MotorMount>,
    /// Recovery devices (filled in Phase 2).
    #[serde(default)]
    pub recovery: Vec<RecoveryDevice>,
    /// Events (staging, deployment, etc.; filled in Phase 2).
    #[serde(default)]
    pub events: Vec<Event>,
    /// Optional upper stage (for multi-stage rockets).
    #[serde(default)]
    pub upper_stage: Option<Box<Stage>>,
}

impl Stage {
    /// Create a new, empty stage.
    #[must_use]
    pub fn new() -> Self {
        Self {
            components: Vec::new(),
            motor_mount: None,
            recovery: Vec::new(),
            events: Vec::new(),
            upper_stage: None,
        }
    }

    /// Add a component to the end of the stage.
    pub fn push(&mut self, component: Component) {
        self.components.push(component);
    }

    /// Find a component by ID.
    #[must_use]
    pub fn find(&self, id: ComponentId) -> Option<&Component> {
        self.components.iter().find(|c| c.id() == id)
    }

    /// Iterate over all components.
    pub fn iter(&self) -> impl Iterator<Item = &Component> {
        self.components.iter()
    }

    /// The total axial length of the stage (sum of component lengths,
    /// accounting for position gaps).
    ///
    /// For now, this is the position of the aft-most component end.
    #[must_use]
    pub fn length(&self) -> Length {
        self.components
            .iter()
            .map(|c| c.position() + c.length())
            .fold(Length::ZERO, |a, b| {
                if a.as_meters() > b.as_meters() {
                    a
                } else {
                    b
                }
            })
    }
}

impl Default for Stage {
    fn default() -> Self {
        Self::new()
    }
}

/// A motor mount (tube that holds a rocket motor).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MotorMount {
    /// ID of the body tube that houses the motor.
    pub tube: ComponentId,
    /// Inner diameter of the motor mount (matches motor diameter).
    pub inner_diameter: Length,
}

/// A recovery device (parachute or streamer).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecoveryDevice {
    /// Stable ID.
    pub id: ComponentId,
    /// Type of recovery device.
    pub kind: RecoveryKind,
    /// Deployed diameter (for parachutes) or length × width (for streamers).
    pub deployed_size: Length,
    /// Drag coefficient when deployed.
    pub cd: f64,
}

/// The kind of recovery device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryKind {
    /// A parachute.
    Parachute,
    /// A streamer.
    Streamer,
}

/// A flight event (staging, deployment, ignition).
///
/// Filled in Phase 2; for now this is a stub for the data model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    /// Stable ID.
    pub id: ComponentId,
    /// What triggers this event.
    pub trigger: EventTrigger,
    /// What action to take when triggered.
    pub action: EventAction,
}

/// What triggers a flight event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventTrigger {
    /// At a specific time after launch.
    Time { seconds: f64 },
    /// At a specific altitude.
    Altitude { meters: f64 },
    /// At apogee (zero vertical velocity).
    Apogee,
    /// At burnout (acceleration drops below threshold).
    Burnout,
    /// At ground impact.
    Ground,
}

/// What action to take when an event fires.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventAction {
    /// Deploy a recovery device.
    DeployRecovery { device_id: ComponentId },
    /// Separate stages.
    SeparateStage,
    /// Ignite a motor.
    IgniteMotor { motor_id: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{BodyTube, NoseCone, NoseShape};
    use crate::materials::MaterialId;

    #[test]
    fn stage_length_is_max_component_end() {
        let mut stage = Stage::new();
        stage.push(Component::NoseCone(NoseCone {
            id: ComponentId::new(1),
            position: Length::ZERO,
            material: MaterialId::new(100),
            shape: NoseShape::Conical,
            length: Length::millimeters(70.0),
            base_diameter: Length::millimeters(25.0),
            wall_thickness: Length::millimeters(1.0),
        }));
        stage.push(Component::BodyTube(BodyTube {
            id: ComponentId::new(2),
            position: Length::millimeters(70.0),
            material: MaterialId::new(101),
            length: Length::millimeters(180.0),
            outer_diameter: Length::millimeters(25.0),
            inner_diameter: Length::millimeters(24.0),
        }));

        // Nose ends at 70mm, body tube ends at 70+180 = 250mm.
        assert!((stage.length().as_millimeters() - 250.0).abs() < 1e-9);
    }

    #[test]
    fn stage_find_by_id() {
        let mut stage = Stage::new();
        stage.push(Component::NoseCone(NoseCone {
            id: ComponentId::new(42),
            position: Length::ZERO,
            material: MaterialId::new(100),
            shape: NoseShape::Conical,
            length: Length::millimeters(70.0),
            base_diameter: Length::millimeters(25.0),
            wall_thickness: Length::millimeters(1.0),
        }));

        assert!(stage.find(ComponentId::new(42)).is_some());
        assert!(stage.find(ComponentId::new(99)).is_none());
    }
}
