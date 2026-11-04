//! Compile authored cycle endpoints into a frozen, quantum-aligned sparse program.
use super::*;
use shoop_app_protocol::composite::{Document, EffectiveInstance, Prepared, Span, TimingBasis};

pub(super) fn frame_at(basis: TimingBasis, cycle: u64, format: ConversionTarget) -> Result<u64> {
    let numerator = u128::from(cycle)
        .checked_mul(u128::from(basis.unit_frames))
        .and_then(|value| value.checked_mul(u128::from(format.sample_rate)))
        .ok_or_else(|| anyhow!("composite position overflow"))?;
    let denominator = u128::from(basis.sample_rate) * u128::from(format.quantum_frames);
    let quanta = numerator
        .checked_add(denominator / 2)
        .ok_or_else(|| anyhow!("composite position overflow"))?
        / denominator;
    u64::try_from(
        quanta
            .checked_mul(u128::from(format.quantum_frames))
            .ok_or_else(|| anyhow!("composite position overflow"))?,
    )
    .map_err(Into::into)
}

fn unit_frames(basis: TimingBasis, format: ConversionTarget) -> Result<u64> {
    if basis.sample_rate == format.sample_rate
        && basis.unit_frames % u64::from(format.quantum_frames) != 0
    {
        return Err(anyhow!(
            "composite time unit must align to the engine quantum"
        ));
    }
    let numerator = u128::from(basis.unit_frames) * u128::from(format.sample_rate);
    let exact = numerator / u128::from(basis.sample_rate);
    Ok(
        if numerator % u128::from(basis.sample_rate) == 0
            && exact > 0
            && exact % u128::from(format.quantum_frames) == 0
        {
            u64::try_from(exact)?
        } else {
            u64::from(format.quantum_frames)
        },
    )
}

pub(super) fn cycles_for_frames(basis: TimingBasis, frames: u64, sample_rate: u32) -> Result<u64> {
    let numerator = u128::from(frames) * u128::from(basis.sample_rate);
    let denominator = u128::from(basis.unit_frames) * u128::from(sample_rate);
    Ok(u64::try_from(numerator.div_ceil(denominator).max(1))?)
}

