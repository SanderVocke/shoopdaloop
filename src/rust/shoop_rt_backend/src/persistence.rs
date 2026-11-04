//! Acknowledged persistence steps and a bounded write pipeline, independent of mutation receipts.
use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

use crate::{
    prepared::{Prepared, Upload, Validation},
    requests::Purpose,
    EngineClock, RtBackend,
};
use alloc::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};
use anyhow::{anyhow, bail, Result};
use shoop_rt_protocol::{Outcome, Refusal, Request};
use shoop_rt_transport::carrier::Carrier;

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
    pub family: Option<shoop_backend::BackendLoopId>,
    validation: VecDeque<Validation>,
    pub prepared: BTreeMap<u64, Prepared>,
    pub revisions: BTreeMap<shoop_rt_protocol::LoopSlotId, u64>,
    canonical_bytes: u64,
    rendered_bytes: u64,
    canonical_limit: u64,
}
impl Job {
    pub fn new(
        id: u64,
        work: VecDeque<Work>,
        uploads: Vec<u64>,
        session: bool,
        content: impl IntoIterator<Item = Arc<shoop_backend::BackendLoopContent>>,
        channels: usize,
        canonical_limit: u64,
    ) -> Result<Self> {
        let validation = content
            .into_iter()
            .map(|image| Validation::new(image, channels))
            .collect::<Result<VecDeque<_>>>()?;
        Ok(Self {
            id,
            work,
            error: None,
            finished: false,
            committed: false,
            completed: 0,
            active: Vec::with_capacity(shoop_rt_protocol::content::CONTENT_WRITE_WINDOW),
            uploads,
            cleaning: false,
            retry_after: None,
            retries: 0,
            session,
            family: None,
            validation,
            prepared: BTreeMap::new(),
            revisions: BTreeMap::new(),
            canonical_bytes: 0,
            rendered_bytes: 0,
            canonical_limit,
        })
    }
    pub fn prepare(&mut self) -> Result<()> {
        if self.error.is_some() {
            return Ok(());
        }
        let Some(validation) = self.validation.front_mut() else {
            return Ok(());
        };
        if validation.step(256 * 1024)? {
            let prepared = self.validation.pop_front().unwrap().finish()?;
            self.canonical_bytes = self
                .canonical_bytes
                .checked_add(prepared.bytes())
                .ok_or_else(|| anyhow!("canonical size overflow"))?;
            self.rendered_bytes = self
                .rendered_bytes
                .checked_add(prepared.rendered_bytes())
                .ok_or_else(|| anyhow!("prepared size overflow"))?;
            if self.canonical_bytes > self.canonical_limit
                || self.rendered_bytes > crate::prepared::MAX_MEDIA_BYTES
            {
                bail!("canonical or prepared session media exceeds the 256 MiB budget");
            }
            if self
                .prepared
                .insert(prepared.image().source_id, prepared)
                .is_some()
            {
                bail!("duplicate content identity");
            }
        }
        Ok(())
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
        self.validation.clear();
        if self.session && !self.committed {
            self.work
                .push_back(Work::Request(Request::ReplacementCancel { id: self.id }));
        }
        if self.family.is_some() && !self.committed {
            self.work
                .push_back(Work::Request(Request::ContentFamilyCancel { id: self.id }));
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
        if matches!(request, Request::ContentApply { .. }) && outcome == Outcome::Done {
            self.fail("engine omitted the applied content revision".to_owned());
        }
        if let (Request::ContentApply { loop_id, .. }, Outcome::ContentApplied { revision }) =
            (request, outcome)
        {
            self.revisions.insert(loop_id, revision);
        }
        if let (Request::ContentFamilyEdit { edit, .. }, Outcome::ContentPrepared { revision }) =
            (request, outcome)
        {
            self.revisions.insert(edit.loop_id, revision);
        }
        if matches!(request, Request::ContentFamilyEdit { edit, .. } if edit.replace)
            && outcome == Outcome::Done
        {
            self.fail("engine omitted the prepared content revision".to_owned());
        }
        if self.retries < 1024
            && !matches!(request, Request::ContentWrite { .. })
            && (outcome == Outcome::Refused(Refusal::Overloaded)
                || (outcome == Outcome::Refused(Refusal::Busy)
                    && matches!(
                        request,
                        Request::ReplacementBegin { .. } | Request::ContentFamilyBegin { .. }
                    )))
        {
            self.retries += 1;
            self.retry_after = Some(callbacks.saturating_add(1));
            self.work.push_front(Work::Request(request));
            return;
        }
        if let Outcome::Refused(refusal) = outcome {
            if matches!(
                request,
                Request::ReplacementCommit { .. }
                    | Request::ContentApply { .. }
                    | Request::ContentFamilyCommit { .. }
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
            Request::ReplacementCommit { .. }
                | Request::ContentApply { .. }
                | Request::ContentFamilyCommit { .. }
        ) {
            self.committed = true;
        }
        self.completed += 1;
    }
    pub(crate) fn next(
        &mut self,
        chunk_bytes: usize,
        callbacks: u64,
        window: usize,
    ) -> Result<Option<Request>> {
        if self.finished || (self.error.is_none() && !self.validation.is_empty()) {
            return Ok(None);
        }
        if !self.active.is_empty() {
            if self.error.is_some() || self.active.len() >= window {
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
                upload.certify(
                    self.prepared
                        .get(&upload.source())
                        .ok_or_else(|| anyhow!("missing prepared content proof"))?,
                )?;
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
                    return self.next(chunk_bytes, callbacks, window);
                }
                self.finished = true;
                return Ok(None);
            }
        };
        if matches!(
            request,
            Request::ReplacementCommit { .. }
                | Request::ContentApply { .. }
                | Request::ContentFamilyCommit { .. }
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
        let window = if self
            .capabilities
            .contains(shoop_rt_protocol::Capabilities::WIDE_CONTENT_UPLOAD)
        {
            shoop_rt_protocol::content::CONTENT_WRITE_WINDOW
        } else {
            shoop_rt_protocol::content::LEGACY_CONTENT_WRITE_WINDOW
        }
        .min(
            self.limits
                .map_or(1, |limits| usize::from(limits.reliable_credits)),
        )
        .max(1);
        match replacement
            .job
            .next(chunk_bytes, self.status.callbacks, window)
        {
            Ok(Some(request)) => {
                self.requests.push(request, Purpose::Persistence);
            }
            Ok(None) => {}
            Err(error) => replacement.job.fail(error.to_string()),
        }
        if replacement.job.finished && replacement.job.error.is_none() && !replacement.published {
            replacement.published = true;
            if replacement.loop_target.is_some() {
                for (id, content) in core::mem::take(&mut replacement.content) {
                    if let Some(entry) = self.mirror.loops.get_mut(&id) {
                        entry.content += 1;
                        entry.rt_content_revision = replacement.job.revisions[&entry.slot];
                        entry.view.length = u64::from(content.length);
                    }
                    self.prepared_content
                        .insert(id, replacement.job.prepared[&content.source_id].clone());
                    self.canonical_content.insert(id, content);
                    self.canonical_imports.insert(id);
                    self.reads.forget(id);
                }
            }
            if let Some(image) = replacement.candidate.take() {
                self.mirror = image;
                self.prepared_content = replacement
                    .content
                    .iter()
                    .map(|(id, image)| (*id, replacement.job.prepared[&image.source_id].clone()))
                    .collect();
                self.canonical_content = core::mem::take(&mut replacement.content);
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
