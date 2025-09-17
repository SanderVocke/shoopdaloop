use anyhow::{anyhow, Result};
use lockfree::prelude::mpsc as lockfree_mpsc;
use std::{sync::mpsc, time::Duration};
use std::fmt::Debug;

type Command<ProcessingT> = Box<dyn Fn(&mut ProcessingT) -> Result<()> + Send + 'static>;
type Sender<ProcessingT> = lockfree_mpsc::Sender<Command<ProcessingT>>;
type Receiver<ProcessingT> = lockfree_mpsc::Receiver<Command<ProcessingT>>;

pub trait HasCommandQueueSender<ProcessingT> {
    fn process_command_sender<'a>(&'a self) -> &'a Sender<ProcessingT>;

    /// Queue a command to be executed on the process thread.
    fn queue_command(
        &self,
        command: impl Fn(&mut ProcessingT) -> Result<()> + Send + 'static,
    ) -> Result<()> {
        let boxed = Box::new(command);
        self.process_command_sender()
            .send(boxed)
            .map_err(|_| anyhow!("failed to queue command"))?;
        Ok(())
    }

    /// Wait until the command queue has been processed at least once
    /// from the moment of calling.
    /// This is done by simply inserting an empty command into the queue
    /// and waiting for it to call back.
    fn wait_process(&self, timeout: Duration) -> Result<()> {
        let (sender, receiver) = mpsc::channel::<()>();
        let cmd = move |_: &mut ProcessingT| -> Result<()> {
            sender
                .send(())
                .map_err(|e| anyhow!("Failed to call back from process thread: {e}"))
        };
        self.queue_command(cmd)?;
        receiver.recv_timeout(timeout)
            .map_err(|e| anyhow!("Timeout expired: {e}"))?;

        Ok(())
    }
}
