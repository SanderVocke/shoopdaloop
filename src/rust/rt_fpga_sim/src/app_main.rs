#[cfg(not(target_arch = "wasm32"))]
fn main() -> rt_fpga_sim::SimResult<()> {
    use rt_fpga_sim::{
        app::{Fpga, Trace},
        app_worker::RunMode,
    };
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let artifacts =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-app");
    if args
        .first()
        .is_some_and(|arg| arg == "--replay" || arg == "--compare")
    {
        let path = args.get(1).ok_or("--replay requires a trace path")?;
        let trace: Trace = serde_json::from_slice(&std::fs::read(path)?)?;
        let wave = match args.len() {
            2 => None,
            4 if args[2] == "--vcd" => Some(std::path::PathBuf::from(&args[3])),
            _ => {
                return Err(
                    "usage: fpga-app-demo --replay|--compare TRACE.json [--vcd PATH]".into(),
                );
            }
        };
        let mut fpga = Fpga::with_trace(artifacts, RunMode::Stepped, wave)?;
        if args[0] == "--compare" {
            let report = rt_fpga_sim::app::compare(&mut fpga, &trace)?;
            println!("{report:?}");
            println!(
                "readiness: {:?}; compiler: {:?}; simulation: {:?}; store: {:?}",
                fpga.readiness,
                fpga.hardware.metrics(),
                fpga.hardware.simulation_metrics(),
                fpga.hardware.store_usage()
            );
            return Ok(());
        }
        let states = fpga.replay(&trace)?;
        println!(
            "{}: {} actions, {} frames; final={:?}; {:?}",
            trace.name,
            states.len(),
            fpga.hardware.frame(),
            fpga.app.loop_state(),
            fpga.hardware.metrics()
        );
        println!("store: {:?}", fpga.hardware.store_usage());
        println!(
            "readiness: {:?}; simulation: {:?}",
            fpga.readiness,
            fpga.hardware.simulation_metrics()
        );
        return Ok(());
    }
    if !args.is_empty() {
        return Err("usage: fpga-app-demo [--replay TRACE.json]".into());
    }
    #[cfg(feature = "app-ui")]
    {
        ui::run(artifacts)
    }
    #[cfg(not(feature = "app-ui"))]
    {
        Err("enable app-ui for the interactive application, or use --replay TRACE.json".into())
    }
}
#[cfg(target_arch = "wasm32")]
fn main() {
    eprintln!("The Marlin application demonstrator is native-only.");
}

#[cfg(all(feature = "app-ui", not(target_arch = "wasm32")))]
mod ui {
    use eframe::egui;
    use rt_fpga_sim::{app::Fpga, app_worker::RunMode};
    use std::{path::PathBuf, sync::Arc, time::Duration};
    struct Demo {
        fpga: Fpga,
        widget: shoop_egui::AppWidget,
        settings: shoop_settings::SettingsViewState,
        error: Option<String>,
        last: std::time::Instant,
        signal: bool,
    }
    impl eframe::App for Demo {
        fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
            let now = std::time::Instant::now();
            let elapsed = now.duration_since(self.last);
            self.last = now;
            self.fpga.app.pump_elapsed(elapsed);
            if let Some(error) = self.fpga.failure.lock().unwrap().as_ref() {
                self.error = Some(error.clone());
            }
            ui.horizontal(|ui| {
                ui.label("Offline simulation · no audio devices");
                if ui.checkbox(&mut self.signal, "Synthetic input").changed() {
                    if let Err(error) = self.fpga.hardware.signal(self.signal) {
                        self.error = Some(error.to_string());
                    }
                }
                ui.label(format!("{} frames", self.fpga.hardware.frame()));
            });
            if let Some(error) = &self.error {
                ui.colored_label(egui::Color32::RED, error);
            }
            let response = self
                .widget
                .show(ui, &self.fpga.app.snapshot(), &self.settings, None);
            for intent in response.app_actions {
                if let Err(error) = self.fpga.app.client.dispatch(intent) {
                    self.error = Some(format!("{error:?}"));
                }
            }
            if !response.settings_actions.is_empty() {
                self.error = Some("Settings are fixed for this offline demonstration.".into());
            }
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }
    }
    pub fn run(artifacts: PathBuf) -> rt_fpga_sim::SimResult<()> {
        let fpga = Fpga::new(artifacts, RunMode::Continuous)?;
        fpga.hardware.signal(true)?;
        let mut builder = shoop_settings::SettingsRegistryBuilder::default();
        shoop_egui::register_settings(&mut builder)?;
        let registry = Arc::new(builder.finish());
        let settings = shoop_settings::SettingsViewState {
            active: Arc::new(registry.defaults(1)),
            diagnostics: Arc::from([]),
            storage_location: "offline preset".into(),
            recovery_required: false,
            persistence: shoop_settings::SettingsPersistenceState::Idle,
        };
        let demo = Demo {
            fpga,
            widget: shoop_egui::AppWidget::new(registry),
            settings,
            error: None,
            last: std::time::Instant::now(),
            signal: true,
        };
        eframe::run_native(
            "ShoopDaLoop · FPGA offline",
            eframe::NativeOptions::default(),
            Box::new(move |context| {
                shoop_egui::initialize(&context.egui_ctx);
                Ok(Box::new(demo))
            }),
        )
        .map_err(|error| error.to_string().into())
    }
}
