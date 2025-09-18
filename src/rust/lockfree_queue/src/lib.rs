use anyhow::{anyhow, Result as AnyhowResult};
use std::{error::Error, fmt::{Display, Formatter}, time::Duration};

#[derive(Debug)]
pub struct NoReceiver {}

impl Display for NoReceiver {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("Receiver disconnected")
    }
}

#[derive(Debug)]
pub enum SendError {
    NoReceiver,
    DescriptiveError(anyhow::Error),
}

pub struct Sender<T> {
    snd: lockfree::prelude::mpsc::Sender<T>,
}

pub struct Receiver<T> {
    recv: lockfree::prelude::mpsc::Receiver<T>,
}

impl<T> Receiver<T> {
    pub fn recv_timeout(&mut self, timeout: Duration) -> AnyhowResult<()> {
        todo!();
    }

    pub fn recv(&mut self) -> AnyhowResult<T> {
        todo!();
    }
}

impl<T> Sender <T> {
    pub fn send(&self, obj : T) -> Result<(), SendError> {
        todo!();
    }
}

pub fn create<T>() -> AnyhowResult<(Sender<T>, Receiver<T>)> {
    todo!();
}