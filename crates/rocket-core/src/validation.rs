//! Semantic checks shared by editing and persistence.

use std::{collections::HashSet, fmt};

use crate::{Component, ComponentId, FinPlanform, Length, NoseShape, Rocket, Stage};

/// An invalid engineering value or reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError(pub String);

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ValidationError {}

/// Check dimensions, IDs, material references, and stage-local mounting references.
/// Empty designs are valid editing documents.
///
/// # Errors
/// Returns the first invalid engineering value or reference.
pub fn validate_rocket(rocket: &Rocket) -> Result<(), ValidationError> {
    for material in rocket.materials.iter() {
        positive(
            material.density.as_kilograms_per_cubic_meter(),
            "Material density",
        )?;
    }
    validate_stage(&rocket.root, rocket, &mut HashSet::new())
}

fn validate_stage(
    stage: &Stage,
    rocket: &Rocket,
    ids: &mut HashSet<ComponentId>,
) -> Result<(), ValidationError> {
    for component in &stage.components {
        if !ids.insert(component.id()) {
            return Err(ValidationError(format!(
                "Duplicate component ID {}",
                component.id()
            )));
        }
        validate_component(component, stage, rocket)
            .map_err(|error| ValidationError(format!("Component {}: {error}", component.id())))?;
    }
    if let Some(mount) = &stage.motor_mount {
        body_reference(stage, mount.tube)?;
        positive(mount.inner_diameter.as_meters(), "Motor mount diameter")?;
    }
    if let Some(upper) = &stage.upper_stage {
        validate_stage(upper, rocket, ids)?;
    }
    Ok(())
}

fn body_reference(stage: &Stage, id: ComponentId) -> Result<(), ValidationError> {
    if matches!(stage.find(id), Some(Component::BodyTube(_))) {
        Ok(())
    } else {
        Err(ValidationError(format!("Body tube {id} is required by a mounting reference; remove or remount dependents first")))
    }
}

#[allow(clippy::too_many_lines)]
fn validate_component(
    component: &Component,
    stage: &Stage,
    rocket: &Rocket,
) -> Result<(), ValidationError> {
    nonnegative(component.position().as_meters(), "Position")?;
    if let Some(id) = component.material_id() {
        if rocket.materials.get(id).is_none() {
            return Err(ValidationError(format!("Material {id} was not found")));
        }
    }
    if !matches!(component, Component::FinSet(_) | Component::MassObject(_)) {
        positive(component.length().as_meters(), "Length")?;
    }
    match component {
        Component::NoseCone(c) => {
            positive(c.base_diameter.as_meters(), "Base diameter")?;
            wall(c.wall_thickness, c.base_diameter)?;
            match c.shape {
                NoseShape::Parabolic { n } if !n.is_finite() || !(0.0..=1.0).contains(&n) => {
                    return Err(ValidationError(
                        "Parabolic exponent must be between 0 and 1".into(),
                    ))
                }
                NoseShape::SecantOgive { radius_ratio }
                    if !radius_ratio.is_finite() || radius_ratio < 1.0 =>
                {
                    return Err(ValidationError(
                        "Ogive radius ratio must be finite and at least 1".into(),
                    ))
                }
                _ => {}
            }
        }
        Component::BodyTube(c) => tube(c.inner_diameter, c.outer_diameter)?,
        Component::LaunchLug(c) => tube(c.inner_diameter, c.outer_diameter)?,
        Component::Transition(c) => {
            positive(c.fore_diameter.as_meters(), "Fore diameter")?;
            positive(c.aft_diameter.as_meters(), "Aft diameter")?;
            wall(
                c.wall_thickness,
                Length::meters(c.fore_diameter.as_meters().min(c.aft_diameter.as_meters())),
            )?;
        }
        Component::Coupler(c) => positive(c.outer_diameter.as_meters(), "Outer diameter")?,
        Component::Pod(c) => {
            positive(c.outer_diameter.as_meters(), "Outer diameter")?;
            nonnegative(c.radial_offset.as_meters(), "Radial offset")?;
            finite(c.angle, "Pod angle")?;
        }
        Component::MassObject(c) => nonnegative(c.mass.as_kilograms(), "Mass")?,
        Component::FinSet(c) => {
            if c.count == 0 {
                return Err(ValidationError("A fin set needs at least one fin".into()));
            }
            positive(c.thickness.as_meters(), "Fin thickness")?;
            body_reference(stage, c.mount)?;
            match &c.planform {
                FinPlanform::Trapezoidal {
                    root_chord,
                    tip_chord,
                    span,
                    sweep,
                } => {
                    positive(root_chord.as_meters(), "Root chord")?;
                    nonnegative(tip_chord.as_meters(), "Tip chord")?;
                    positive(span.as_meters(), "Fin span")?;
                    finite(sweep.as_meters(), "Fin sweep")?;
                }
                FinPlanform::Elliptical { root_chord, span } => {
                    positive(root_chord.as_meters(), "Root chord")?;
                    positive(span.as_meters(), "Fin span")?;
                }
                FinPlanform::Custom { points } => {
                    if points.len() < 3
                        || points.iter().any(|(x, y)| !x.is_finite() || !y.is_finite())
                    {
                        return Err(ValidationError(
                            "Fin polygon needs at least three finite vertices".into(),
                        ));
                    }
                    positive(c.planform.area(), "Fin area")?;
                }
            }
        }
    }
    Ok(())
}

