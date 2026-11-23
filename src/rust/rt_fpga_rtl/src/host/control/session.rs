use super::descriptor;
use crate::{
    host::{Client, Error, RegisterBus, Request, Upload, UploadStep, registers::*},
    target::ProgramImage,
};
use shoop_rt_protocol::immediate::SwitchProgram;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActiveProgram {
    pub revision: u64,
    pub frame: u64,
}

/// Produced only after a matched switch completion has returned from the fabric.
pub struct FencedSwitch {
    requested: SwitchProgram,
    selected: ActiveProgram,
}
impl FencedSwitch {
    pub(super) fn new(requested: SwitchProgram, words: [u32; HOST_PAYLOAD_WORDS]) -> Self {
        let join = |low, high| u64::from(low) | u64::from(high) << 32;
        Self {
            requested,
            selected: ActiveProgram {
                frame: join(words[0], words[1]),
                revision: join(words[2], words[3]),
            },
        }
    }
    pub(super) fn frame(&self) -> u64 {
        self.selected.frame
    }
}

/// Owns the staged image and retains its predecessor until the fabric memory fence returns.
pub struct ProgramSession<B> {
    client: Client<B>,
    active: Option<Upload>,
    staged: Option<Upload>,
    uploaded: bool,
    failed: bool,
}

impl<B: RegisterBus> ProgramSession<B> {
    pub fn new(client: Client<B>) -> Self {
        Self {
            client,
            active: None,
            staged: None,
            uploaded: false,
            failed: false,
        }
    }

    pub fn client(&mut self) -> &mut Client<B> {
        &mut self.client
    }

    pub fn active(&self) -> Option<&ProgramImage> {
        self.active.as_ref().map(|upload| &upload.image)
    }

    pub fn staged(&self) -> Option<&ProgramImage> {
        self.staged.as_ref().map(|upload| &upload.image)
    }

    pub fn active_and_client(&mut self) -> (Option<&ProgramImage>, &mut Client<B>) {
        (self.active.as_ref().map(|u| &u.image), &mut self.client)
    }

    pub fn stage(&mut self, upload: Upload) -> Result<(), Upload> {
        let current = self.active().map_or(0, |image| image.layout.revision);
        if self.staged.is_some() || self.failed || upload.image.layout.expected != current {
            return Err(upload);
        }
        self.staged = Some(upload);
        self.uploaded = false;
        Ok(())
    }

    /// Transfer at most one write or media reclamation per visit.
    pub fn upload_step(
        &mut self,
        time_left: impl FnMut() -> bool,
    ) -> Result<Option<SwitchProgram>, Error<B::Error>> {
        if self.failed {
            return Err(Error::Fault);
        }
        let Some(upload) = self.staged.as_mut() else {
            return Err(Error::InvalidRequest);
        };
        let request = match upload.next_step() {
            Some(UploadStep::Configure(write)) => {
                Request::configuration_write(write).ok_or(Error::InvalidRequest)
            }
            Some(UploadStep::ClearView(view)) => Ok(Request::clear_view(view)),
            None => {
                self.uploaded = true;
                return Ok(Some(descriptor(&upload.image)));
            }
        };
        self.exchange(request, time_left)?;
        Ok(None)
    }

    pub fn activate(
        &mut self,
        requested: SwitchProgram,
        mut time_left: impl FnMut() -> bool,
    ) -> Result<(ActiveProgram, Option<Upload>), Error<B::Error>> {
        if self.failed {
            return Err(Error::Fault);
        }
        let Some(staged) = self
            .staged
            .as_ref()
            .filter(|u| self.uploaded && descriptor(&u.image) == requested)
        else {
            return Err(Error::InvalidRequest);
        };
        let layout = &staged.image.layout;
        if !layout.composite_cuts().is_empty() {
            let mut window =
                super::ControlWindow::begin(&mut self.client, layout.expected, &mut time_left)?;
            let result = (|| {
                for &cut in layout.composite_cuts() {
                    if !window.composite_cut_ready(cut)? {
                        return Ok(None);
                    }
                }
                window.switch_program(requested).map(Some)
            })();
            let release = window.release();
            let fence = match (result, release) {
                (Ok(Some(fence)), Ok(())) => fence,
                (Ok(None), Ok(())) => return Err(Error::Rejected(RESULT_REJECTED)),
                (Err(error), _) | (_, Err(error)) => {
                    self.failed = true;
                    return Err(error);
                }
            };
            return self.accept_fence(fence);
        }
        let response = self.exchange(
            Ok(Request::switch_program(
                staged.image.layout.expected,
                requested,
            )),
            time_left,
        )?;
        self.accept_fence(FencedSwitch::new(requested, response))
    }

    pub fn accept_fence(
        &mut self,
        fence: FencedSwitch,
    ) -> Result<(ActiveProgram, Option<Upload>), Error<B::Error>> {
        if self.failed
            || fence.selected.revision != fence.requested.revision
            || !self
                .staged()
                .is_some_and(|image| self.uploaded && descriptor(image) == fence.requested)
        {
            self.failed = true;
            return Err(Error::UnexpectedResponse);
        }
        let staged = self.staged.take().unwrap();
        self.uploaded = false;
        Ok((fence.selected, self.active.replace(staged)))
    }

    fn exchange(
        &mut self,
        request: Result<Request, Error<B::Error>>,
        time_left: impl FnMut() -> bool,
    ) -> Result<[u32; HOST_PAYLOAD_WORDS], Error<B::Error>> {
        let result = request
            .and_then(|request| self.client.exchange(request, time_left))
            .and_then(|response| {
                if response.code == RESULT_OK {
                    Ok(response.data)
                } else {
                    Err(Error::Rejected(response.code))
                }
            });
        if result.is_err() {
            self.failed = true;
        }
        result
    }
}
