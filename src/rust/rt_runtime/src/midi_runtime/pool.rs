use crate::Error;
use alloc::{boxed::Box, vec};

pub(super) struct Queue<T: Copy> {
    rows: Box<[Option<T>]>,
    head: usize,
    len: usize,
}
impl<T: Copy> Queue<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            rows: vec![None; capacity].into_boxed_slice(),
            head: 0,
            len: 0,
        }
    }
    pub fn push(&mut self, value: T) {
        assert!(self.len < self.rows.len());
        let index = (self.head + self.len) % self.rows.len();
        self.rows[index] = Some(value);
        self.len += 1;
    }
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        let value = self.rows[self.head].take();
        self.head = (self.head + 1) % self.rows.len();
        self.len -= 1;
        value
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn allocated_bytes(&self) -> usize {
        core::mem::size_of_val(&*self.rows)
    }
}

pub(super) struct Pool<T: Copy> {
    rows: Box<[Option<T>]>,
    references: Box<[u32]>,
    free: Queue<u32>,
}
impl<T: Copy> Pool<T> {
    pub fn new(capacity: usize) -> Self {
        let mut free = Queue::new(capacity);
        for index in 0..capacity {
            free.push(index as u32 + 1);
        }
        Self {
            rows: vec![None; capacity].into_boxed_slice(),
            references: vec![0; capacity].into_boxed_slice(),
            free,
        }
    }
    pub fn available(&self) -> usize {
        self.free.len()
    }
    pub fn insert(&mut self, value: T) -> u32 {
        let id = self.free.pop().expect("reserved pool slot");
        self.rows[id as usize - 1] = Some(value);
        self.references[id as usize - 1] = 1;
        id
    }
    pub fn value(&self, id: u32) -> T {
        self.rows[id as usize - 1].expect("owned pool slot")
    }
    pub fn replace(&mut self, id: u32, value: T) {
        self.rows[id as usize - 1] = Some(value);
    }
    pub fn row(&self, id: u32) -> &T {
        self.rows[id as usize - 1]
            .as_ref()
            .expect("owned pool slot")
    }
    pub fn row_mut(&mut self, id: u32) -> &mut T {
        self.rows[id as usize - 1]
            .as_mut()
            .expect("owned pool slot")
    }
    pub fn retain(&mut self, id: u32) {
        if id != 0 {
            let count = &mut self.references[id as usize - 1];
            assert!(*count != 0);
            *count = count.checked_add(1).expect("prepared reference ceiling");
        }
    }
    pub fn release(&mut self, id: u32) -> bool {
        if id == 0 {
            return false;
        }
        let count = &mut self.references[id as usize - 1];
        assert!(*count != 0);
        *count -= 1;
        *count == 0
    }
    pub fn reclaim(&mut self, id: u32) {
        assert_eq!(self.references[id as usize - 1], 0);
        assert!(self.rows[id as usize - 1].take().is_some());
        self.free.push(id);
    }
    pub fn live(&self, id: u32) -> Result<T, Error> {
        let index = id.checked_sub(1).ok_or(Error::InvalidId)? as usize;
        if self.references.get(index).copied().unwrap_or(0) == 0 {
            return Err(Error::InvalidId);
        }
        self.rows[index].ok_or(Error::InvalidId)
    }
    pub fn allocated_bytes(&self) -> usize {
        core::mem::size_of_val(&*self.rows)
            + core::mem::size_of_val(&*self.references)
            + self.free.allocated_bytes()
    }
}
