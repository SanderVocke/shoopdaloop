//! Acknowledged persistence steps and a bounded write pipeline, independent of mutation receipts.
use crate::{prepared::Upload, requests::Purpose, EngineClock, RtBackend};
use anyhow::{anyhow, bail, Result};
use shoop_rt_protocol::{Outcome, Refusal, Request};
use shoop_rt_transport::carrier::Carrier;
use std::collections::VecDeque;

pub(crate) enum Work {
    Request(Request),
    Upload { upload: Upload, begun: bool },
}
pub(crate) struct Job {
    pub id: u64,
    pub work: VecDeque<Work>,
    pub error: Option<String>,
    pub finished: bool,
    pub committed: bool,
    pub completed: usize,
    active: Vec<Request>,
    uploads: Vec<u64>,
    cleaning: bool,
    retry_after: Option<u64>,
    retries: u16,
    session: bool,
}
impl Job {
    pub fn new(id: u64, work: VecDeque<Work>, uploads: Vec<u64>, session: bool) -> Self {
        Self {
            id,
            work,
            error: None,
            finished: false,
            committed: false,
            completed: 0,
            active: Vec::with_capacity(8),
            uploads,
            cleaning: false,
            retry_after: None,
            retries: 0,
            session,
        }
    }
    pub fn fail(&mut self, message: String) {
        self.error.get_or_insert(message);
    }
    pub fn disconnected(&mut self, message: String) {
        self.fail(format!(
            "{message}; the last engine mutation outcome may be unknown"
        ));
        self.active.clear();
        self.work.clear();
        self.finished = true;
    }
    pub fn cancel(&mut self) -> Result<()> {
        if self.committed {
            bail!("the replacement commit is already admitted");
        }
        self.fail("session replacement cancelled".to_owned());
        Ok(())
    }
    fn cleanup(&mut self) {
        self.cleaning = true;
        self.work.clear();
        if self.session && !self.committed {
            self.work
                .push_back(Work::Request(Request::ReplacementCancel { id: self.id }));
        }
        for id in self.uploads.drain(..) {
            self.work
                .push_back(Work::Request(Request::ContentRelease { id }));
        }
    }
    pub fn answered(&mut self, request: Request, outcome: Outcome, callbacks: u64) {
        let Some(index) = self.active.iter().position(|active| *active == request) else {
            return;
        };
        self.active.swap_remove(index);
        if self.retries < 1024
            && !matches!(request, Request::ContentWrite { .. })
            && (outcome == Outcome::Refused(Refusal::Overloaded)
                || (outcome == Outcome::Refused(Refusal::Busy)
                    && matches!(request, Request::ReplacementBegin { .. })))
        {
            self.retries += 1;
            self.retry_after = Some(callbacks.saturating_add(1));
            self.work.push_front(Work::Request(request));
            return;
        }
        if let Outcome::Refused(refusal) = outcome {
            if matches!(
                request,
                Request::ReplacementCommit { .. } | Request::ContentApply { .. }
            ) {
                self.committed = false;
            }
            if !self.cleaning {
                let operation = match request {
                    Request::ContentWrite { .. } => "content chunk".to_owned(),
                    _ => format!("{request:?}"),
                };
                self.fail(format!("engine refused {operation}: {refusal:?}"));
            }
        } else if matches!(
            request,
            Request::ReplacementCommit { .. } | Request::ContentApply { .. }
        ) {
            self.committed = true;
        }
        self.completed += 1;
    }
    fn next(&mut self, chunk_bytes: usize, callbacks: u64) -> Result<Option<Request>> {
        if self.finished {
            return Ok(None);
        }
        if !self.active.is_empty() {
            if self.error.is_some() || self.active.len() == 8 {
                return Ok(None);
            }
            let Some(Request::ContentWrite { id, .. }) = self.active.first() else {
                return Ok(None);
            };
            let id = *id;
            if !self.active.iter().all(|request| matches!(request, Request::ContentWrite { id: active, .. } if *active == id)) { return Ok(None); }
            let Some(Work::Upload {
                upload,
                begun: true,
            }) = self.work.front_mut()
            else {
                return Ok(None);
            };
            if upload.description.id != id {
                return Ok(None);
            }
            let Some((offset, data)) = upload.chunk(chunk_bytes) else {
                return Ok(None);
            };
            let request = Request::ContentWrite { id, offset, data };
            self.active.push(request);
            return Ok(Some(request));
        }
        if self.error.is_some() && !self.cleaning {
            self.cleanup();
        }
        if self.retry_after.is_some_and(|after| callbacks < after) {
            return Ok(None);
        }
        self.retry_after = None;
        let request = match self.work.front_mut() {
            Some(Work::Request(_)) => {
                let Some(Work::Request(request)) = self.work.pop_front() else {
                    unreachable!()
                };
                request
            }
            Some(Work::Upload { upload, begun }) => {
                let id = upload.description.id;
                if !*begun {
                    *begun = true;
                    Request::ContentBegin(upload.description)
                } else if let Some(message) = upload.initial()? {
                    Request::ContentInitial { id, message }
                } else if let Some((offset, data)) = upload.chunk(chunk_bytes) {
                    Request::ContentWrite { id, offset, data }
                } else {
                    self.work.pop_front();
                    Request::ContentFinish { id }
                }
            }
            None => {
                if !self.cleaning {
                    self.cleanup();
                    return self.next(chunk_bytes, callbacks);
                }
                self.finished = true;
                return Ok(None);
            }
        };
        if matches!(
            request,
            Request::ReplacementCommit { .. } | Request::ContentApply { .. }
        ) {
            self.committed = true;
        }
        self.active.push(request);
        Ok(Some(request))
    }
}
impl<C: Carrier, K: EngineClock> RtBackend<C, K> {
    pub(crate) fn advance_persistence(&mut self) {
        let chunk_bytes = self
            .limits
            .map_or(0, |limits| usize::from(limits.content_chunk_bytes));
        if chunk_bytes == 0 || !self.is_ready() {
            return;
        }
        let Some(replacement) = self.replacement.as_mut() else {
            return;
        };
        match replacement.job.next(chunk_bytes, self.status.callbacks) {
            Ok(Some(request)) => {
                self.requests.push(request, Purpose::Persistence);
            }
            Ok(None) => {}
            Err(error) => replacement.job.fail(error.to_string()),
        }
        if replacement.job.finished && replacement.job.error.is_none() && !replacement.published {
            replacement.published = true;
            if let Some(id) = replacement.loop_target {
                let content = replacement.content.remove(&id).expect("prepared content");
                if let Some(entry) = self.mirror.loops.get_mut(&id) {
                    entry.content += 1;
                    entry.view.length = u64::from(content.length);
                }
                self.canonical_content.insert(id, content);
                self.canonical_imports.insert(id);
                self.reads.forget(id);
            }
            if let Some(image) = replacement.candidate.take() {
                self.mirror = image;
                self.canonical_content = std::mem::take(&mut replacement.content);
                self.canonical_imports = self.canonical_content.keys().copied().collect();
                self.connection_failures
                    .append(&mut replacement.connection_failures);
                self.reads.clear();
            }
        }
    }
    pub(crate) fn persistence_progress(
        &mut self,
    ) -> Result<shoop_backend::BackendAsyncResult<shoop_backend::BackendSessionReplacement>> {
        self.pump();
        let replacement = self
            .replacement
            .as_ref()
            .ok_or_else(|| anyhow!("no session replacement"))?;
        if !replacement.job.finished {
            return Ok(shoop_backend::BackendAsyncResult::Pending(
                shoop_backend::BackendOperationProgress {
                    key: replacement.job.id,
                    kind: if replacement.candidate.is_some() {
                        shoop_backend::BackendOperationKind::SessionReplacement
                    } else {
                        shoop_backend::BackendOperationKind::LoopContentReplacement
                    },
                    completed: replacement.job.completed,
                    total: None,
                },
            ));
        }
        let replacement = self.replacement.take().expect("finished replacement");
        match replacement.job.error {
            Some(message) => Err(anyhow!(message)),
            None => Ok(shoop_backend::BackendAsyncResult::Ready(
                replacement.mapping,
            )),
        }
    }
}
