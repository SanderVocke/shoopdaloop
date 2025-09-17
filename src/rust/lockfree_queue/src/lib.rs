use anyhow::{anyhow, Result};
use std::time::Duration;

pub struct Sender<T> {
    snd: lockfree::prelude::mpsc::Sender<T>,
}

pub struct Receiver<T> {
    recv: lockfree::prelude::mpsc::Receiver<T>,
}

impl<T> Receiver<T> {
    pub fn recv_timeout(&mut self, timeout: Duration) {
        todo!();
    }

    pub fn recv(&mut self) -> Result<T> {
        todo!();
    }
}

impl<T> Sender <T> {
    pub fn send(&self, obj : T) -> Result<()> {
        todo!();
    }
}

pub fn create<T>() -> Result<(Sender<T>, Receiver<T>)> {
    todo!();
}