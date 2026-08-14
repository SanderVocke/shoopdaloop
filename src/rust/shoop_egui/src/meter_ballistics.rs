const PEAK_HOLD_SECONDS: f64 = 0.12;
const RELEASE_DB_PER_SECOND: f32 = 70.0;

#[derive(Clone, Copy, Debug)]
pub(crate) struct MeterReading {
    pub db: f32,
    pub animating: bool,
}

#[derive(Debug, Default)]
pub(crate) struct PeakMeterAnimation {
    displayed_db: f32,
    hold_until: f64,
    last_update: Option<f64>,
}

impl PeakMeterAnimation {
    pub fn update(&mut self, target_db: f32, minimum_db: f32, now: f64) -> MeterReading {
        let target_db = if target_db.is_finite() {
            target_db.clamp(minimum_db, 0.0)
        } else {
            minimum_db
        };
        let Some(last_update) = self.last_update else {
            self.displayed_db = target_db;
            self.hold_until = now + PEAK_HOLD_SECONDS;
            self.last_update = Some(now);
            return MeterReading {
                db: self.displayed_db,
                animating: false,
            };
        };

        if now < last_update {
            self.last_update = Some(now);
        } else if target_db >= self.displayed_db {
            self.displayed_db = target_db;
            self.hold_until = now + PEAK_HOLD_SECONDS;
            self.last_update = Some(now);
        } else {
            let decay_start = last_update.max(self.hold_until);
            let elapsed = (now - decay_start).max(0.0) as f32;
            self.displayed_db = (self.displayed_db - RELEASE_DB_PER_SECOND * elapsed)
                .max(target_db)
                .max(minimum_db);
            self.last_update = Some(now);
        }

        MeterReading {
            db: self.displayed_db,
            animating: self.displayed_db > target_db,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(left: f32, right: f32) {
        assert!((left - right).abs() < 0.001, "{left} != {right}");
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn attacks_immediately_then_holds_before_releasing() {
        let mut meter = PeakMeterAnimation::default();
        close(meter.update(-50.0, -50.0, 0.0).db, -50.0);
        close(meter.update(-10.0, -50.0, 0.01).db, -10.0);

        let held = meter.update(-50.0, -50.0, 0.12);
        close(held.db, -10.0);
        assert!(held.animating);

        let falling = meter.update(-50.0, -50.0, 0.23);
        close(falling.db, -17.0);
        assert!(falling.animating);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn release_is_time_based_and_never_falls_below_the_current_signal() {
        let mut meter = PeakMeterAnimation::default();
        meter.update(0.0, -50.0, 0.0);
        let first = meter.update(-20.0, -50.0, PEAK_HOLD_SECONDS + 0.1);
        close(first.db, -7.0);
        let second = meter.update(-9.0, -50.0, PEAK_HOLD_SECONDS + 0.2);
        close(second.db, -9.0);
        assert!(!second.animating);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn full_scale_reaches_the_floor_in_about_seven_tenths_of_a_second_after_hold() {
        let mut meter = PeakMeterAnimation::default();
        meter.update(0.0, -50.0, 0.0);
        let reading = meter.update(-50.0, -50.0, PEAK_HOLD_SECONDS + 50.0 / 70.0);
        close(reading.db, -50.0);
        assert!(!reading.animating);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn invalid_values_are_silence_and_values_are_clamped_to_the_meter_range() {
        let mut meter = PeakMeterAnimation::default();
        close(meter.update(f32::NAN, -50.0, 0.0).db, -50.0);
        close(meter.update(3.0, -50.0, 0.1).db, 0.0);
    }
}
