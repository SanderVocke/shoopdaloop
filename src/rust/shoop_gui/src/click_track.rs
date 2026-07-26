//! A metronome, driven by the sync loop.
//!
//! A looper wants to hear the bar it is recording into, and the sync loop already defines that
//! bar. So the click is derived from the sync loop's position rather than from a clock of its
//! own: it cannot drift, and changing the sync length changes the tempo with no extra work.
//!
//! Sounded through the instrument, because it already renders notes and mixes into the same
//! port. That is why this only computes *when* to click and on what note -- it produces no
//! audio itself.

/// Which beat of the bar, and how it should sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Click {
    /// Zero-based beat within the bar.
    pub beat: u32,
    /// Whether this is the downbeat, which is accented.
    pub accented: bool,
}

#[derive(Debug, Clone)]
pub struct ClickTrack {
    /// Beats per bar, where a bar is one sync-loop cycle.
    pub beats_per_bar: u32,
    pub enabled: bool,
    /// Note used for the accented beat, and for the rest.
    pub accent_note: u8,
    pub beat_note: u8,
    pub velocity: u8,
    /// Beat last sounded, so a beat is not clicked twice within one buffer.
    last_beat: Option<u32>,
}

impl Default for ClickTrack {
    fn default() -> Self {
        Self {
            beats_per_bar: 4,
            enabled: true,
            // High wood-block-ish pitches, far from where a player is likely to be playing so
            // the click stays distinguishable.
            accent_note: 96,
            beat_note: 84,
            velocity: 100,
            last_beat: None,
        }
    }
}

impl ClickTrack {
    /// The beat a position falls in, given the bar's length.
    ///
    /// `None` when there is no bar to divide, which is what a zero-length sync loop means.
    pub fn beat_at(&self, position: u32, sync_length: u32) -> Option<u32> {
        if sync_length == 0 || self.beats_per_bar == 0 {
            return None;
        }
        let per_beat = sync_length / self.beats_per_bar;
        if per_beat == 0 {
            // More beats than frames: dividing further would click every frame.
            return None;
        }
        Some((position / per_beat).min(self.beats_per_bar - 1))
    }

    /// Whether a click is due at this position, and which one.
    ///
    /// Stateful because it is polled: it reports a beat once, when the position first enters it,
    /// rather than on every poll while the position stays inside.
    pub fn poll(&mut self, position: u32, sync_length: u32) -> Option<Click> {
        if !self.enabled {
            // Forgotten while disabled, so re-enabling clicks the beat it lands in rather than
            // suppressing it as already sounded.
            self.last_beat = None;
            return None;
        }
        let beat = self.beat_at(position, sync_length)?;
        if self.last_beat == Some(beat) {
            return None;
        }
        self.last_beat = Some(beat);
        Some(Click {
            beat,
            accented: beat == 0,
        })
    }

    /// The note a click sounds.
    pub fn note_for(&self, click: Click) -> u8 {
        if click.accented {
            self.accent_note
        } else {
            self.beat_note
        }
    }

    /// Forgets what was last sounded, for when the transport is restarted.
    pub fn reset(&mut self) {
        self.last_beat = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> ClickTrack {
        ClickTrack {
            enabled: true,
            ..Default::default()
        }
    }

    #[test]
    fn a_bar_divides_into_beats() {
        let t = track();
        // Four beats across 400 frames: one every 100.
        assert_eq!(t.beat_at(0, 400), Some(0));
        assert_eq!(t.beat_at(99, 400), Some(0));
        assert_eq!(t.beat_at(100, 400), Some(1));
        assert_eq!(t.beat_at(399, 400), Some(3));
    }

    #[test]
    fn a_position_at_the_very_end_does_not_overflow_the_bar() {
        let t = track();
        // Rounding could otherwise give beat 4 of a four-beat bar.
        assert_eq!(t.beat_at(400, 400), Some(3));
        assert_eq!(t.beat_at(100_000, 400), Some(3));
    }

    #[test]
    fn a_bar_with_no_length_has_no_beats() {
        let t = track();
        assert_eq!(t.beat_at(0, 0), None);
    }

    #[test]
    fn more_beats_than_frames_does_not_click_every_frame() {
        let t = ClickTrack {
            beats_per_bar: 100,
            enabled: true,
            ..Default::default()
        };
        // Ten frames cannot hold a hundred beats; better to fall silent than to machine-gun.
        assert_eq!(t.beat_at(5, 10), None);
    }

    #[test]
    fn a_beat_sounds_once_however_often_it_is_polled() {
        let mut t = track();
        assert_eq!(t.poll(0, 400).map(|c| c.beat), Some(0));
        // Still inside beat zero.
        assert_eq!(t.poll(10, 400), None);
        assert_eq!(t.poll(99, 400), None);
        // Now beat one.
        assert_eq!(t.poll(100, 400).map(|c| c.beat), Some(1));
    }

    #[test]
    fn the_downbeat_is_accented_and_the_others_are_not() {
        let mut t = track();
        let first = t.poll(0, 400).expect("a click");
        assert!(first.accented);
        assert_eq!(t.note_for(first), t.accent_note);

        let second = t.poll(100, 400).expect("a click");
        assert!(!second.accented);
        assert_eq!(t.note_for(second), t.beat_note);
    }

    #[test]
    fn the_click_is_on_by_default() {
        // The bar is what everything aligns to, so it has to be audible without being asked for.
        assert!(ClickTrack::default().enabled);
    }

    #[test]
    fn a_disabled_track_is_silent() {
        let mut t = ClickTrack {
            enabled: false,
            ..Default::default()
        };
        assert_eq!(t.poll(0, 400), None);
        assert_eq!(t.poll(100, 400), None);
    }

    #[test]
    fn enabling_mid_bar_clicks_the_beat_it_lands_in() {
        let mut t = ClickTrack {
            enabled: false,
            ..Default::default()
        };
        // Polled while disabled, which must not mark the beat as already sounded.
        t.poll(150, 400);
        t.enabled = true;
        assert_eq!(t.poll(150, 400).map(|c| c.beat), Some(1));
    }

    #[test]
    fn a_bar_wrapping_clicks_the_downbeat_again() {
        let mut t = track();
        t.poll(0, 400);
        t.poll(100, 400);
        t.poll(200, 400);
        t.poll(300, 400);
        // Wrapped round to the start of the next bar.
        let click = t.poll(0, 400).expect("the downbeat");
        assert_eq!(click.beat, 0);
        assert!(click.accented);
    }

    #[test]
    fn resetting_lets_the_current_beat_sound_again() {
        let mut t = track();
        t.poll(0, 400);
        assert_eq!(t.poll(0, 400), None);
        t.reset();
        assert!(t.poll(0, 400).is_some());
    }

    #[test]
    fn changing_the_bar_length_changes_the_tempo() {
        let t = track();
        // Half the length is twice the tempo: the same position is a later beat.
        assert_eq!(t.beat_at(100, 400), Some(1));
        assert_eq!(t.beat_at(100, 200), Some(2));
    }
}
