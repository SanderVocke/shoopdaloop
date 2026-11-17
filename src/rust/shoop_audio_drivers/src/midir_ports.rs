//! midir ports as MIDI sources and sinks.

use std::any::Any;

use midir::{Ignore, MidiInput, MidiOutput, MidiOutputConnection};

use crate::midi::{
    MidiDevices, MidiInputDevice, MidiInputSender, MidiOutputDevice, MidiSink, MidiSource,
};

struct MidirInput {
    client: String,
    port: String,
}

impl MidiSource for MidirInput {
    fn connect(
        self: Box<Self>,
        mut sender: MidiInputSender,
    ) -> Result<Box<dyn Any + Send>, String> {
        let mut input = MidiInput::new(&self.client).map_err(|error| error.to_string())?;
        input.ignore(Ignore::None);
        let port = input
            .ports()
            .into_iter()
            .find(|port| input.port_name(port).is_ok_and(|name| name == self.port))
            .ok_or_else(|| format!("no MIDI input named {}", self.port))?;
        let connection = input
            .connect(
                &port,
                &format!("{} in", self.client),
                move |_, message, _| {
                    sender.push(message);
                },
                (),
            )
            .map_err(|error| error.to_string())?;
        Ok(Box::new(connection))
    }
}

struct MidirOutput(MidiOutputConnection);

impl MidiSink for MidirOutput {
    fn send(&mut self, message: &[u8]) -> bool {
        self.0.send(message).is_ok()
    }
}

/// Names of the midir input and output ports.
pub fn midi_ports() -> (Vec<String>, Vec<String>) {
    let inputs = MidiInput::new("ShoopDaLoop-list")
        .map(|input| {
            input
                .ports()
                .iter()
                .filter_map(|port| input.port_name(port).ok())
                .collect()
        })
        .unwrap_or_default();
    let outputs = MidiOutput::new("ShoopDaLoop-list")
        .map(|output| {
            output
                .ports()
                .iter()
                .filter_map(|port| output.port_name(port).ok())
                .collect()
        })
        .unwrap_or_default();
    (inputs, outputs)
}

/// Open the named midir ports. Ports that are missing or fail to open are left out
/// with a warning.
pub fn open_midi(
    client: &str,
    inputs: &[String],
    outputs: &[String],
    warnings: &mut Vec<String>,
) -> MidiDevices {
    let (available, _) = if inputs.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        midi_ports()
    };
    let mut devices = MidiDevices::default();
    for name in inputs {
        if available.contains(name) {
            devices.inputs.push(MidiInputDevice {
                name: name.clone(),
                source: Box::new(MidirInput {
                    client: client.into(),
                    port: name.clone(),
                }),
            });
        } else {
            warnings.push(format!("no MIDI input named {name}"));
        }
    }
    for name in outputs {
        match open_output(client, name) {
            Ok(connection) => devices.outputs.push(MidiOutputDevice {
                name: name.clone(),
                sink: Box::new(MidirOutput(connection)),
            }),
            Err(error) => warnings.push(format!("MIDI output {name} not opened: {error}")),
        }
    }
    devices
}

fn open_output(client: &str, name: &str) -> Result<MidiOutputConnection, String> {
    let output = MidiOutput::new(client).map_err(|error| error.to_string())?;
    let port = output
        .ports()
        .into_iter()
        .find(|port| {
            output
                .port_name(port)
                .is_ok_and(|port_name| port_name == name)
        })
        .ok_or_else(|| "no such port".to_string())?;
    output
        .connect(&port, &format!("{client} out"))
        .map_err(|error| error.to_string())
}
