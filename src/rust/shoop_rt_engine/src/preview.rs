//! Transient bus playback; the upload remains pinned until playback stops.
use crate::endpoint::{
    completion_refusal, execution_outcome, ControlEndpoint, Purpose, RuntimeResult,
};
use crate::node::EngineOp;
use core::sync::atomic::Ordering;
use rt_runtime::resource::Cursor;
use shoop_loops::runtime::{Operation, Reply};
use shoop_rt_protocol::{BusId, Outcome, Refusal};

pub(crate) struct Preview {
    correlation: Option<u64>,
    request_id: u64,
    pub content: u64,
    bus: BusId,
    frames: u64,
    cursor: Option<Cursor>,
    pending: bool,
    begun: bool,
    cancelled: bool,
    stopping: bool,
    failure: Option<Refusal>,
}
impl ControlEndpoint {
    pub(crate) fn start_preview(
        &mut self,
        correlation: u64,
        request_id: u64,
        bus: BusId,
        content: u64,
        frames: u64,
    ) -> Result<Option<Outcome>, Refusal> {
        if self.graph_epoch == rt_runtime::Epoch(0) {
            return Err(Refusal::Busy);
        }
        if self.preview.is_some() {
            return Err(Refusal::Busy);
        }
        if request_id == 0 || frames == 0 || self.model.bus(bus).is_none() {
            return Err(Refusal::InvalidId);
        }
        let source = self.uploads.source_raw(content, false, frames)?;
        self.submit_loop(
            self.at(),
            Operation::NodePairSourceCursor { id: source },
            Purpose::Preview,
        )?;
        self.preview = Some(Preview {
            correlation: Some(correlation),
            request_id,
            content,
            bus,
            frames,
            cursor: None,
            pending: true,
            begun: false,
            cancelled: false,
            stopping: false,
            failure: None,
        });
        Ok(None)
    }
    pub(crate) fn disconnect_preview(&mut self) {
        if let Some(p) = &mut self.preview {
            p.correlation = None;
            p.cancelled = true;
        }
    }
    pub(crate) fn cancel_preview(&mut self) {
        if let Some(preview) = &mut self.preview {
            preview.cancelled = true;
        }
    }
    pub(crate) fn advance_preview(&mut self) {
        let Some(mut p) = self.preview.take() else {
            return;
        };
        p.cancelled |= !self.connected || self.model.bus(p.bus).is_none();
        if p.pending {
            self.preview = Some(p);
            return;
        }
        let done = p.begun && self.cells.preview_finished.load(Ordering::Acquire) == p.request_id;
        if done || p.stopping || ((p.cancelled || p.failure.is_some()) && !p.begun) {
            if let Some(correlation) = p.correlation.filter(|_| self.connected) {
                self.answer(
                    correlation,
                    p.failure.map_or(Outcome::Done, Outcome::Refused),
                );
            }
            return;
        }
        let operation = if p.cancelled || p.failure.is_some() {
            EngineOp::PreviewStop
        } else if let Some(cursor) = p.cursor {
            EngineOp::PreviewStart {
                cursor,
                frames: p.frames,
                request_id: p.request_id,
                source_channels: self.limits.strip_audio_channels,
            }
        } else {
            self.preview = Some(p);
            return;
        };
        if !p.begun || p.cancelled || p.failure.is_some() {
            if self
                .submit_op(
                    self.at(),
                    self.pool.bus(p.bus.0.index as usize),
                    operation,
                    Purpose::Preview,
                )
                .is_ok()
            {
                p.pending = true;
                if p.begun {
                    p.stopping = true;
                } else {
                    p.begun = true;
                }
            }
        }
        self.preview = Some(p);
    }
    pub(crate) fn preview_result(&mut self, finished: bool, result: RuntimeResult) {
        let mut cursor = None;
        let failure = match &result {
            RuntimeResult::Completion(outcome, ..) => completion_refusal(*outcome),
            RuntimeResult::Execution(execution) => {
                if let Some(Reply::Cursor(value)) = execution.operation_result {
                    cursor = Some(value);
                }
                match execution_outcome(execution) {
                    Outcome::Refused(reason) => Some(reason),
                    _ => None,
                }
            }
        };
        if let Some(p) = &mut self.preview {
            if let Some(cursor) = cursor {
                p.cursor = Some(cursor);
            }
            if let Some(failure) = failure {
                p.failure = Some(failure);
            }
            if finished {
                p.pending = false;
            }
        }
        if let RuntimeResult::Completion(_, command, retired) = result {
            self.reclaim(command);
            self.retire(retired);
        }
    }
}
