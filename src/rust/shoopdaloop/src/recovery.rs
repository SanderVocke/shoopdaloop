use crate::application::ApplicationHandle;
use eframe::egui;
use shoop_app_protocol::{ClientMessage, ManagementReply, ManagementRequest, Outcome};
use shoop_host::{connection, remote::Target as HostEntry};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

enum Event {
    Reserved(ClientMessage),
    Finished {
        result: Result<(), String>,
        definitive: bool,
    },
}

#[derive(Default)]
pub struct ExitController {
    pub open: bool,
    authorized: bool,
    ticket: Option<ClientMessage>,
    receiver: Option<mpsc::Receiver<Event>>,
    error: Option<String>,
}
impl ExitController {
    pub fn is_pending(&self) -> bool {
        self.receiver.is_some()
    }
    pub fn intercept_close(&mut self, context: &egui::Context) {
        if !self.authorized && context.input(|input| input.viewport().close_requested()) {
            context.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.open = true;
        }
    }
    pub fn close_ui(&mut self, context: &egui::Context) {
        self.authorized = true;
        context.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    pub(crate) fn start(&mut self, entry: impl Into<HostEntry>, handle: ApplicationHandle) {
        let entry = entry.into();
        self.error = None;
        let ticket = self.ticket.clone();
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        std::thread::spawn(move || {
            let ticket = match ticket.map(Ok).unwrap_or_else(|| handle.prepare_shutdown()) {
                Ok(ticket) => ticket,
                Err(error) => {
                    let _ = sender.send(Event::Finished {
                        result: Err(error.to_string()),
                        definitive: true,
                    });
                    return;
                }
            };
            let _ = sender.send(Event::Reserved(ticket.clone()));
            let (result, definitive) = shutdown(&entry, ticket);
            let _ = sender.send(Event::Finished { result, definitive });
        });
    }
    pub fn show(&mut self, context: &egui::Context, entry: &HostEntry, handle: &ApplicationHandle) {
        if let Some(receiver) = self.receiver.take() {
            let mut finished = false;
            while let Ok(event) = receiver.try_recv() {
                match event {
                    Event::Reserved(ticket) => self.ticket = Some(ticket),
                    Event::Finished { result, definitive } => {
                        finished = true;
                        if definitive {
                            self.ticket = None;
                        }
                        match result {
                            Ok(()) => self.close_ui(context),
                            Err(error) => self.error = Some(error),
                        }
                    }
                }
            }
            if !finished {
                self.receiver = Some(receiver);
            }
        }
        if !self.open {
            return;
        }
        egui::Window::new("Close session UI")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("Close this UI, or stop the session and close its host too.");
                if entry.transport == shoop_host::registry::AppTransport::SharedMemory {
                    ui.label("Shared-memory mode cannot reattach after this UI exits.");
                }
                if let Some(error) = &self.error {
                    ui.colored_label(egui::Color32::YELLOW, error);
                }
                if self.receiver.is_some() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Waiting for host reset and cleanup…");
                    });
                }
                if ui.button("Close UI only (keep host running)").clicked() {
                    self.close_ui(context);
                }
                if ui
                    .add_enabled(
                        self.receiver.is_none(),
                        egui::Button::new(if self.error.is_some() {
                            "Retry closing UI and host"
                        } else {
                            "Close UI and host"
                        }),
                    )
                    .clicked()
                {
                    self.start(entry.clone(), handle.clone());
                }
                if ui
                    .add_enabled(self.receiver.is_none(), egui::Button::new("Cancel"))
                    .clicked()
                {
                    self.open = false;
                    handle.resume_controls();
                }
            });
        context.request_repaint_after(Duration::from_millis(50));
    }
}

