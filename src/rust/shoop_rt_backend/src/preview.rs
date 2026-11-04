use crate::{
    persistence::{Job, Work},
    prepared::{Upload, MAX_MEDIA_BYTES},
    requests::Purpose,
    EngineClock, RtBackend,
};
use alloc::string::ToString;
use alloc::{collections::VecDeque, sync::Arc, vec, vec::Vec};
use anyhow::{anyhow, bail, Result};
use shoop_backend::*;
use shoop_rt_protocol::Request;
use shoop_rt_transport::carrier::Carrier;

pub(crate) struct Preview {
    pub id: u64,
    pub job: Job,
    pub(crate) stopped: bool,
    stop_sent: bool,
}
impl<C: Carrier, K: EngineClock> RtBackend<C, K> {
    pub(crate) fn stop_preview(&mut self) {
        if let Some(preview) = &mut self.preview {
            if !preview.stopped {
                preview.stopped = true;
                // Cancel unsent upload work, but retain in-flight receipts and release ownership normally.
                preview.job.fail("Preview stopped".into());
            }
        }
    }
    pub(crate) fn advance_preview(&mut self) {
        let Some(preview) = &mut self.preview else {
            return;
        };
        if preview.stopped && !preview.stop_sent {
            preview.stop_sent = true;
            self.requests
                .push(Request::PreviewStop, Purpose::CancelReads);
        }
        if let Err(error) = preview.job.prepare() {
            preview.job.fail(error.to_string());
        }
        let bytes = self
            .limits
            .map_or(0, |l| usize::from(l.content_chunk_bytes));
        if bytes == 0 {
            return;
        }
        match preview.job.next(bytes, self.status.callbacks, 16) {
            Ok(Some(request)) => {
                self.requests.push(request, Purpose::Preview);
            }
            Ok(None) => {}
            Err(error) => preview.job.fail(error.to_string()),
        }
        if preview.job.finished && preview.stopped {
            self.preview = None;
        }
    }
    pub(crate) fn bus_preview(
        &mut self,
        bus: BackendBusId,
        request_id: u64,
        rate: u32,
        samples: Arc<Vec<f32>>,
    ) -> Result<BackendAsyncResult<()>> {
        if let Some(preview) = &self.preview {
            if preview.stopped || preview.id != request_id {
                if preview.job.finished {
                    self.preview = None;
                    return self.bus_preview(bus, request_id, rate, samples);
                }
                return Ok(BackendAsyncResult::Pending(BackendOperationProgress {
                    key: request_id,
                    kind: BackendOperationKind::LoopContentReplacement,
                    completed: 0,
                    total: None,
                }));
            }
            if !preview.job.finished {
                return Ok(BackendAsyncResult::Pending(BackendOperationProgress {
                    key: request_id,
                    kind: BackendOperationKind::LoopContentReplacement,
                    completed: preview.job.completed,
                    total: None,
                }));
            }
            let preview = self.preview.take().unwrap();
            if let Some(error) = preview.job.error {
                bail!("{error}");
            }
            return Ok(BackendAsyncResult::Ready(()));
        }
        self.ready()?;
        if self.driver.is_none_or(|d| d.sample_rate != rate)
            || samples.is_empty()
            || samples.len() > 4_000_000
        {
            bail!("invalid preview rate or size");
        }
        let slot = self
            .mirror
            .buses
            .get(&bus)
            .filter(|b| b.confirmed && !b.removing)
            .ok_or_else(|| anyhow!("preview bus is unavailable"))?
            .slot;
        let token = self.next_persistence;
        self.next_persistence = token
            .checked_add(1)
            .ok_or_else(|| anyhow!("preview identity exhausted"))?;
        let channels = usize::from(self.limits().strip_audio_channels);
        let content = Arc::new(BackendLoopContent {
            source_id: token,
            length: samples.len() as u32,
            gain: 1.,
            balance: 0.,
            audio: vec![BackendAudioContent {
                mode: BackendChannelMode::Direct,
                samples,
                gain: 1.,
                start_offset: 0,
                capture_alignment_frames: 0,
                preplay: 0,
            }],
            midi: Vec::new(),
        });
        let upload = Upload::new(token, false, Arc::clone(&content), channels)?;
        let work = VecDeque::from([
            Work::Upload {
                upload,
                begun: false,
            },
            Work::Request(Request::PreviewStart {
                request_id: token,
                bus: slot,
                content: token,
                frames: u64::from(content.length),
            }),
        ]);
        self.preview = Some(Preview {
            id: request_id,
            stopped: false,
            stop_sent: false,
            job: Job::new(
                token,
                work,
                vec![token],
                false,
                [content],
                channels,
                MAX_MEDIA_BYTES,
            )?,
        });
        self.bus_preview(bus, request_id, rate, Arc::new(Vec::new()))
    }
}
