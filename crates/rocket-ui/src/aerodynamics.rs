//! Aerodynamic data inspection, comparison, and external CFD studies.
#![allow(clippy::too_many_lines)]
use crate::app::AetherApp;
use eframe::egui;
use rocket_aero::{AeroDataset, AeroModel, AeroQuery, DomainPolicy};
use rocket_cfd::{CfdOutcome, CfdSettings};
use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, mpsc::Receiver, Arc},
};

pub struct CfdJob {
    pub receiver: Receiver<Result<CfdOutcome, String>>,
    pub cancel: Arc<AtomicBool>,
    pub directory: PathBuf,
}
pub struct AeroPanelState {
    pub preview: Option<Arc<AeroDataset>>,
    pub settings: Option<CfdSettings>,
    pub python: String,
    pub foam_bashrc: String,
    pub job: Option<CfdJob>,
    pub completed: Option<(PathBuf, CfdOutcome)>,
    pub artifact: Option<PathBuf>,
    pub error: Option<String>,
    mach: f64,
    alpha_deg: f64,
    beta_deg: f64,
    reynolds: f64,
}
impl Default for AeroPanelState {
    fn default() -> Self {
        Self {
            preview: None,
            settings: None,
            python: std::env::var("AETHER_CFD_PYTHON").unwrap_or_else(|_| "python3".into()),
            foam_bashrc: std::env::var("WM_PROJECT_DIR").map_or_else(
                |_| {
                    [
                        "/Volumes/OpenFOAM-v2606/etc/bashrc",
                        "/usr/lib/openfoam/openfoam2606/etc/bashrc",
                        "/opt/openfoam2606/etc/bashrc",
                    ]
                    .into_iter()
                    .find(|p| std::path::Path::new(p).is_file())
                    .unwrap_or("")
                    .into()
                },
                |p| format!("{p}/etc/bashrc"),
            ),
            job: None,
            completed: None,
            artifact: None,
            error: None,
            mach: 0.2,
            alpha_deg: 0.0,
            beta_deg: 0.0,
            reynolds: 1e6,
        }
    }
}
impl AeroPanelState {
    pub fn set_preview(&mut self, data: Arc<AeroDataset>) {
        if let Some(row) = data.rows.first() {
            self.mach = row.mach;
            self.alpha_deg = row.alpha_rad.to_degrees();
            self.beta_deg = row.beta_rad.to_degrees();
        }
        self.reynolds = data
            .metadata
            .reynolds_min
            .midpoint(data.metadata.reynolds_max);
        self.preview = Some(data);
    }
}
pub fn show(app: &mut AetherApp, ui: &mut egui::Ui) {
    ui.heading("Aerodynamics");
    ui.label(
        "Review aerodynamic evidence and compare predictions before selecting a model for flight.",
    );
    ui.horizontal_wrapped(|ui| {
        if ui.button("Import coefficient dataset…").clicked() {
            app.import_dataset();
        }
        if ui.button("Use built-in flight model").clicked() {
            let mut case = app.document.case().clone();
            case.aerodynamics = AeroModel::Builtin;
            match app.document.edit_case(case, false) {
                Ok(_) => app.mark_dirty(),
                Err(e) => app.aero_panel.error = Some(e.to_string()),
            }
        }
    });
    egui::ScrollArea::vertical().id_salt("aerodynamics_workspace").show(ui,|ui|{
        let preview=app.aero_panel.preview.clone().or_else(||match &app.document.case().aerodynamics{AeroModel::Dataset{data,..}=>Some(data.clone()),AeroModel::Builtin=>None});
        if let Some(data)=preview {
            ui.separator();ui.strong(&data.metadata.name);ui.label(&data.metadata.source);
            ui.label(format!("Reference: A = {} m² · L = {} m · moment station = {} m aft of nose",data.metadata.reference_area_m2,data.metadata.reference_length_m,data.metadata.moment_reference_station_m));
            ui.label(format!("{} static rows · Re [{:.3}, {:.3}] based on reference length",data.rows.len(),data.metadata.reynolds_min,data.metadata.reynolds_max));
            ui.label("Body axes: +x toward nose, +y right, +z completes right-handed frame. F = qA(−CA, CY, CN). M = qAL(Cl, Cm, Cn).");
            ui.collapsing("Evidence and limitations",|ui|{ui.label(&data.metadata.evidence);ui.monospace(format!("Vehicle SHA-256: {}",data.metadata.vehicle_sha256));});
            let model=AeroModel::Dataset{data:data.clone(),outside_domain:DomainPolicy::Stop};
            let binding=model.validate_vehicle(app.document.rocket());
            if let Err(error)=&binding{ui.colored_label(egui::Color32::LIGHT_RED,error.to_string());}
            ui.horizontal(|ui|{
                if ui.add_enabled(binding.is_ok(),egui::Button::new("Use dataset for flight case")).clicked(){let mut case=app.document.case().clone();case.aerodynamics=model;match app.document.edit_case(case,false){Ok(_)=>app.mark_dirty(),Err(e)=>app.aero_panel.error=Some(e.to_string())}}
                if let AeroModel::Dataset{data:active,outside_domain}=&app.document.case().aerodynamics {
                    if active.as_ref()==data.as_ref(){let mut fallback=*outside_domain==DomainPolicy::BuiltinFallback;
                        if ui.checkbox(&mut fallback,"Use built-in model outside data coverage").changed(){let mut case=app.document.case().clone();case.aerodynamics=AeroModel::Dataset{data:data.clone(),outside_domain:if fallback{DomainPolicy::BuiltinFallback}else{DomainPolicy::Stop}};match app.document.edit_case(case,false){Ok(_)=>app.mark_dirty(),Err(e)=>app.aero_panel.error=Some(e.to_string())}}
                    }
                }
            });
            ui.label("Default outside-domain behavior: stop with a diagnostic. Explicit fallback is retained in the case and marked in run telemetry.");
            ui.horizontal_wrapped(|ui|{
                for (label,value) in [("Mach",&mut app.aero_panel.mach),("Alpha (deg)",&mut app.aero_panel.alpha_deg),("Beta (deg)",&mut app.aero_panel.beta_deg),("Reynolds",&mut app.aero_panel.reynolds)]{ui.label(label);ui.add(egui::DragValue::new(value).speed(0.01).update_while_editing(false));}
            });
            let query=AeroQuery{mach:app.aero_panel.mach,alpha:app.aero_panel.alpha_deg.to_radians(),beta:app.aero_panel.beta_deg.to_radians(),roll_rate:0.0,pitch_rate:0.0,reynolds:app.aero_panel.reynolds};
            let mut built=rocket_aero::compute(app.document.rocket(),query);
            let force_scale=built.reference_area/data.metadata.reference_area_m2;
            let moment_scale=force_scale*built.reference_length/data.metadata.reference_length_m;
            built.c_a*=force_scale;built.c_y*=force_scale;built.c_n*=force_scale;
            built.c_l*=moment_scale;built.c_m*=moment_scale;built.c_n_yaw*=moment_scale;
            match data.evaluate(query,app.document.rocket().mass_properties().cg_x.as_meters()) {
                Ok(imported)=>{
                    ui.label("Comparison at dry-vehicle CG, normalized to dataset area and length. Static imported data supplies no damping derivatives.");
                    egui::Grid::new("coefficient_comparison").striped(true).show(ui,|ui|{
                        ui.strong("Coefficient");ui.strong("Built-in");ui.strong("Dataset");ui.strong("Difference");ui.end_row();
                        for (name,a,b) in [("CA",built.c_a,imported.c_a),("CY",built.c_y,imported.c_y),("CN",built.c_n,imported.c_n),("Cl",built.c_l,imported.c_l),("Cm",built.c_m,imported.c_m),("Cn",built.c_n_yaw,imported.c_n_yaw)]{ui.label(name);ui.monospace(format!("{a:.6}"));ui.monospace(format!("{b:.6}"));ui.monospace(format!("{:+.6}",b-a));ui.end_row();}
                    });
                }
                Err(error)=>{ui.colored_label(egui::Color32::from_rgb(240,185,105),error.to_string());}
            }
        } else {ui.label("Import a dataset to inspect its provenance, validity envelope, and differences from built-in predictions.");}
        ui.add_space(16.0);ui.separator();ui.heading("CFD study · OpenFOAM");
        ui.label("Steady compressible RANS (k–ω SST) on a Gmsh tetrahedral mesh. One static operating point per job. Source geometry, mesh, settings, logs, and raw coefficients are retained.");
        ui.colored_label(egui::Color32::from_rgb(240,185,105),"Exploratory CFD: a converged solve still needs mesh/domain studies and experimental validation. No inflated boundary-layer mesh is generated yet.");
        if ui.button("Reset CFD settings from vehicle").clicked(){app.aero_panel.settings=Some(CfdSettings::for_vehicle(app.document.rocket()));}
        if app.aero_panel.settings.is_none(){app.aero_panel.settings=Some(CfdSettings::for_vehicle(app.document.rocket()));}
        if let Some(settings)=&mut app.aero_panel.settings {
            ui.checkbox(&mut settings.include_launch_lugs,"Include approximate launch-lug geometry (needs finer mesh)");
            egui::Grid::new("cfd_settings").num_columns(2).show(ui,|ui|{
                for (name,value) in [("Mach (0.05–0.7)",&mut settings.mach),("Alpha (deg)",&mut settings.alpha_deg),("Beta (deg)",&mut settings.beta_deg),("Mesh size near vehicle (m)",&mut settings.mesh_size_m),("Pressure (Pa)",&mut settings.pressure_pa),("Temperature (K)",&mut settings.temperature_k),("Reference area (m²)",&mut settings.reference_area_m2),("Reference length (m)",&mut settings.reference_length_m),("Moment reference station (m)",&mut settings.moment_reference_station_m),("Turbulence intensity",&mut settings.turbulence_intensity)]{ui.label(name);ui.add(egui::DragValue::new(value).speed(0.001).max_decimals(6).update_while_editing(false));ui.end_row();}
                ui.label("Maximum iterations");ui.add(egui::DragValue::new(&mut settings.iterations).range(50..=10_000).update_while_editing(false));ui.end_row();
            });
        }
        ui.collapsing("Installed CFD tools",|ui|{
            ui.label("Python executable with Gmsh 4.15.2 installed");ui.text_edit_singleline(&mut app.aero_panel.python);
            ui.label("OpenFOAM etc/bashrc (tested with v2606)");ui.text_edit_singleline(&mut app.aero_panel.foam_bashrc);
            ui.label("Tool paths are local session settings; generated study inputs contain no executable commands.");
        });
        ui.horizontal_wrapped(|ui|{
            let idle=app.aero_panel.job.is_none();
            if ui.add_enabled(idle,egui::Button::new("Run CFD study…")).clicked(){app.prepare_cfd(true);}
            if ui.add_enabled(idle,egui::Button::new("Export CFD study…")).clicked(){app.prepare_cfd(false);}
            if !idle {ui.spinner();if ui.button("Cancel CFD").clicked(){app.cancel_cfd();}}
        });
        if let Some(path)=&app.aero_panel.artifact{ui.label(format!("Study artifacts: {}",path.display()));}
        if let Some((_,outcome))=&app.aero_panel.completed {
            ui.strong(if outcome.converged{"Solver converged · not independently validated"}else{"Convergence criteria not met · raw coefficients retained"});
            ui.label(format!("{} cells · {} iterations · coefficient drift {:.5}",outcome.mesh_cells,outcome.iterations,outcome.coefficient_relative_change));
            ui.label(format!("CA {:.6} · CN {:.6} · Cm {:.6}",outcome.coefficients.ca,outcome.coefficients.cn,outcome.coefficients.cm));
            if !outcome.converged{ui.label("No activation dataset was created. Review logs and refine the study.");}
        }
        if let Some(error)=&app.aero_panel.error{ui.colored_label(egui::Color32::LIGHT_RED,error);}
    });
}
