//! Host-advertised processor descriptors, independent of this client's build.
use crate::{EngineClock, RtBackend};
use alloc::{format, sync::Arc, vec::Vec};
use shoop_app_api::{
    HostedProcessorResources, TrackProcessorConstraints, TrackProcessorDescriptor,
    TrackProcessorEditorDescriptor, TrackProcessorFeatures, TrackProcessorMidiPolicy,
    TrackProcessorPresetDescriptor, TrackProcessorTypeId,
};
use shoop_rt_protocol::{processors::Unavailable, Capabilities};
use shoop_rt_transport::carrier::Carrier;

impl<C: Carrier, K: EngineClock> RtBackend<C, K> {
    pub fn processor_limits(&self) -> Option<shoop_rt_protocol::ProcessorLimits> {
        self.processors
    }
    pub(crate) fn processor_descriptors(&self) -> Arc<[TrackProcessorDescriptor]> {
        let Some(limits) = self.processors else {
            return Arc::from([]);
        };
        let external = self.capabilities.contains(Capabilities::EXTERNAL_DRY_WET)
            && self.capabilities.contains(Capabilities::DRY_WET);
        let hosted = self.capabilities.contains(Capabilities::TRACK_PROCESSORS)
            && self.capabilities.contains(Capabilities::DRY_WET);
        let unavailable = |available: bool, reason: Option<Unavailable>| {
            if available {
                None
            } else {
                Some(
                    reason
                        .map_or(
                            "this host does not support this track processor",
                            Unavailable::message,
                        )
                        .into(),
                )
            }
        };
        let width = Some(u32::from(limits.channels));
        let fx = hosted
            && limits.effective.fx_instances > 0
            && limits.effective.fx_unavailable.is_none();
        let synth = hosted
            && limits.effective.synth_instances > 0
            && limits.effective.synth_unavailable.is_none();
        let presets: Vec<_> = self
            .processor_presets
            .iter()
            .map(|preset| TrackProcessorPresetDescriptor {
                id: format!("{}:{}", preset.bank, preset.program),
                name: preset.name.as_str().into(),
            })
            .collect();
        let resources = |kind, instance_limit| {
            Some(HostedProcessorResources {
                instance_limit,
                instances_in_use: self
                    .mirror
                    .tracks
                    .values()
                    .filter(|track| {
                        track.confirmed
                            && !track.removing
                            && crate::processor_controls::track_kind(track) == Some(kind)
                    })
                    .count()
                    .saturating_add(
                        self.mirror
                            .buses
                            .values()
                            .filter(|bus| {
                                bus.confirmed
                                    && !bus.removing
                                    && bus.processor_slot.is_some()
                                    && kind == shoop_rt_protocol::ProcessorKind::BuiltInFx
                            })
                            .count(),
                    ) as u16,
                buffering_frames: limits.builtin_latency_frames,
                reserved_host_bytes: limits.reserved_bytes,
                budget_host_bytes: limits.effective.memory_bytes,
            })
        };
        Arc::from([
            TrackProcessorDescriptor {
                resources: None,
                id: TrackProcessorTypeId::new(TrackProcessorTypeId::EXTERNAL),
                label: "External".into(),
                available: external,
                unavailable_reason: unavailable(external, None),
                constraints: TrackProcessorConstraints {
                    max_dry_audio_channels: width,
                    max_wet_audio_channels: width,
                    midi: TrackProcessorMidiPolicy::Optional,
                    ..Default::default()
                },
                features: TrackProcessorFeatures::default(),
                editor: None,
            },
            TrackProcessorDescriptor {
                resources: resources(
                    shoop_rt_protocol::ProcessorKind::BuiltInFx,
                    limits.effective.fx_instances,
                ),
                id: TrackProcessorTypeId::new(TrackProcessorTypeId::BUILTIN_FX),
                label: "Built-in FX".into(),
                available: fx,
                unavailable_reason: unavailable(fx, limits.effective.fx_unavailable),
                constraints: TrackProcessorConstraints {
                    min_dry_audio_channels: Some(1),
                    max_dry_audio_channels: width,
                    min_wet_audio_channels: Some(1),
                    max_wet_audio_channels: width,
                    matching_audio_channels: true,
                    midi: TrackProcessorMidiPolicy::Required,
                },
                features: TrackProcessorFeatures {
                    state: true,
                    embedded_ui: true,
                    ..Default::default()
                },
                editor: Some(TrackProcessorEditorDescriptor::BuiltInFx),
            },
            TrackProcessorDescriptor {
                resources: resources(
                    shoop_rt_protocol::ProcessorKind::OxiSynth,
                    limits.effective.synth_instances,
                ),
                id: TrackProcessorTypeId::new(TrackProcessorTypeId::OXISYNTH),
                label: "Built-in Synth".into(),
                available: synth,
                unavailable_reason: unavailable(synth, limits.effective.synth_unavailable),
                constraints: TrackProcessorConstraints {
                    min_dry_audio_channels: Some(2),
                    max_dry_audio_channels: Some(2),
                    min_wet_audio_channels: Some(2),
                    max_wet_audio_channels: Some(2),
                    matching_audio_channels: false,
                    midi: TrackProcessorMidiPolicy::Required,
                },
                features: TrackProcessorFeatures {
                    state: true,
                    embedded_ui: true,
                    ..Default::default()
                },
                editor: Some(TrackProcessorEditorDescriptor::OxiSynth {
                    presets: presets.into(),
                }),
            },
        ])
    }
}
