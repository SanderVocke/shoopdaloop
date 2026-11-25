#[derive(Default)]
pub(crate) struct Reboot {
    requested_at: Option<u64>,
}

impl Reboot {
    pub fn poll(&mut self, now_ms: u64) -> bool {
        let started = *self.requested_at.get_or_insert(now_ms);
        now_ms.saturating_sub(started) >= 250
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;

    #[shoop_test]
    fn reboot_gives_the_reply_time_to_drain_and_retries_cannot_postpone_it() {
        let mut reboot = Reboot::default();
        assert!(!reboot.poll(1000));
        assert!(!reboot.poll(1000));
        assert!(!reboot.poll(1100));
        assert!(!reboot.poll(1249));
        assert!(reboot.poll(1250));
        assert!(reboot.poll(1300));
    }
}
