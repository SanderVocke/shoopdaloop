use shoop_app_api::{RemoteFxReconnect, RemoteFxTarget};
use std::net::SocketAddr;

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
    addresses: Vec<SocketAddr>,
}

impl Picker {
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn set_discovery(&mut self, discover: fn() -> Vec<SocketAddr>) {
        self.discover = Some(discover);
    }

    pub(crate) fn opened(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.started = false;
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

    pub(crate) fn show(&mut self, ui: &mut egui::Ui, local_session: bool) {
        #[cfg(not(target_arch = "wasm32"))]
        self.poll(ui);
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
            ui.text_edit_singleline(&mut self.address);
        });
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

    #[cfg(not(target_arch = "wasm32"))]
    fn poll(&mut self, ui: &mut egui::Ui) {
        if let Some(pending) = &self.pending {
            match pending.try_recv() {
                Ok(addresses) => {
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
