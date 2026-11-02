use crate::{api::ChainType, CarlaMainThreadUiDispatcher, CarlaNativeHost};
use shoop_dsp::{Block, Config, Error, Info, Latency, MidiEvent, Processor};

pub struct CarlaDsp {
    host: CarlaNativeHost,
    config: Config,
    kind: ChainType,
    state: Option<String>,
    dispatcher: Option<CarlaMainThreadUiDispatcher>,
}
impl CarlaDsp {
    pub fn prepare(
        config: Config,
        kind: ChainType,
        state: Option<&str>,
        dispatcher: Option<CarlaMainThreadUiDispatcher>,
    ) -> anyhow::Result<Self> {
        config.validate()?;
        let mut host = match &dispatcher {
            Some(dispatcher) => CarlaNativeHost::instantiate_with_ui_dispatcher(
                kind,
                config.sample_rate,
                config.frames as u32,
                dispatcher.clone(),
            )?,
            None => CarlaNativeHost::instantiate(kind, config.sample_rate, config.frames as u32)?,
        };
        anyhow::ensure!(
            host.info().audio_inputs == config.channels,
            "Carla channel shape mismatch"
        );
        if let Some(state) = state {
            host.restore_state(state)?;
        }
        host.set_active(true);
        Ok(Self {
            host,
            config,
            kind,
            state: state.map(str::to_owned),
            dispatcher,
        })
    }
}
impl Processor for CarlaDsp {
    fn info(&self) -> Info {
        Info {
            config: self.config,
            latency: Latency::Unknown,
            midi: true,
            state: true,
            ui: self.dispatcher.is_some(),
        }
    }
    fn process(&mut self, block: Block<'_>) -> Result<(), Error> {
        block.validate(self.config)?;
        if block.midi_input.len() > crate::CARLA_MIDI_BUFFER_CAPACITY {
            return Err(Error::Capacity);
        }
        let mut events = [(0, &[][..]); crate::CARLA_MIDI_BUFFER_CAPACITY];
        for (slot, event) in events.iter_mut().zip(block.midi_input) {
            if event.len > 4 {
                return Err(Error::Unsupported);
            }
            *slot = (event.frame, event.bytes()?);
        }
        for c in 0..self.config.channels {
            self.host.audio_input_mut(c).ok_or(Error::Shape)?[..block.frames]
                .copy_from_slice(&block.input[c * block.frames..(c + 1) * block.frames]);
        }
        self.host
            .set_source_time(block.frame, block.transport.playing);
        self.host
            .set_midi_input_events(0, &events[..block.midi_input.len()])
            .map_err(|_| Error::Algorithm)?;
        self.host
            .process(block.frames)
            .map_err(|_| Error::Algorithm)?;
        for c in 0..self.config.channels {
            block.output[c * block.frames..(c + 1) * block.frames]
                .copy_from_slice(&self.host.audio_output(c).ok_or(Error::Shape)?[..block.frames]);
        }
        block.midi_output.clear();
        self.host
            .visit_midi_output(0, |frame, bytes| {
                block.midi_output.push(MidiEvent::new(frame, 0, bytes)?)?;
                Ok(())
            })
            .map_err(|_| Error::Capacity)
    }
    fn reset(&mut self) -> Result<(), Error> {
        let replacement = Self::prepare(
            self.config,
            self.kind,
            self.state.as_deref(),
            self.dispatcher.clone(),
        )
        .map_err(|_| Error::Algorithm)?;
        *self = replacement;
        Ok(())
    }
    fn save_state(&mut self) -> Result<Vec<u8>, Error> {
        self.host
            .save_state()
            .map(String::into_bytes)
            .map_err(|_| Error::Algorithm)
    }
    fn restore_state(&mut self, state: &[u8]) -> Result<(), Error> {
        let state = core::str::from_utf8(state).map_err(|_| Error::Shape)?;
        self.host.set_active(false);
        let result = self.host.restore_state(state).map_err(|_| Error::Algorithm);
        self.host.set_active(true);
        result?;
        self.state = Some(state.to_owned());
        Ok(())
    }
    fn set_visible(&mut self, visible: bool) -> Result<(), Error> {
        self.host.set_visible(visible).map_err(|_| Error::Algorithm)
    }
}
