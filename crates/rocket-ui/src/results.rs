//! Dedicated retained-run analysis workspace.
#![allow(clippy::too_many_lines, clippy::cast_precision_loss)]
use crate::{
    app::AetherApp,
    plots::{self, PlotState},
};
use eframe::egui;
use rocket_sim::{RunRecord, Termination};
#[derive(Default)]
pub struct ResultsState {
    pub selected: Option<String>,
    pub baseline: Option<String>,
    pub plot: PlotState,
}

pub fn sidebar(app: &mut AetherApp, ui: &mut egui::Ui) {
    ui.heading("Retained runs");
    if app.sim_panel.job.is_some() {
        ui.spinner();
        ui.label("Flight case running…");
        if ui.button("Cancel run").clicked() {
            app.cancel_run();
        }
    }
    if ui.button("Import run record…").clicked() {
        app.import_run();
    }
    ui.separator();
    let runs = app.document.project().runs.clone();
    if runs.is_empty() {
        ui.label("Run a flight case to create an inspectable result.");
    }
    for (i, run) in runs.iter().enumerate().rev() {
        let selected = app.results.selected.as_ref() == Some(&run.id);
        if ui
            .selectable_label(
                selected,
                format!(
                    "{} · {}\n{} · {:.1} m AGL",
                    i + 1,
                    run.label,
                    outcome(&run.result.termination),
                    run.result.max_altitude.as_meters()
                ),
            )
            .clicked()
        {
            app.results.selected = Some(run.id.clone());
            app.results.plot.reset();
        }
    }
}
pub fn show(app: &mut AetherApp, ui: &mut egui::Ui) {
    let runs = app.document.project().runs.clone();
    let selected = app
        .results
        .selected
        .as_ref()
        .and_then(|id| runs.iter().find(|r| r.id == *id))
        .or_else(|| runs.last())
        .cloned();
    ui.heading("Results");
    if let Some(error) = &app.sim_panel.error {
        ui.colored_label(egui::Color32::LIGHT_RED, error);
    }
    let Some(run) = selected else {
        ui.label("Choose Flight case and Run case. Results will appear here and remain available after edits.");
        if let Some(error) = &app.sim_panel.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
        return;
    };
    app.results.selected = Some(run.id.clone());
    ui.horizontal_wrapped(|ui| {
        ui.strong(&run.label);
        ui.label(outcome(&run.result.termination));
        if ui.button("Export CSV…").clicked() {
            app.export_csv(run.id.clone());
        }
        if ui.button("Export run…").clicked() {
            app.export_run(run.id.clone());
        }
    });
    if let Termination::NumericalFailure(message) | Termination::ModelDomainLimit(message) =
        &run.result.termination
    {
        ui.colored_label(egui::Color32::LIGHT_RED, message);
    }
    if run.rocket != *app.document.rocket() || run.case != *app.document.case() {
        ui.colored_label(
            egui::Color32::from_rgb(240, 185, 105),
            "Earlier inputs: this run retains its own vehicle and flight case.",
        );
    }
    let previous_baseline = app.results.baseline.clone();
    ui.horizontal_wrapped(|ui| {
        ui.label("Compare with");
        let selected_label = app
            .results
            .baseline
            .as_ref()
            .and_then(|id| runs.iter().find(|r| r.id == *id))
            .map_or("None", |r| r.label.as_str());
        egui::ComboBox::from_id_salt("baseline_run")
            .width(210.0)
            .selected_text(selected_label)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut app.results.baseline, None, "None");
                for (i, r) in runs.iter().enumerate() {
                    if r.id != run.id {
                        ui.selectable_value(
                            &mut app.results.baseline,
                            Some(r.id.clone()),
                            format!("{} · {}", i + 1, r.label),
                        );
                    }
                }
            });
    });
    if previous_baseline != app.results.baseline {
        app.results.plot.reset();
    }
    let baseline = app
        .results
        .baseline
        .as_ref()
        .filter(|id| **id != run.id)
        .and_then(|id| runs.iter().find(|r| r.id == *id));
    egui::ScrollArea::vertical().id_salt("results_workspace").show(ui,|ui|{
        egui::Grid::new("run_summary").num_columns(4).striped(true).show(ui,|ui|{
            ui.strong("Metric");ui.strong("Selected");ui.strong("Baseline");ui.strong("Difference");ui.end_row();
            let metrics=summary(&run);let base=baseline.map(|b|summary(b));
            for (i,(label,value)) in metrics.iter().enumerate(){ui.label(*label);ui.monospace(format!("{value:.4}"));if let Some(base)=&base{ui.monospace(format!("{:.4}",base[i].1));ui.monospace(format!("{:+.4}",value-base[i].1));}else{ui.label("—");ui.label("—");}ui.end_row();}
        });
        ui.add_space(12.0);
        plots::show(ui,&mut app.results.plot,&run,baseline.map(std::convert::AsRef::as_ref));
        ui.collapsing("Events and provenance",|ui|{
            ui.label(format!("Run {} · document revision {}",run.id,run.source_document_revision));
            ui.label(format!("{} · source {}",run.solver_version,run.source_revision));
            for event in &run.result.events{ui.label(format!("{:.4} s · {}",event.time_s,event.name));}
            let fallback=run.result.telemetry.iter().filter(|p|p.flight.is_some_and(|f|f.aero_source==rocket_aero::AeroSource::BuiltinOutsideDataset)).count();
            if fallback>0{ui.colored_label(egui::Color32::from_rgb(240,185,105),format!("{fallback} samples used the explicitly selected built-in fallback outside the dataset domain."));}
            if matches!(run.case.aerodynamics,rocket_aero::AeroModel::Dataset{..}){ui.label("Imported coefficients are static: no dynamic aerodynamic derivatives are supplied by the dataset.");}
        });
        ui.collapsing("Telemetry table",|ui|{
            ui.label("SI units. The CSV includes all channels, quaternion components, rates, and coefficient source.");
            egui::ScrollArea::both().id_salt("telemetry_table").max_height(300.0).show_rows(ui,22.0,run.result.telemetry.len()+1,|ui,range|{
                egui::Grid::new("telemetry_rows").striped(true).min_col_width(80.0).show(ui,|ui|for index in range {
                    if index==0{for title in ["t (s)","AGL (m)","Speed (m/s)","Mach","q (Pa)","Alpha (rad)","Mass (kg)","Thrust (N)","Drag (N)","Accel (m/s²)"]{ui.strong(title);}ui.end_row();continue;}
                    let p=&run.result.telemetry[index-1];let f=p.flight;
                    let values=[Some(p.time.as_seconds()),Some(p.position.2-run.case.config.pad_altitude.as_meters()),Some((p.velocity.0.powi(2)+p.velocity.1.powi(2)+p.velocity.2.powi(2)).sqrt()),f.map(|v|v.mach),f.map(|v|v.dynamic_pressure_pa),f.map(|v|v.alpha_rad),Some(p.mass.as_kilograms()),Some(p.thrust.as_newtons()),Some(p.drag.as_newtons()),Some(p.acceleration)];
                    for v in values {ui.monospace(v.map_or_else(||"—".into(),|v|format!("{v:.5}")));}ui.end_row();
                });
            });
        });
    });
}
fn summary(run: &RunRecord) -> [(&'static str, f64); 5] {
    [
        ("Apogee AGL (m)", run.result.max_altitude.as_meters()),
        (
            "Peak ground speed (m/s)",
            run.result.max_velocity.as_meters_per_second(),
        ),
        ("Peak local Mach", run.result.max_mach()),
        (
            "Peak dynamic pressure (Pa)",
            run.result.max_dynamic_pressure_pa.unwrap_or(0.0),
        ),
        ("Run end (s)", run.result.landing_time.as_seconds()),
    ]
}
fn outcome(t: &Termination) -> &'static str {
    match t {
        Termination::GroundImpact => "Ground impact",
        Termination::DurationLimit => "Duration limit",
        Termination::Cancelled => "Cancelled",
        Termination::NumericalFailure(_) => "Numerical failure",
        Termination::ModelDomainLimit(_) => "Aerodynamic domain limit",
        Termination::InvalidInput => "Invalid inputs",
    }
}
