use eframe::egui;
use shoop_app_protocol::{BackendRestartPhase as Phase, BackendRestartStatus};
use shoop_host::remote::Target as HostEntry;
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

#[cfg(test)]
mod tests;

enum Event {
    Support(Result<Option<BackendRestartStatus>, String>),
    Progress(BackendRestartStatus),
    Finished(Result<(), String>),
}

#[derive(Default)]
pub(crate) struct BackendRestart {
    pub open: bool,
    pub available: bool,
    pub pending: bool,
    endpoint: Option<String>,
    last_probe: Option<Instant>,
    receiver: Option<mpsc::Receiver<Event>>,
    phase: Option<Phase>,
    error: Option<String>,
    generation: Option<u64>,
    intent_generation: Option<u64>,
}

impl BackendRestart {
    pub fn notice(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn show(&mut self, context: &egui::Context, entry: &HostEntry) -> bool {
        if self.endpoint.as_deref() != Some(&entry.management) && !self.pending {
            *self = Self {
                endpoint: Some(entry.management.clone()),
                ..Default::default()
            };
        }
        if self.poll() {
            return true;
        }
        self.probe(entry);
        if self.open {
            self.confirm(context, entry);
        }
        false
    }

    fn poll(&mut self) -> bool {
        let mut completed = false;
        if let Some(receiver) = self.receiver.take() {
            let mut finished = false;
            loop {
                let event = match receiver.try_recv() {
                    Ok(event) => event,
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        if !finished {
                            self.pending = false;
                            self.available = false;
                            self.error = Some("Recovery worker stopped; query management status before requesting another restart".into());
                            self.last_probe = None;
                        }
                        finished = true;
                        break;
                    }
                };
                match event {
                    Event::Support(result) => {
                        finished = true;
                        self.available = matches!(result, Ok(Some(status)) if matches!(status.phase, Phase::Idle | Phase::Ready));
                        match result {
                            Ok(Some(status)) => {
                                self.generation = Some(status.generation);
                                completed = status.phase == Phase::Ready
                                    && self
                                        .intent_generation
                                        .is_some_and(|generation| status.generation > generation);
                                if status.phase == Phase::Failed {
                                    self.error = Some(format!(
                                        "Backend restart failed: {:?}; full device reboot required",
                                        status.failure
                                    ));
                                }
                            }
                            Err(error) => self.error = Some(error),
                            Ok(None) => {}
                        }
                    }
                    Event::Progress(status) => self.phase = Some(status.phase),
                    Event::Finished(result) => {
                        finished = true;
                        self.pending = false;
                        match result {
                            Ok(()) => {
                                self.open = false;
                                completed = true;
                            }
                            Err(error) => {
                                self.available = false;
                                self.error = Some(error);
                            }
                        }
                    }
                }
            }
            if !finished {
                self.receiver = Some(receiver);
            }
        }
        completed
    }

    fn probe(&mut self, entry: &HostEntry) {
        if self.receiver.is_none()
            && !self.pending
            && self
                .last_probe
                .is_none_or(|last| last.elapsed() >= Duration::from_secs(5))
        {
            let entry = entry.clone();
            let (sender, receiver) = mpsc::channel();
            self.receiver = Some(receiver);
            self.last_probe = Some(Instant::now());
            std::thread::spawn(move || {
                let result =
                    shoop_host::connection::backend_restart_status(&entry, Duration::from_secs(3))
                        .map_err(|error| error.to_string());
                let _ = sender.send(Event::Support(result));
            });
        }
    }

    fn confirm(&mut self, context: &egui::Context, entry: &HostEntry) {
        egui::Window::new("Restart backend?").collapsible(false).resizable(false)
            .show(context, |ui| {
                ui.strong(&entry.name);
                ui.label("Audio will stop. The current backend session and unsaved recordings will be discarded.");
                ui.label("The ARM system, network and thermal protection remain running. This does not reboot the device.");
                if let Some(error) = &self.error { ui.colored_label(egui::Color32::YELLOW, error); }
                if self.pending {
                    ui.spinner();
                    ui.label(format!("Restarting backend: {:?}", self.phase));
                }
                let enabled = self.available && !self.pending && self.receiver.is_none();
                if ui.add_enabled(enabled, egui::Button::new("Restart backend — discard session")).clicked() {
                    self.start(entry.clone());
                }
                if ui.add_enabled(!self.pending, egui::Button::new("Cancel")).clicked() { self.open = false; }
            });
        context.request_repaint_after(Duration::from_millis(50));
    }

    fn start(&mut self, entry: HostEntry) {
        self.pending = true;
        self.intent_generation = self.generation;
        self.error = None;
        self.phase = None;
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        std::thread::spawn(move || {
            let result = shoop_host::connection::restart_backend(
                &entry,
                Duration::from_secs(30),
                |status| {
                    let _ = sender.send(Event::Progress(status));
                },
            )
            .map_err(|error| format!("{error:#}"));
            let _ = sender.send(Event::Finished(result));
        });
    }
}
