use super::queue::allocate_scope;
use super::{GraphCompiler, PreparedGraph, Quantum};
use crate::{Epoch, Error, Frame};
use rtrb::{Consumer, Producer, PushError, RingBuffer};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphTicket {
    scope: u32,
    sequence: u64,
}

pub struct GraphControl {
    compiler: GraphCompiler,
    submissions: Producer<Submission>,
    receipts: Consumer<GraphReceipt>,
    scope: u32,
    sequence: u64,
    outstanding: bool,
    closed: bool,
}

#[derive(Debug)]
pub struct GraphRejected {
    pub graph: PreparedGraph,
    pub error: Error,
}

/// Success returns retired routing; failure returns the unchanged candidate.
#[derive(Debug)]
pub struct GraphReceipt {
    pub ticket: GraphTicket,
    pub requested: Frame,
    pub effective: Option<Frame>,
    pub actual: Frame,
    pub result: Result<Epoch, Error>,
    pub graph: PreparedGraph,
}

pub(super) struct Submission {
    pub ticket: GraphTicket,
    pub requested: Frame,
    pub expected: Epoch,
    pub effective: Option<Frame>,
    pub graph: PreparedGraph,
}

pub(super) struct Inbox {
    submissions: Consumer<Submission>,
    receipts: Producer<GraphReceipt>,
    pending: Option<Submission>,
    ready: Option<GraphReceipt>,
    scope: u32,
    closed: bool,
}

pub(super) fn channel(graph: &PreparedGraph) -> Result<(GraphControl, Inbox), Error> {
    let scope = allocate_scope()?;
    let (mut tx, mut rx) = RingBuffer::new(1);
    let (mut reply_tx, mut reply_rx) = RingBuffer::new(1);
    let compiler = graph.compiler();
    let ticket = GraphTicket { scope, sequence: 0 };
    tx.push(Submission {
        ticket,
        requested: Frame(0),
        expected: Epoch(0),
        effective: None,
        graph: compiler.compile(graph.description())?,
    })
    .map_err(|_| Error::InvalidState)?;
    let dummy = rx.pop().map_err(|_| Error::InvalidState)?;
    reply_tx
        .push(GraphReceipt {
            ticket,
            requested: Frame(0),
            effective: None,
            actual: Frame(0),
            result: Err(Error::InvalidState),
            graph: dummy.graph,
        })
        .map_err(|_| Error::InvalidState)?;
    drop(reply_rx.pop().map_err(|_| Error::InvalidState)?);
    Ok((
        GraphControl {
            compiler,
            submissions: tx,
            receipts: reply_rx,
            scope,
            sequence: 0,
            outstanding: false,
            closed: false,
        },
        Inbox {
            submissions: rx,
            receipts: reply_tx,
            pending: None,
            ready: None,
            scope,
            closed: false,
        },
    ))
}

impl GraphControl {
    pub fn compiler(&self) -> &GraphCompiler {
        &self.compiler
    }
    /// A single credit remains occupied until the terminal receipt is consumed.
    pub fn submit(
        &mut self,
        requested: Frame,
        expected: Epoch,
        graph: PreparedGraph,
    ) -> Result<GraphTicket, GraphRejected> {
        let error = if self.closed || self.submissions.is_abandoned() {
            Some(Error::Disconnected)
        } else if self.outstanding {
            Some(Error::Full)
        } else if !self.compiler.accepts(&graph) {
            Some(Error::InvalidId)
        } else {
            None
        };
        if let Some(error) = error {
            return Err(GraphRejected { graph, error });
        }
        let Some(next) = self.sequence.checked_add(1) else {
            return Err(GraphRejected {
                graph,
                error: Error::Exhausted,
            });
        };
        let ticket = GraphTicket {
            scope: self.scope,
            sequence: self.sequence,
        };
        let submission = Submission {
            ticket,
            requested,
            expected,
            effective: None,
            graph,
        };
        match self.submissions.push(submission) {
            Ok(()) => {
                self.sequence = next;
                self.outstanding = true;
                Ok(ticket)
            }
            Err(PushError::Full(s)) => Err(GraphRejected {
                graph: s.graph,
                error: Error::Full,
            }),
        }
    }
    pub fn try_receipt(&mut self) -> Option<GraphReceipt> {
        let r = self.receipts.pop().ok()?;
        self.outstanding = false;
        Some(r)
    }
    pub fn outstanding(&self) -> bool {
        self.outstanding
    }
}

impl Inbox {
    fn flush(&mut self) {
        if let Some(r) = self.ready.take() {
            if let Err(PushError::Full(r)) = self.receipts.push(r) {
                self.ready = Some(r);
            }
        }
    }
    pub fn complete(&mut self, s: Submission, actual: Frame, result: Result<Epoch, Error>) {
        self.ready = Some(GraphReceipt {
            ticket: s.ticket,
            requested: s.requested,
            effective: s.effective,
            actual,
            result,
            graph: s.graph,
        });
        self.flush();
    }
    pub fn begin_callback(&mut self, next: Frame, quantum: Quantum) {
        self.flush();
        if self.closed || self.pending.is_some() || self.ready.is_some() {
            return;
        }
        if let Ok(mut s) = self.submissions.pop() {
            match quantum.effective(s.requested, next) {
                Ok(effective) => {
                    s.effective = Some(effective);
                    self.pending = Some(s);
                }
                Err(e) => self.complete(s, next, Err(e)),
            }
        }
    }
    pub fn take_due(&mut self, frame: Frame) -> Option<Submission> {
        if self
            .pending
            .as_ref()
            .is_some_and(|s| s.effective.is_some_and(|e| e <= frame))
        {
            self.pending.take()
        } else {
            None
        }
    }
    pub fn shutdown(&mut self, control: &mut GraphControl, actual: Frame) -> Result<(), Error> {
        if control.scope != self.scope {
            return Err(Error::InvalidId);
        }
        control.closed = true;
        self.closed = true;
        self.flush();
        let s = self.pending.take().or_else(|| self.submissions.pop().ok());
        if let Some(s) = s {
            self.complete(s, actual, Err(Error::Disconnected));
        }
        Ok(())
    }
}
