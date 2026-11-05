//! Host-only preparation. No factory or DSP constructor runs after callbacks start.
use rt_buffered::{Backend, BufferLimits, MidiLimits, Timing};
use rt_runtime::{graph::PortCounts, Quantum};
use shoop_rt_engine::{EngineDescription, PreparedProcessor, PreparedProcessors, ProcessorPreset};
use shoop_rt_protocol::{
    processors::{Effective, Support},
    ProcessorKind,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Policy(#[from] shoop_rt_protocol::processors::Error),
    #[error("processor storage exceeds the configured memory or shape capacity")]
    Capacity,
    #[error("could not prepare {0}")]
    Preparation(&'static str),
}
pub const fn support() -> Support {
    Support {
        builtin_fx: cfg!(feature = "builtin-fx"),
        oxisynth: cfg!(feature = "oxisynth"),
    }
}
fn add(a: u64, b: u64) -> Result<u64, Error> {
    a.checked_add(b).ok_or(Error::Capacity)
}
fn mul(a: u64, b: u64) -> Result<u64, Error> {
    a.checked_mul(b).ok_or(Error::Capacity)
}

/// Conservative reservation for the pinned rack: 32 sample-seconds of float
/// delay storage per channel/stereo bank, plus explicit staging and unit metadata.
/// Feedback decay time changes gains, not allocated delay lengths.
fn fx_bytes(d: &EngineDescription) -> Result<u64, Error> {
    add(
        add(
            mul(mul(d.sample_rate as u64, 128)?, d.strip_channels as u64 + 1)?,
            mul(d.quantum as u64, d.strip_channels as u64 * 16)?,
        )?,
        1 << 20,
    )
}
fn synth_bytes(d: &EngineDescription) -> Result<u64, Error> {
    #[cfg(feature = "oxisynth")]
    {
        // Bound immutable-font parsing peak and decoded data, all 256 voices,
        // the slowest chorus lookup table, output buffers and fixed metadata.
        let font = mul(shoop_dsp::oxisynth::SOUNDFONT_BYTES.len() as u64, 4)?;
        add(
            add(add(font, 256 * 16384)?, mul(d.sample_rate as u64, 16)?)?,
            add(mul(d.quantum as u64, 8)?, 1 << 20)?,
        )
    }
    #[cfg(not(feature = "oxisynth"))]
    {
        let _ = d;
        Ok(0)
    }
}
fn limits(d: &EngineDescription) -> BufferLimits {
    BufferLimits {
        midi_input: MidiLimits {
            events_per_quantum: d.midi_events,
            bytes_per_quantum: d.midi_bytes,
        },
        midi_output: MidiLimits {
            events_per_quantum: 0,
            bytes_per_quantum: 0,
        },
        parameters_per_quantum: 64,
    }
}
fn reservation(d: &EngineDescription, effective: Effective, q: Quantum) -> Result<u64, Error> {
    let mut total = 0;
    let timing = Timing {
        batch_quanta: 1,
        slack_quanta: 0,
    };
    for (kind, count) in [
        (ProcessorKind::BuiltInFx, effective.fx_instances),
        (ProcessorKind::OxiSynth, effective.synth_instances),
    ] {
        if count == 0 {
            continue;
        }
        let channels = if kind == ProcessorKind::OxiSynth {
            2
        } else {
            d.strip_channels as usize
        };
        let ports = PortCounts {
            audio_inputs: channels,
            audio_outputs: channels,
            midi_inputs: 1,
            midi_outputs: 0,
        };
        let adapter = limits(d)
            .storage_bytes_for(ports, q, timing)
            .map_err(|_| Error::Capacity)? as u64;
        let auxiliary = shoop_rt_engine::HostedProcessor::additional_prepared_bytes(limits(d))
            .map_err(|_| Error::Capacity)? as u64;
        let adapter = add(adapter, auxiliary)?;
        let backend = if kind == ProcessorKind::BuiltInFx {
            fx_bytes(d)?
        } else {
            synth_bytes(d)?
        };
        total = add(total, mul(add(adapter, backend)?, u64::from(count) * 2)?)?;
    }
    if total > 0 {
        total = add(total, 1 << 20)?;
    }
    if total > effective.memory_bytes {
        return Err(Error::Capacity);
    }
    usize::try_from(total).map_err(|_| Error::Capacity)?;
    Ok(total)
}
fn backend(kind: ProcessorKind, d: &EngineDescription) -> Result<Box<dyn Backend>, Error> {
    let _ = d;
    match kind {
        ProcessorKind::BuiltInFx => {
            #[cfg(feature = "builtin-fx")]
            {
                Ok(Box::new(
                    shoop_dsp::backends::FxBackend::prepare(
                        d.sample_rate as f32,
                        d.quantum,
                        d.strip_channels as usize,
                    )
                    .map_err(|_| Error::Preparation("Built-in FX"))?,
                ))
            }
            #[cfg(not(feature = "builtin-fx"))]
            {
                Err(Error::Preparation("Built-in FX is not compiled"))
            }
        }
        ProcessorKind::OxiSynth => {
            #[cfg(feature = "oxisynth")]
            {
                Ok(Box::new(
                    shoop_dsp::backends::SynthBackend::prepare(d.sample_rate as f32, d.quantum)
                        .map_err(|_| Error::Preparation("Built-in Synth"))?,
                ))
            }
            #[cfg(not(feature = "oxisynth"))]
            {
                Err(Error::Preparation("Built-in Synth is not compiled"))
            }
        }
        ProcessorKind::External => Err(Error::Preparation(
            "external processors do not own inline DSP",
        )),
    }
}
pub fn prepare(d: &EngineDescription) -> Result<PreparedProcessors, Error> {
    let effective =
        d.processors
            .resolve_for_channels(support(), d.sample_rate, d.strip_channels)?;
    let q = Quantum::new(d.quantum).map_err(|_| Error::Capacity)?;
    if !(1..=8).contains(&d.strip_channels) {
        return Err(Error::Capacity);
    }
    let reserved_bytes = reservation(d, effective, q)?;
    let mut instances = Vec::with_capacity(effective.slots());
    for _ in 0..2 {
        for (kind, count) in [
            (ProcessorKind::BuiltInFx, effective.fx_instances),
            (ProcessorKind::OxiSynth, effective.synth_instances),
        ] {
            for _ in 0..count {
                instances.push(PreparedProcessor {
                    kind,
                    backend: backend(kind, d)?,
                });
            }
        }
    }
    #[cfg(feature = "oxisynth")]
    let presets: Vec<ProcessorPreset> = if effective.synth_instances == 0 {
        Vec::new()
    } else {
        shoop_dsp::oxisynth::available_presets()
            .iter()
            .map(|preset| {
                Ok(ProcessorPreset {
                    bank: preset.id.bank,
                    program: preset.id.program,
                    name: shoop_rt_protocol::Name::new(preset.name)
                        .ok_or(Error::Preparation("synth preset metadata"))?,
                })
            })
            .collect::<Result<_, Error>>()?
    };
    #[cfg(not(feature = "oxisynth"))]
    let presets: Vec<ProcessorPreset> = Vec::new();
    Ok(PreparedProcessors {
        effective,
        reserved_bytes,
        instances,
        presets: presets.into_boxed_slice(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_rt_protocol::{ProcessorPolicy, ProcessorSettings};
    use shoop_wasm_test_support::{
        heap::{assert_no_heap, TrackingAllocator},
        shoop_test,
    };
    #[global_allocator]
    static ALLOCATOR: TrackingAllocator = TrackingAllocator;
    #[shoop_test]
    fn disabled_factory_has_no_preparation_or_resource_allocation() {
        let d = EngineDescription::default();
        let prepared = assert_no_heap(|| prepare(&d).unwrap());
        assert!(prepared.instances.is_empty());
        assert!(prepared.presets.is_empty());
        assert_eq!(prepared.reserved_bytes, 0);
    }
    #[shoop_test]
    fn enabled_factory_reserves_a_complete_second_bank_and_refuses_over_budget() {
        let d = EngineDescription {
            processors: ProcessorSettings::default(),
            ..EngineDescription::default()
        };
        let prepared = prepare(&d).unwrap();
        assert_eq!(
            prepared.instances.len(),
            if cfg!(feature = "builtin-fx") { 4 } else { 0 }
                + if cfg!(feature = "oxisynth") { 4 } else { 0 }
        );
        assert_eq!(
            prepared.effective.fx_instances,
            if cfg!(feature = "builtin-fx") { 2 } else { 0 }
        );
        assert_eq!(
            prepared.effective.synth_instances,
            if cfg!(feature = "oxisynth") { 2 } else { 0 }
        );
        assert!(prepared.reserved_bytes <= 256 << 20);
        if cfg!(feature = "builtin-fx") || cfg!(feature = "oxisynth") {
            let mut small = d.clone();
            small.processors.memory_mb = 1;
            assert!(matches!(
                assert_no_heap(|| prepare(&small)),
                Err(Error::Capacity)
            ));
        }
        let mut explicit = d;
        if !cfg!(feature = "oxisynth") {
            explicit.processors.oxisynth = ProcessorPolicy::On;
            assert!(prepare(&explicit).is_err());
        }
    }
    #[shoop_test]
    fn measured_preparation_peak_fits_the_complete_reserved_bank_budget() {
        for channels in [1, 2, 8] {
            let d = EngineDescription {
                strip_channels: channels,
                processors: ProcessorSettings {
                    memory_mb: 512,
                    ..ProcessorSettings::default()
                },
                ..EngineDescription::default()
            };
            let (prepared, memory) =
                shoop_wasm_test_support::heap::measure_memory(|| prepare(&d).unwrap());
            assert!(
                memory.peak as u64 <= prepared.reserved_bytes,
                "channels={channels} peak={} reserved={}",
                memory.peak,
                prepared.reserved_bytes
            );
            #[cfg(not(target_arch = "wasm32"))]
            std::println!(
                "channels={channels} requested_peak={} requested_retained={} reserved={}",
                memory.peak,
                memory.live,
                prepared.reserved_bytes
            );
        }
    }
}