fn tube(inner: Length, outer: Length) -> Result<(), ValidationError> {
    positive(outer.as_meters(), "Outer diameter")?;
    nonnegative(inner.as_meters(), "Inner diameter")?;
    if inner.as_meters() >= outer.as_meters() {
        return Err(ValidationError(
            "Inner diameter must be smaller than outer diameter".into(),
        ));
    }
    Ok(())
}
fn wall(thickness: Length, diameter: Length) -> Result<(), ValidationError> {
    nonnegative(thickness.as_meters(), "Wall thickness")?;
    if thickness.as_meters() * 2.0 > diameter.as_meters() {
        return Err(ValidationError(
            "Wall thickness cannot exceed the radius".into(),
        ));
    }
    Ok(())
}
fn finite(value: f64, name: &str) -> Result<(), ValidationError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(ValidationError(format!("{name} must be finite")))
    }
}
fn nonnegative(value: f64, name: &str) -> Result<(), ValidationError> {
    finite(value, name)?;
    if value >= 0.0 {
        Ok(())
    } else {
        Err(ValidationError(format!("{name} cannot be negative")))
    }
}
fn positive(value: f64, name: &str) -> Result<(), ValidationError> {
    finite(value, name)?;
    if value > 0.0 {
        Ok(())
    } else {
        Err(ValidationError(format!("{name} must be greater than zero")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Coupler, Mass, Pod, Transition};

    fn rocket() -> Rocket {
        let mut rocket: Rocket =
            serde_json::from_str(include_str!("../../../examples/aether-one.rocket"))
                .expect("reference");
        let material = rocket.materials.iter().next().expect("material").id;
        rocket.root.push(Component::Transition(Transition {
            id: ComponentId::new(6),
            position: Length::ZERO,
            material,
            length: Length::millimeters(30.0),
            fore_diameter: Length::millimeters(25.0),
            aft_diameter: Length::millimeters(35.0),
            wall_thickness: Length::millimeters(1.0),
        }));
        rocket.root.push(Component::Coupler(Coupler {
            id: ComponentId::new(7),
            position: Length::ZERO,
            material,
            length: Length::millimeters(30.0),
            outer_diameter: Length::millimeters(24.0),
        }));
        rocket.root.push(Component::Pod(Pod {
            id: ComponentId::new(8),
            position: Length::ZERO,
            material,
            length: Length::millimeters(30.0),
            outer_diameter: Length::millimeters(10.0),
            radial_offset: Length::millimeters(20.0),
            angle: 0.0,
        }));
        rocket
    }
    #[test]
    fn each_component_variant_rejects_invalid_physical_values() {
        let original = rocket();
        validate_rocket(&original).expect("all variants valid");
        for index in 0..original.root.components.len() {
            let mut invalid = original.clone();
            match &mut invalid.root.components[index] {
                Component::NoseCone(c) => c.wall_thickness = Length::meters(1.0),
                Component::BodyTube(c) => c.inner_diameter = c.outer_diameter,
                Component::Transition(c) => c.aft_diameter = Length::ZERO,
                Component::Coupler(c) => c.length = Length::meters(f64::NAN),
                Component::FinSet(c) => c.thickness = Length::ZERO,
                Component::LaunchLug(c) => c.inner_diameter = Length::meters(-1.0),
                Component::Pod(c) => c.angle = f64::INFINITY,
                Component::MassObject(c) => c.mass = Mass::kilograms(-1.0),
            }
            assert!(validate_rocket(&invalid).is_err(), "variant {index}");
        }
    }
    #[test]
    fn reference_and_identity_checks_include_upper_stages() {
        let original = rocket();
        let mut invalid = original.clone();
        invalid
            .root
            .components
            .push(invalid.root.components[0].clone());
        assert!(validate_rocket(&invalid).is_err());
        let mut invalid = original.clone();
        if let Component::FinSet(c) = &mut invalid.root.components[2] {
            c.mount = ComponentId::new(1);
        }
        assert!(validate_rocket(&invalid).is_err());
        let mut invalid = original.clone();
        if let Component::NoseCone(c) = &mut invalid.root.components[0] {
            c.material = ComponentId::new(999);
        }
        assert!(validate_rocket(&invalid).is_err());
        let mut invalid = original;
        invalid.root.upper_stage = Some(Box::new(invalid.root.clone()));
        assert!(validate_rocket(&invalid).is_err());
    }
}
