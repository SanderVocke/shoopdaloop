use anyhow::{anyhow, Result};
use lockfree_queue::create;
use lockfree_queue::Receiver as BaseReceiver;
use lockfree_queue::Sender as BaseSender;
use std::fmt::Debug;
use std::time::Duration;

pub type Command<ProcessingT> = Box<dyn Fn(&mut ProcessingT) -> Result<()> + Send + 'static>;
pub type Sender<ProcessingT> = BaseSender<Command<ProcessingT>>;
pub type Receiver<ProcessingT> = BaseReceiver<Command<ProcessingT>>;

pub trait HasCommandQueueSender<ProcessingT> {
    fn process_command_sender<'a>(&'a self) -> &'a Sender<ProcessingT>;

    /// Queue a command to be executed on the process thread.
    fn queue_command(
        &self,
        command: impl Fn(&mut ProcessingT) -> Result<()> + Send + 'static,
    ) -> Result<()> {
        let boxed = Box::new(command);
        self.process_command_sender()
            .send(boxed)?;
        Ok(())
    }

    /// Wait until the command queue has been processed at least once
    /// from the moment of calling.
    /// This is done by simply inserting an empty command into the queue
    /// and waiting for it to call back.
    fn wait_process(&self, timeout: Duration) -> Result<()> {
        let (sender, mut receiver) = create()?;
        let cmd = move |_: &mut ProcessingT| -> Result<()> { sender.send(()) };
        self.queue_command(cmd)?;
        receiver.recv_timeout(timeout)?;
        Ok(())
    }
}
