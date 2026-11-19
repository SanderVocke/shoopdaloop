use super::Error;
use crate::records::{LoopRecord, Mode};

/// Execution operands contain settled intent, never a reference to loop state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum MidiJob {
    Begin {
        frame: u64,
    },
    FinishCapture {
        context: u16,
    },
    BeginCapture {
        context: u16,
        frame: u64,
    },
    Observe,
    Append {
        context: u16,
    },
    Cleanup {
        context: u16,
        record: LoopRecord,
    },
    Claim {
        context: u16,
        playing: bool,
        established: bool,
    },
    ReadUntil {
        context: u16,
        record: LoopRecord,
    },
    Output,
    Complete {
        context: u16,
        mode: Mode,
    },
    Maintain,
}

pub(crate) struct Execution<'a, F> {
    pub session: &'a mut super::MidiSession,
    pub events: &'a [super::InputEvent],
    pub sink: &'a mut F,
    pub base: u32,
    pub quantum: u32,
    pub error: Option<Error>,
}
impl<F: FnMut(super::OutputEvent) -> Result<(), Error>> Execution<'_, F> {
    pub fn execute(&mut self, job: MidiJob) -> Result<(), crate::Error> {
        if let Err(error) =
            self.session
                .execute_job(job, self.base, self.quantum, self.events, self.sink)
        {
            self.error.get_or_insert(error);
        }
        if job == MidiJob::Maintain {
            self.base += self.quantum;
        }
        Ok(())
    }
}
