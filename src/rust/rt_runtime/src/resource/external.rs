//! Scoped cursor plumbing for external sample storage, without a CPU sample copy.
use super::*;
pub struct ExternalReadScope {
    scope: u32,
}
impl ExternalReadScope {
    pub fn new() -> Result<Self, Error> {
        Ok(Self {
            scope: index::new_scope()?,
        })
    }
    pub fn index(&self, snapshot: SnapshotId) -> ReadIndex {
        ReadIndex {
            scope: self.scope,
            snapshot,
        }
    }
    pub fn cursor(&self, snapshot: SnapshotId) -> Cursor {
        Cursor {
            scope: self.scope,
            snapshot,
            page: None,
            position: 0,
        }
    }
    pub fn snapshot(&self, cursor: Cursor) -> Result<SnapshotId, Error> {
        if cursor.scope != self.scope || cursor.page.is_some() {
            return Err(Error::InvalidId);
        }
        Ok(cursor.snapshot)
    }
    pub fn advance(&self, cursor: &mut Cursor, bytes: usize, total: usize) -> Result<(), Error> {
        self.snapshot(*cursor)?;
        cursor.position = cursor
            .position
            .checked_add(bytes)
            .filter(|&p| p <= total)
            .ok_or(Error::InvalidOffset)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_wasm_test_support::shoop_test;
    #[shoop_test]
    fn external_cursor_scope_and_checked_progress_do_not_alias_native_storage() {
        let a = ExternalReadScope::new().unwrap();
        let b = ExternalReadScope::new().unwrap();
        let snapshot = SnapshotId {
            slot: 0,
            generation: 1,
        };
        let mut cursor = a.cursor(snapshot);
        assert_eq!(b.snapshot(cursor), Err(Error::InvalidId));
        a.advance(&mut cursor, 3, 8).unwrap();
        assert_eq!(cursor.position(), 3);
        assert_eq!(a.advance(&mut cursor, 6, 8), Err(Error::InvalidOffset));
        assert_eq!(cursor.position(), 3);
        let config = crate::Config::default().prepare().unwrap();
        let quantum = crate::Quantum::new(32).unwrap();
        let mut native = ResourceStore::<crate::Q824>::new_numeric(config, quantum).unwrap();
        assert_eq!(
            native.render_io().cursor_at(a.index(snapshot), 0),
            Err(Error::InvalidId)
        );
    }
}
