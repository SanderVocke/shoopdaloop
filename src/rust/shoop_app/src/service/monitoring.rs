//! Service-owned monitoring policy; phase demand is evaluated by the render owner.
use super::*;

impl Service {
    pub(super) fn set_automatic_monitoring_policy(
        &mut self,
        track: TrackId,
        enabled: bool,
    ) -> Result<(), CommandError> {
        if self.backend.composite_limits().is_none() {
            return Ok(());
        }
        let before = self.backend.mutation_cursor();
        self.backend
            .set_track_automatic_monitoring(self.tracks[&track].backend, enabled)?;
        let sequence = self.backend.mutation_cursor();
        if sequence > before {
            self.automatic_monitoring_intent.insert(track, sequence);
        }
        Ok(())
    }
    pub(super) fn reconcile_monitoring_policy(&mut self) {
        if self.backend.composite_limits().is_none() || self.engine_closed {
            return;
        }
        self.automatic_monitoring_intent
            .retain(|id, _| self.tracks.contains_key(id));
        let targets = self
            .tracks
            .iter()
            .filter_map(|(id, track)| {
                (!track.view.is_sync
                    && track.view.structural_state == StructuralState::Confirmed
                    && !self.automatic_monitoring_intent.contains_key(id)
                    && self
                        .observed
                        .tracks
                        .get(&track.backend)
                        .is_some_and(|state| {
                            state.automatic_monitoring_enabled != self.global.auto_arm_track_inputs
                        }))
                .then_some(*id)
            })
            .collect::<Vec<_>>();
        for track in targets {
            if let Err(error) =
                self.set_automatic_monitoring_policy(track, self.global.auto_arm_track_inputs)
            {
                tracing::debug!(track = track.raw(), error = %error.message, "service.automatic_monitoring_policy_deferred");
            }
        }
    }
}
