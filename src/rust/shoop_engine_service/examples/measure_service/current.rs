use super::host::{Admission, Bench, Counters, Mode, FRAMES, LENGTH};
use shoop_engine::{
    engine::{self, Engine, EngineHandle},
    external_audio_port::ExternalAudioPort,
    external_midi_port::ExternalMidiPort,
    port::PortDirection,
    session::{Port, Session},
    state_mirror::AudioPortStateMirror,
    ChannelMode, LoopMode,
};
use shoop_wasm_test_support::heap;
use std::{hint::black_box, sync::Arc};

pub struct Current {
    engine: Engine,
    handle: EngineHandle,
    inputs: [usize; 2],
    outputs: [usize; 2],
    midi_output: usize,
    channels: [usize; 2],
    meters: [Arc<AudioPortStateMirror>; 2],
    left: [f32; FRAMES],
    right: [f32; FRAMES],
    frame: u64,
    mode: Mode,
}
impl Current {
    pub fn new(mode: Mode) -> Self {
        assert!(matches!(mode, Mode::Idle | Mode::Steady));
        let mut session = Session::default();
        session.set_sample_rate(48_000);
        session.set_buffer_size(FRAMES as u32);
        session.set_loop_smoothing_ms(0);
        let loop_id = session.create_loop();
        let inputs = std::array::from_fn(|channel| {
            let mut input =
                ExternalAudioPort::new(format!("in-{channel}"), PortDirection::Input, 0);
            input.stage_input(&[if channel == 0 { 0.25 } else { 0.5 }; FRAMES]);
            session.add_port(Port::External(input))
        });
        let meters = std::array::from_fn(|_| Arc::new(AudioPortStateMirror::default()));
        let outputs = std::array::from_fn(|channel| {
            session
                .add_audio_port_with_state(
                    Port::External(ExternalAudioPort::new(
                        format!("out-{channel}"),
                        PortDirection::Output,
                        0,
                    )),
                    Arc::clone(&meters[channel]),
                )
                .unwrap()
        });
        let channels = std::array::from_fn(|channel| {
            let id = session
                .add_audio_channel_with_bounded_capacity(loop_id, 256, LENGTH, ChannelMode::Direct)
                .unwrap();
            session.audio_channel_mut(id).unwrap().load_data(&vec![
                if channel == 0 {
                    0.5
                } else {
                    1.
                };
                LENGTH
            ]);
            session.connect_channel_input(id, inputs[channel]).unwrap();
            session
                .connect_channel_output(id, outputs[channel])
                .unwrap();
            session
                .connect_ports_internal(inputs[channel], outputs[channel])
                .unwrap();
            id
        });
        let midi_input = session.add_port(Port::ExternalMidi(ExternalMidiPort::new(
            "midi-in",
            PortDirection::Input,
        )));
        let midi_output = session.add_port(Port::ExternalMidi(ExternalMidiPort::new(
            "midi-out",
            PortDirection::Output,
        )));
        let midi = session
            .add_midi_channel(loop_id, 2048, ChannelMode::Direct)
            .unwrap();
        session
            .midi_channel_mut(midi)
            .unwrap()
            .set_length(LENGTH as u32);
        session.connect_channel_input(midi, midi_input).unwrap();
        session.connect_channel_output(midi, midi_output).unwrap();
        session
            .connect_ports_internal(midi_input, midi_output)
            .unwrap();
        let loop_ = session.loop_mut(loop_id).unwrap();
        loop_.set_length(LENGTH as u32);
        loop_.set_mode(if mode == Mode::Steady {
            LoopMode::Playing
        } else {
            LoopMode::Stopped
        });
        session.apply_graph_changes().unwrap();
        let (engine, handle) = engine::split(session, 16);
        Self {
            engine,
            handle,
            inputs,
            outputs,
            midi_output,
            channels,
            meters,
            left: [0.; FRAMES],
            right: [0.; FRAMES],
            frame: 0,
            mode,
        }
    }
}
impl Bench for Current {
    fn frame(&self) -> u64 {
        self.frame
    }
    fn control(&mut self) -> Admission {
        Admission::default()
    }
    fn process(&mut self) -> Counters {
        for channel in 0..2 {
            self.engine
                .session_mut()
                .port_mut(self.inputs[channel])
                .unwrap()
                .as_external_mut()
                .unwrap()
                .stage_input(&[if channel == 0 { 0.25 } else { 0.5 }; FRAMES]);
        }
        self.engine
            .session_mut()
            .port_mut(self.midi_output)
            .unwrap()
            .as_external_midi_mut()
            .unwrap()
            .request_output();
        self.engine.process(FRAMES);
        self.left.copy_from_slice(
            self.engine
                .session()
                .port(self.outputs[0])
                .unwrap()
                .as_external()
                .unwrap()
                .output(FRAMES),
        );
        self.right.copy_from_slice(
            self.engine
                .session()
                .port(self.outputs[1])
                .unwrap()
                .as_external()
                .unwrap()
                .output(FRAMES),
        );
        let midi = self
            .engine
            .session()
            .port(self.midi_output)
            .unwrap()
            .as_external_midi()
            .unwrap()
            .outgoing()
            .len();
        self.frame += FRAMES as u64;
        Counters {
            midi,
            ..Counters::default()
        }
    }
    fn collect(&mut self, _work: &mut Counters) {
        self.handle.reclaim();
    }
    fn meters(&mut self) {
        for meter in &self.meters {
            black_box(heap::assert_no_heap(|| meter.read(String::new())));
        }
    }
    fn check(&self, _frame: u64) {
        let expected = if self.mode == Mode::Steady {
            [0.75, 1.5]
        } else {
            [0.25, 0.5]
        };
        assert_eq!(self.left, [expected[0]; FRAMES]);
        assert_eq!(self.right, [expected[1]; FRAMES]);
        assert_eq!(self.engine.session().n_stale_cycles(), 0);
        assert_eq!(self.engine.session().n_stuck_cycles(), 0);
    }
    fn finish(mut self) {
        let lengths = self
            .channels
            .map(|id| self.engine.session().audio_channel(id).unwrap().length());
        let remaining = self.channels.map(|id| {
            self.engine
                .session()
                .audio_channel(id)
                .unwrap()
                .storage_remaining()
        });
        eprintln!("meta current_channel_samples={lengths:?} current_storage_remaining={remaining:?} current_subblocks={}", self.engine.session().n_sub_blocks_last_cycle());
        self.engine
            .session_mut()
            .loop_mut(0)
            .unwrap()
            .set_mode(LoopMode::Stopped);
        heap::assert_no_heap(|| self.process());
        self.handle.reclaim();
    }
}
