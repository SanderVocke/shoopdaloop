use super::*;
use shoop_app_protocol::composite as common;

fn mode_name(mode: LoopMode) -> &'static str {
    match mode {
        LoopMode::Stopped => "stopped",
        LoopMode::Playing => "playing",
        LoopMode::Recording => "recording",
        LoopMode::Replacing => "replacing",
        LoopMode::PlayingDryThroughWet => "playing_dry_through_wet",
        LoopMode::RecordingDryIntoWet => "recording_dry_into_wet",
        LoopMode::Unknown => "unknown",
    }
}
fn parse_mode(mode: &str) -> Result<LoopMode> {
    Ok(match mode {
        "stopped" => LoopMode::Stopped,
        "playing" => LoopMode::Playing,
        "recording" => LoopMode::Recording,
        _ => bail!("unsupported composite event mode: {mode}"),
    })
}

pub(super) fn to_document(content: &common::Content) -> CompositeDocument {
    let p = &content.prepared;
    CompositeDocument {
        kind: if content.document.kind == CompositeKind::Regular {
            CompositeKindDocument::Regular
        } else {
            CompositeKindDocument::Script
        },
        instances: content
            .document
            .instances
            .iter()
            .map(|event| CompositeLoopInstanceDocument {
                instance_id: event.instance_id,
                loop_id: event.loop_id.raw(),
                start_cycle: event.start_cycle,
                n_cycles: event.n_cycles,
                mode: event.mode.map(|mode| mode_name(mode).to_owned()),
            })
            .collect(),
        timing: Some(CompositeTimingDocument {
            basis_sample_rate: content.document.basis.sample_rate,
            basis_unit_frames: content.document.basis.unit_frames,
            sample_rate: p.sample_rate,
            quantum_frames: p.quantum,
            unit_frames: p.unit_frames,
            iterations: p.iterations,
            instances: p
                .instances
                .iter()
                .map(|event| CompositeEffectiveInstanceDocument {
                    instance_id: event.instance_id,
                    start_frame: event.start_frame,
                    end_frame: event.end_frame,
                })
                .collect(),
            spans: p
                .spans
                .iter()
                .map(|span| CompositeSpanDocument {
                    target: span.target.raw(),
                    start: span.start,
                    end: span.end,
                    occurrence_start: span.occurrence_start,
                    ordinal: span.ordinal,
                    mode: mode_name(span.mode).to_owned(),
                })
                .collect(),
        }),
    }
}

pub(super) fn from_document(content: &CompositeDocument) -> Result<common::Content> {
    let p = content
        .timing
        .as_ref()
        .ok_or_else(|| anyhow!("composite timing was not migrated"))?;
    let content = common::Content {
        document: common::Document {
            kind: match content.kind {
                CompositeKindDocument::Regular => CompositeKind::Regular,
                CompositeKindDocument::Script => CompositeKind::Script,
            },
            basis: common::TimingBasis {
                sample_rate: p.basis_sample_rate,
                unit_frames: p.basis_unit_frames,
            },
            instances: content
                .instances
                .iter()
                .map(|event| {
                    Ok(common::Instance {
                        instance_id: event.instance_id,
                        loop_id: LoopId::from_raw(event.loop_id),
                        start_cycle: event.start_cycle,
                        n_cycles: event.n_cycles,
                        mode: event.mode.as_deref().map(parse_mode).transpose()?,
                    })
                })
                .collect::<Result<_>>()?,
        },
        prepared: common::Prepared {
            sample_rate: p.sample_rate,
            quantum: p.quantum_frames,
            unit_frames: p.unit_frames,
            iterations: p.iterations,
            instances: p
                .instances
                .iter()
                .map(|event| common::EffectiveInstance {
                    instance_id: event.instance_id,
                    start_frame: event.start_frame,
                    end_frame: event.end_frame,
                })
                .collect(),
            spans: p
                .spans
                .iter()
                .map(|span| {
                    Ok(common::Span {
                        target: LoopId::from_raw(span.target),
                        start: span.start,
                        end: span.end,
                        occurrence_start: span.occurrence_start,
                        ordinal: span.ordinal,
                        mode: parse_mode(&span.mode)?,
                    })
                })
                .collect::<Result<_>>()?,
        },
    };
    ensure!(content.valid(), "invalid frozen composite timing");
    Ok(content)
}