pub(super) fn compile(
    target: LoopId,
    document: &Document,
    format: ConversionTarget,
    limits: BackendCompositeLimits,
    length: impl Fn(LoopId) -> Result<u64>,
) -> Result<Prepared> {
    if !document.valid() || !format.is_valid() || document.instances.len() > limits.entries as usize
    {
        return Err(anyhow!("invalid or oversized composite document"));
    }
    let unit = unit_frames(document.basis, format)?;
    let mut instances = Vec::with_capacity(document.instances.len());
    let mut entries = Vec::with_capacity(document.instances.len());
    for event in &document.instances {
        if event.loop_id == target {
            return Err(anyhow!("a composite cannot contain itself"));
        }
        let duration = event.n_cycles.map(u64::from).map(Ok).unwrap_or_else(|| {
            cycles_for_frames(document.basis, length(event.loop_id)?, format.sample_rate)
        })?;
        let end_cycle = event
            .start_cycle
            .checked_add(duration)
            .ok_or_else(|| anyhow!("composite duration overflow"))?;
        let start_frame = frame_at(document.basis, event.start_cycle, format)?;
        let end_frame = frame_at(document.basis, end_cycle, format)?.max(
            start_frame
                .checked_add(unit)
                .ok_or_else(|| anyhow!("composite duration overflow"))?,
        );
        instances.push(EffectiveInstance {
            instance_id: event.instance_id,
            start_frame,
            end_frame,
        });
        let mode = event
            .mode
            .map(|mode| match mode {
                LoopMode::Stopped => Ok(shoop_loop_plan::Mode::Stopped),
                LoopMode::Playing => Ok(shoop_loop_plan::Mode::Playing),
                LoopMode::Recording => Ok(shoop_loop_plan::Mode::Recording),
                _ => Err(anyhow!(
                    "the engine does not support this composite event mode"
                )),
            })
            .transpose()?;
        entries.push(shoop_loop_plan::Entry {
            target: event.loop_id,
            delay: start_frame / unit,
            duration: Some((end_frame - start_frame) / unit),
            repetitions: 1,
            mode,
        });
    }
    let description = shoop_loop_plan::Description {
        source: target,
        nominal_unit_frames: unit,
        sections: vec![shoop_loop_plan::Section { entries }],
    };
    let compiled = shoop_loop_plan::compile::<_, shoop_loop_plan::Error>(
        &description,
        shoop_loop_plan::Limits {
            program_entries: limits.entries as usize,
            program_segments: limits.segments as usize,
        },
        |_| Ok(0),
    )
    .map_err(|error| anyhow!("invalid composite program: {error:?}"))?;
    let spans = compiled
        .targets()
        .iter()
        .flat_map(|target| {
            target.segments.iter().map(|span| Span {
                target: target.target,
                start: span.start,
                end: span.end,
                occurrence_start: span.occurrence_start,
                ordinal: span.ordinal,
                mode: match span.mode {
                    shoop_loop_plan::Mode::Stopped => LoopMode::Stopped,
                    shoop_loop_plan::Mode::Playing => LoopMode::Playing,
                    shoop_loop_plan::Mode::Recording => LoopMode::Recording,
                },
            })
        })
        .collect();
    Ok(Prepared {
        sample_rate: format.sample_rate,
        quantum: format.quantum_frames,
        unit_frames: unit,
        iterations: compiled.iterations,
        spans,
        instances,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_app_protocol::composite::Instance;
    use shoop_wasm_test_support::shoop_test;
    fn document() -> Document {
        Document {
            kind: CompositeKind::Regular,
            basis: TimingBasis {
                sample_rate: 48000,
                unit_frames: 480,
            },
            instances: vec![
                Instance {
                    instance_id: 1,
                    loop_id: LoopId::from_raw(2),
                    start_cycle: 0,
                    n_cycles: Some(3),
                    mode: None,
                },
                Instance {
                    instance_id: 2,
                    loop_id: LoopId::from_raw(3),
                    start_cycle: 1003,
                    n_cycles: Some(7),
                    mode: None,
                },
            ],
        }
    }
    fn limits() -> BackendCompositeLimits {
        BackendCompositeLimits {
            entries: 8,
            segments: 16,
            relations: 8,
            quantum: 32,
        }
    }
    #[shoop_test]
    fn sample_conversion_quantizes_individual_endpoints_without_accumulating_unit_drift() {
        let prepared = compile(
            LoopId::from_raw(1),
            &document(),
            ConversionTarget {
                sample_rate: 44100,
                quantum_frames: 32,
            },
            limits(),
            |_| Ok(0),
        )
        .unwrap();
        assert_eq!(prepared.unit_frames, 32);
        assert_eq!(
            (
                prepared.instances[0].start_frame,
                prepared.instances[0].end_frame
            ),
            (0, 1312)
        );
        assert_eq!(
            (
                prepared.instances[1].start_frame,
                prepared.instances[1].end_frame
            ),
            (442336, 445408)
        );
        assert_eq!(prepared.unit_frames * prepared.iterations, 445408);
    }
    #[shoop_test]
    fn incompatible_live_basis_overflow_and_unsupported_modes_refuse_before_preparation() {
        let format = ConversionTarget {
            sample_rate: 48000,
            quantum_frames: 32,
        };
        let mut doc = document();
        doc.basis.unit_frames = 1;
        assert!(compile(LoopId::from_raw(1), &doc, format, limits(), |_| Ok(0)).is_err());
        doc.basis.unit_frames = u64::MAX - 31;
        doc.instances[1].start_cycle = u64::MAX;
        assert!(compile(LoopId::from_raw(1), &doc, format, limits(), |_| Ok(0)).is_err());
        doc = document();
        doc.kind = CompositeKind::Script;
        for event in &mut doc.instances {
            event.mode = Some(LoopMode::Replacing);
        }
        assert!(compile(LoopId::from_raw(1), &doc, format, limits(), |_| Ok(0)).is_err());
    }
}
