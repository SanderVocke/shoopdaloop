#[derive(Debug, Default)]
pub(crate) struct OptimisticValue<T> {
    pending: Option<T>,
}

impl<T: Copy + PartialEq> OptimisticValue<T> {
    pub(crate) fn resolve(&mut self, authoritative: T, interaction_active: bool) -> T {
        if !interaction_active && self.pending == Some(authoritative) {
            self.pending = None;
        }
        self.pending.unwrap_or(authoritative)
    }

    pub(crate) fn set(&mut self, value: T) {
        self.pending = Some(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tracy_nextest_capture::tracy_capture_test]
    fn pending_value_survives_stale_publication_until_acknowledged() {
        let mut value = OptimisticValue::default();
        value.set(2);
        assert_eq!(value.resolve(1, true), 2);
        assert_eq!(value.resolve(1, false), 2);
        assert_eq!(value.resolve(2, false), 2);
        assert_eq!(value.resolve(3, false), 3);
    }
}
