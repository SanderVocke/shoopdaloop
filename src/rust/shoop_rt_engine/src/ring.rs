//! A fixed-capacity FIFO prepared off-render.

pub(crate) struct Ring<T> {
    items: Box<[Option<T>]>,
    head: usize,
    len: usize,
}

impl<T: Copy> Ring<T> {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            items: vec![None; capacity].into_boxed_slice(),
            head: 0,
            len: 0,
        }
    }
    pub(crate) fn len(&self) -> usize {
        self.len
    }
    pub(crate) fn capacity(&self) -> usize {
        self.items.len()
    }
    pub(crate) fn is_full(&self) -> bool {
        self.len == self.items.len()
    }
    pub(crate) fn push(&mut self, value: T) -> Result<(), T> {
        if self.is_full() {
            return Err(value);
        }
        let index = (self.head + self.len) % self.items.len();
        self.items[index] = Some(value);
        self.len += 1;
        Ok(())
    }
    pub(crate) fn front(&self) -> Option<T> {
        if self.len == 0 {
            None
        } else {
            self.items[self.head]
        }
    }
    pub(crate) fn pop(&mut self) -> Option<T> {
        let value = self.front()?;
        self.items[self.head] = None;
        self.head = (self.head + 1) % self.items.len();
        self.len -= 1;
        Some(value)
    }
    pub(crate) fn clear(&mut self) {
        while self.pop().is_some() {}
    }
}
