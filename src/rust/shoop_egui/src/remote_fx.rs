use shoop_app_api::{RemoteFxReconnect, RemoteFxTarget};
use std::net::SocketAddr;

use crate::{HostLocation, HostUiState};

#[cfg(not(target_arch = "wasm32"))]
use std::{sync::mpsc, time::Duration};

#[cfg(not(target_arch = "wasm32"))]
struct DesktopLauncher {
    prepare: fn(&str) -> Result<SocketAddr, String>,
    pending: Option<mpsc::Receiver<Result<SocketAddr, String>>>,
    error: Option<String>,
    #[cfg(test)]
    button_rect: Option<egui::Rect>,
}

#[cfg(not(target_arch = "wasm32"))]
impl DesktopLauncher {
    fn show(&mut self, ui: &mut egui::Ui, host: &HostUiState, address: &mut String) {
        if let Some(pending) = &self.pending {
            match pending.try_recv() {
                Ok(result) => {
                    self.pending = None;
                    match result {
                        Ok(endpoint) => *address = endpoint.to_string(),
                        Err(error) => self.error = Some(error),
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending = None;
                    self.error = Some("FX host startup was interrupted. Try again.".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        let endpoint = host
            .endpoint
            .as_ref()
            .filter(|_| host.session_controls_enabled());
        if self.pending.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Preparing FX host…");
            });
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        } else {
            let start = ui
                .add_enabled(endpoint.is_some(), egui::Button::new("Run FX on this computer"))
                .on_hover_text("Start or reuse an FX host on the desktop's network interface for this session.");
            #[cfg(test)]
            {
                self.button_rect = Some(start.rect);
            }
            if let Some(endpoint) = endpoint.filter(|_| start.clicked()).cloned() {
                let (reply, pending) = mpsc::channel();
                let prepare = self.prepare;
                let context = ui.ctx().clone();
                self.pending = Some(pending);
                self.error = None;
                std::thread::spawn(move || {
                    let _ = reply.send(prepare(&endpoint));
                    context.request_repaint();
                });
            }
        }
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
    }

    fn reset(&mut self) {
        self.pending = None;
        self.error = None;
    }
}

#[derive(Default)]
pub(crate) struct Picker {
    address: String,
    reconnect: RemoteFxReconnect,
    dry_on_loss: bool,
    #[cfg(not(target_arch = "wasm32"))]
    discover: Option<fn() -> Vec<SocketAddr>>,
    #[cfg(not(target_arch = "wasm32"))]
    pending: Option<std::sync::mpsc::Receiver<Vec<SocketAddr>>>,
    #[cfg(not(target_arch = "wasm32"))]
    started: bool,
    #[cfg(not(target_arch = "wasm32"))]
    local: Option<DesktopLauncher>,
    addresses: Vec<SocketAddr>,
}

impl Picker {
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn set_discovery(&mut self, discover: fn() -> Vec<SocketAddr>) {
        self.discover = Some(discover);
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn set_launcher(&mut self, prepare: fn(&str) -> Result<SocketAddr, String>) {
        self.local = Some(DesktopLauncher {
            prepare,
            pending: None,
            error: None,
            #[cfg(test)]
            button_rect: None,
        });
    }

    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub(crate) fn launch_button_rect(&self) -> Option<egui::Rect> {
        self.local.as_ref()?.button_rect
    }

    pub(crate) fn is_preparing(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.local
                .as_ref()
                .is_some_and(|local| local.pending.is_some())
        }
        #[cfg(target_arch = "wasm32")]
        {
            false
        }
    }

    pub(crate) fn opened(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.started = false;
        }
        self.closed();
    }

    pub(crate) fn closed(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(local) = &mut self.local {
            local.reset();
        }
    }

    pub(crate) fn target(&self) -> Option<RemoteFxTarget> {
        let target = RemoteFxTarget {
            address: self.address.trim().parse().ok()?,
            reconnect: self.reconnect,
            dry_on_loss: self.dry_on_loss,
        };
        target.valid().then_some(target)
    }

    pub(crate) fn uses_host_service(&self, has_default: bool) -> bool {
        has_default && self.address.trim().is_empty()
    }

    pub(crate) fn clear_choice(&mut self) {
        self.address.clear();
        self.closed();
    }

    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        host: Option<&HostUiState>,
        has_default: bool,
    ) {
        let local_session = host.is_none_or(|host| host.location != HostLocation::RemoteSocket);
        #[cfg(not(target_arch = "wasm32"))]
        self.poll(ui, local_session, has_default);
        #[cfg(not(target_arch = "wasm32"))]
        if let (Some(local), Some(host)) = (&mut self.local, host.filter(|_| !local_session)) {
            local.show(ui, host, &mut self.address);
        }
        ui.add_enabled_ui(!self.is_preparing(), |ui| {
            self.show_address(ui, local_session, has_default)
        });
        if self.uses_host_service(has_default) {
            return;
        }
        let policies = [
            (RemoteFxReconnect::Recreate, "Recreate chain"),
            (RemoteFxReconnect::Resume, "Resume live chain"),
            (RemoteFxReconnect::Manual, "Retry manually"),
        ];
        ui.horizontal(|ui| {
            ui.label("On disconnect");
            let selected = policies
                .iter()
                .find(|(policy, _)| *policy == self.reconnect)
                .map(|(_, label)| *label)
                .unwrap_or_default();
            egui::ComboBox::from_id_salt("remote_fx_reconnect")
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for (value, label) in policies {
                        ui.selectable_value(&mut self.reconnect, value, label);
                    }
                });
        });
        ui.checkbox(
            &mut self.dry_on_loss,
            "Play delayed dry audio when FX is missing",
        );
    }

    fn show_address(&mut self, ui: &mut egui::Ui, local_session: bool, has_default: bool) {
        let addresses = self
            .addresses
            .iter()
            .filter(|address| local_session || !address.ip().is_loopback());
        if addresses.clone().next().is_some() {
            egui::ComboBox::from_id_salt("remote_fx_services")
                .selected_text("Discovered FX services")
                .show_ui(ui, |ui| {
                    for address in addresses {
                        let address = address.to_string();
                        if ui
                            .selectable_label(self.address == address, &address)
                            .clicked()
                        {
                            self.address = address;
                        }
                    }
                });
        }
        ui.horizontal(|ui| {
            ui.label("FX service address");
            let hint = if has_default {
                "Use the host's FX service"
            } else {
                "IP address and port"
            };
            ui.add(egui::TextEdit::singleline(&mut self.address).hint_text(hint));
            if has_default && !self.address.is_empty() && ui.button("Use host service").clicked() {
                self.address.clear();
            }
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn poll(&mut self, ui: &mut egui::Ui, local_session: bool, has_default: bool) {
        if let Some(pending) = &self.pending {
            match pending.try_recv() {
                Ok(addresses) => {
                    if !has_default && self.address.is_empty() {
                        let mut reachable = addresses
                            .iter()
                            .filter(|address| local_session || !address.ip().is_loopback());
                        if let (Some(address), None) = (reachable.next(), reachable.next()) {
                            self.address = address.to_string();
                        }
                    }
                    self.addresses = addresses;
                    self.pending = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.pending = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(discover) = self.discover {
            if self.pending.is_some() {
                ui.label("Looking for FX services…");
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(100));
            } else if !self.started || ui.button("Refresh FX services").clicked() {
                self.started = true;
                let (reply, pending) = std::sync::mpsc::channel();
                let context = ui.ctx().clone();
                std::thread::spawn(move || {
                    let _ = reply.send(discover());
                    context.request_repaint();
                });
                self.pending = Some(pending);
            }
        }
    }
}