pub(crate) fn shutdown(
    entry: &impl shoop_host::remote::ConnectionTarget,
    ticket: ClientMessage,
) -> (Result<(), String>, bool) {
    let ClientMessage::Shutdown {
        authority,
        operation,
    } = ticket
    else {
        return (Err("Invalid shutdown reservation".into()), true);
    };
    let mut request = ManagementRequest::Shutdown {
        authority,
        operation,
    };
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        match connection::management(entry, request, Duration::from_secs(2)) {
            Ok(ManagementReply::Finished {
                authority: reply_authority,
                result,
            }) if reply_authority == authority && result.id == operation => {
                return (
                    match result.outcome {
                        Outcome::Confirmed { .. } => Ok(()),
                        outcome => Err(format!("Host cleanup failed: {outcome:?}")),
                    },
                    true,
                );
            }
            Ok(ManagementReply::Accepted {
                authority: reply_authority,
                operation: pending,
            }) if reply_authority == authority && pending.id == operation => {}
            Ok(ManagementReply::Refused(reason)) => {
                return (Err(format!("Host refused shutdown: {reason:?}")), true)
            }
            Ok(_) => {
                return (
                    Err("Host returned an invalid shutdown response".into()),
                    false,
                )
            }
            Err(error) => {
                return (
                    Err(format!(
                    "Could not confirm host shutdown: {error}. Retry checks the same operation."
                )),
                    false,
                )
            }
        }
        if Instant::now() >= deadline {
            return (Err("Host cleanup has not completed. Retry to check its result, or close the UI only.".into()), false);
        }
        request = ManagementRequest::Result {
            authority,
            operation,
        };
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[shoop_wasm_test_support::shoop_test]
    fn platform_close_prompts_until_the_user_authorizes_exit() {
        let context = egui::Context::default();
        let mut exit = ExitController::default();
        let input = || egui::RawInput {
            viewports: [(
                egui::ViewportId::ROOT,
                egui::ViewportInfo {
                    events: vec![egui::ViewportEvent::Close],
                    ..Default::default()
                },
            )]
            .into_iter()
            .collect(),
            ..Default::default()
        };
        let mut output = context.run_ui(input(), |ui| exit.intercept_close(ui.ctx()));
        assert!(exit.open);
        assert!(output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::CancelClose));
        output.textures_delta.clear();
        exit.open = false;
        assert!(!exit.authorized);
        let mut output = context.run_ui(Default::default(), |ui| exit.close_ui(ui.ctx()));
        output.textures_delta.clear();
        let mut output = context.run_ui(input(), |ui| exit.intercept_close(ui.ctx()));
        assert!(!output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::CancelClose));
        output.textures_delta.clear();
    }
}

#[derive(Default)]
pub struct HostPanel {
    pub open: bool,
    report: Option<shoop_app_protocol::HostReport>,
    receiver: Option<mpsc::Receiver<Result<shoop_app_protocol::HostReport, String>>>,
    error: Option<String>,
}
impl HostPanel {
    fn refresh(&mut self, entry: HostEntry) {
        if self.receiver.is_some() {
            return;
        }
        self.report = None;
        self.error = None;
        let (sender, receiver) = mpsc::sync_channel(1);
        self.receiver = Some(receiver);
        std::thread::spawn(move || {
            let _ = sender.send(
                connection::host_report(&entry, Duration::from_secs(5))
                    .map_err(|error| error.to_string()),
            );
        });
    }
    pub fn show(&mut self, context: &egui::Context, entry: &HostEntry) {
        if !self.open {
            return;
        }
        if self.report.is_none() && self.receiver.is_none() && self.error.is_none() {
            self.refresh(entry.clone());
        }
        if let Some(receiver) = self.receiver.take() {
            match receiver.try_recv() {
                Ok(Ok(report)) => self.report = Some(report),
                Ok(Err(error)) => self.error = Some(error),
                Err(mpsc::TryRecvError::Empty) => self.receiver = Some(receiver),
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.error = Some("Host query ended without a reply".into())
                }
            }
        }
        let mut open = self.open;
        egui::Window::new("Connected host").open(&mut open).show(context, |ui| {
            ui.label(format!("Endpoint: {}", entry.endpoint));
            ui.label("Last queried host state. Refresh to query the endpoint again.");
            if let Some(report) = &self.report {
                ui.label(format!("Ready: {}; controller connected: {}; closed: {}", report.ready, report.busy, report.closed));
                ui.label(format!("Current smoothing: {} ms; Master auto-connect: {}", report.loop_smoothing_ms, report.master_auto_connect));
                ui.label(format!("Audio state: {:?}", report.effective.audio_driver));
                ui.label(format!("Sample rate: {} Hz; buffer: {} frames", report.effective.sample_rate, report.effective.buffer_size));
                ui.label(format!("Host operations in progress: {}", report.pending_operations));
                if let Some(settings) = &report.requested {
                    ui.separator();
                    ui.label(format!("Configured driver: {}", settings.driver.kind().id()));
                    ui.label(format!("Capacity: {} tracks, {} loops, {} channels, {} MiB", settings.pools.tracks, settings.pools.loops, settings.pools.channels, settings.pools.loop_memory_mb));
                    ui.label(format!("Startup smoothing: {} ms", settings.loop_smoothing_ms));
                    ui.collapsing("Startup configuration", |ui| {
                        if let Ok(json) = serde_json::to_string_pretty(settings) { ui.monospace(json); }
                    });
                }
                for warning in &report.diagnostics { ui.colored_label(egui::Color32::YELLOW, warning); }
                ui.separator();
                ui.label("Driver and capacity changes require a host restart. Local preferences are used when launching a local host.");
            }
            if let Some(error) = &self.error { ui.colored_label(egui::Color32::YELLOW, error); }
            if self.receiver.is_some() { ui.spinner(); }
            if ui.add_enabled(self.receiver.is_none(), egui::Button::new("Refresh")).clicked() { self.refresh(entry.clone()); }
        });
        self.open = open;
        if self.receiver.is_some() {
            context.request_repaint_after(Duration::from_millis(50));
        }
    }
}
