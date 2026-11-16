use super::*;
impl Provider {
    fn reconcile_commit(&mut self, client: &Client, scripts: &mut shoop_scripting::ScriptManager) {
        let Phase::Committed(operation) = self.phase else {
            return;
        };
        let outcome = client.results().get(&operation).or_else(|| {
            client
                .snapshot()?
                .results
                .iter()
                .find(|result| result.id == operation)
                .map(|result| &result.outcome)
        });
        let Some(outcome) = outcome else {
            if self.prepared_scripts.is_some()
                && client.state() == State::Ready
                && client
                    .authority()
                    .is_some_and(|authority| authority.client != operation.client)
                && client.snapshot().is_some_and(|snapshot| {
                    !snapshot
                        .pending
                        .iter()
                        .any(|pending| pending.id == operation)
                })
            {
                self.phase = Phase::Unresolved;
                if let Some(task) = &mut self.task {
                    task.view.status = IoTaskStatus::Failed;
                    task.view.message = "Prior musical outcome is unknown; local scripts remain inactive. Cancel discards only the local candidate, or load/start a new project.".into();
                }
            }
            return;
        };
        match outcome {
            Outcome::Refused { message, .. } => {
                self.finish(IoTaskStatus::Failed, message.clone());
            }
            Outcome::Confirmed { revision } => {
                if self.prepared_scripts.is_some() && client.state() != State::Ready {
                    if let Some(task) = &mut self.task {
                        task.view.message = "Music committed; retaining local scripts until the matching session is confirmed".into();
                    }
                } else if client.state() == State::Lost {
                    self.finish(IoTaskStatus::Completed, "Session media committed; connection lost before the refreshed state arrived. Please reconnect to inspect the session");
                } else if client
                    .snapshot()
                    .is_some_and(|state| state.revision >= *revision)
                {
                    if self.prepared_scripts.is_some() {
                        let expected = self.task.as_ref().unwrap().key.epoch.checked_add(1);
                        if client.snapshot().map(|state| state.persistence_epoch) != expected {
                            self.phase = Phase::Unresolved;
                            if let Some(task) = &mut self.task {
                                task.view.status = IoTaskStatus::Failed;
                                task.view.message = "Music committed, but its session was replaced; local scripts remain retained and inactive. Cancel discards only the local candidate.".into();
                            }
                            return;
                        }
                        scripts
                            .install_stopped_session_scripts(self.prepared_scripts.take().unwrap());
                        self.installed_epoch =
                            client.snapshot().map(|state| state.persistence_epoch);
                    }
                    self.finish(
                        IoTaskStatus::Completed,
                        "Session media and client additions committed",
                    );
                } else if let Some(task) = &mut self.task {
                    task.view.progress = 0.95;
                    task.view.message =
                        "Media committed; awaiting the confirmed session state".into();
                }
            }
            _ => self.finish(
                IoTaskStatus::Failed,
                "The host returned an unexpected media commit result",
            ),
        }
        if client.state() == State::Lost && matches!(self.phase, Phase::Idle) {
            self.messages.clear();
            self.pending_cancel = None;
        }
    }
    fn stage(&mut self, client: &mut Client, image: Arc<image::Image>) -> Result<()> {
        let state = client
            .snapshot()
            .ok_or_else(|| anyhow!("session is not ready"))?;
        self.current(state)?;
        let task = self.task.as_mut().unwrap();
        if matches!(task.target, transfer::Target::Session { .. }) {
            task.target = transfer::Target::Session {
                revision: state.revision,
            };
        }
        self.messages.push_back(
            client
                .stage_image(task.key, task.target, image)
                .map_err(|error| anyhow!("{error:?}"))?,
        );
        task.view.message = "Staging validated media; current session remains active".into();
        task.view.progress = 0.6;
        self.phase = Phase::Staging;
        Ok(())
    }
    fn request(&mut self, client: &Client, request: transfer::Request) -> Result<()> {
        ensure!(
            self.messages.len() < MAX_MESSAGES_PER_TURN,
            "file message window is full"
        );
        self.messages
            .push_back(client.persistence(request).map_err(|e| anyhow!("{e:?}"))?);
        Ok(())
    }
    fn read_window(&mut self, client: &Client) -> Result<()> {
        let Phase::Reading(read) = &mut self.phase else {
            return Ok(());
        };
        let Some(object) = read.object else {
            return Ok(());
        };
        let key = read.receiver.description().key;
        for _ in 0..16 {
            if read.requested >= object.bytes
                || read.requested.saturating_sub(read.received) >= (16 * CHUNK_BYTES) as u64
            {
                break;
            }
            self.messages.push_back(
                client
                    .persistence(transfer::Request::Read {
                        key,
                        object: object.index,
                        offset: read.requested,
                    })
                    .map_err(|e| anyhow!("{e:?}"))?,
            );
            read.requested = object.bytes.min(read.requested + CHUNK_BYTES as u64);
        }
        Ok(())
    }
    fn finish_read_object(&mut self, client: &Client) -> Result<()> {
        let Phase::Reading(read) = &mut self.phase else {
            return Ok(());
        };
        let Some(object) = read.object else {
            return Ok(());
        };
        if read.received != object.bytes {
            return Ok(());
        }
        let key = read.receiver.description().key;
        let authority = client
            .authority()
            .ok_or_else(|| anyhow!("client lost authority"))?;
        read.receiver
            .apply(
                authority,
                transfer::Request::FinishObject {
                    key,
                    object: object.index,
                },
            )
            .map_err(|r| anyhow!("invalid captured object: {r:?}"))?;
        read.object = None;
        if object.index + 1 < read.receiver.description().objects {
            self.request(
                client,
                transfer::Request::Describe {
                    key,
                    object: object.index + 1,
                },
            )?;
        } else {
            read.receiver
                .apply(authority, transfer::Request::Finish { key })
                .map_err(|r| anyhow!("invalid capture totals: {r:?}"))?;
            let Phase::Reading(read) = std::mem::replace(&mut self.phase, Phase::Idle) else {
                unreachable!()
            };
            self.pending_cancel = Some(key);
            self.work(Work::Assemble(
                read.receiver
                    .into_objects()
                    .map_err(|r| anyhow!("incomplete capture: {r:?}"))?,
            ))?;
        }
        Ok(())
    }
    #[cfg(test)]
    pub(super) fn pump(&mut self, client: &mut Client, delivery: &mut dyn Delivery) -> Result<()> {
        self.pump_scripts(client, delivery, &mut shoop_scripting::ScriptManager::new())
    }
    pub(super) fn pump_scripts(
        &mut self,
        client: &mut Client,
        delivery: &mut dyn Delivery,
        scripts: &mut shoop_scripting::ScriptManager,
    ) -> Result<()> {
        if client.state() == State::Ready && matches!(self.phase, Phase::PreparingScripts) {
            let (image, mut preparation) = self.script_preparation.take().unwrap();
            match preparation.step(256 * 1024)? {
                Some(prepared) => {
                    self.prepared_scripts = Some(prepared);
                    self.stage(client, image)?;
                }
                None => {
                    self.script_preparation = Some((image, preparation));
                    return Ok(());
                }
            }
        }
        if let Some(value) = self.worker.as_mut().and_then(CodecJob::poll) {
            self.worker = None;
            if matches!(self.phase, Phase::Working) {
                match value.map_err(|e| anyhow!(e))? {
                    Value::Decoded(decoded) => self.decoded(decoded)?,
                    Value::Inspected {
                        decoded,
                        conversion_required,
                    } => self.inspected(
                        decoded,
                        conversion_required,
                        client
                            .snapshot()
                            .ok_or_else(|| anyhow!("session is not ready"))?,
                    )?,
                    Value::Captured(image) => {
                        self.current(
                            client
                                .snapshot()
                                .ok_or_else(|| anyhow!("session is not ready"))?,
                        )?;
                        self.captured(image)?;
                    }
                    Value::PreparedSession {
                        image,
                        scripts: additions,
                    } => {
                        self.script_preparation =
                            Some((image, scripts.begin_session_script_preparation(additions)?));
                        self.phase = Phase::PreparingScripts;
                    }
                    Value::Image(image) => {
                        if self
                            .task
                            .as_ref()
                            .is_some_and(|task| task.view.kind == IoTaskKind::LoadSession)
                        {
                            self.script_preparation = Some((
                                image,
                                scripts.begin_session_script_preparation(Default::default())?,
                            ));
                            self.phase = Phase::PreparingScripts;
                        } else {
                            self.stage(client, image)?;
                        }
                    }
                    Value::File {
                        name,
                        mime,
                        bytes,
                        warning,
                    } => {
                        let task = self.task.as_mut().unwrap();
                        self.output = Some(ApplicationFileOutput {
                            task_id: task.view.id,
                            suggested_name: name,
                            mime_type: mime,
                            bytes,
                        });
                        if !warning.is_empty() {
                            if !task.output_warning.is_empty() {
                                task.output_warning.push(' ');
                            }
                            task.output_warning.push_str(&warning);
                        }
                        task.view.message = if task.output_warning.is_empty() {
                            "Awaiting platform file output".into()
                        } else {
                            format!("{} Awaiting platform file output", task.output_warning)
                        };
                        task.view.progress = 0.9;
                        self.phase = Phase::Output;
                    }
                }
            }
        }
        self.reconcile_commit(client, scripts);
        if client.state() != State::Ready {
            if client.state() == State::Lost
                && matches!(self.phase, Phase::Committed(_))
                && self.prepared_scripts.is_some()
            {
                if let Some(task) = &mut self.task {
                    task.view.message = "Commit outcome unresolved; local scripts retained inactive and operation will not replay".into();
                }
            } else if client.state() == State::Lost
                && !matches!(self.phase, Phase::Idle | Phase::Output)
            {
                self.messages.clear();
                self.pending_cancel = None;
                self.finish(IoTaskStatus::Failed, "Connection lost. An admitted mutation remains host-owned; reconnect to inspect its confirmed outcome. This task will not replay it.");
                self.pending_cancel = None;
            }
            return Ok(());
        }
        if let Some((key, image)) = client.take_captured_image() {
            if self.task.as_ref().is_some_and(|t| t.key == key)
                && matches!(self.phase, Phase::CaptureSent)
            {
                self.current(
                    client
                        .snapshot()
                        .ok_or_else(|| anyhow!("session is not ready"))?,
                )?;
                self.captured(image)?;
            }
        }
        while let Some(reply) = client.take_persistence_reply() {
            if self.task.as_ref().is_none_or(|t| t.key != reply.key()) {
                continue;
            }
            let key = reply.key();
            match reply {
                transfer::Reply::CaptureNotice { message, .. } => {
                    self.task
                        .as_mut()
                        .unwrap()
                        .output_warning
                        .push_str(&message);
                }
                transfer::Reply::Captured(description)
                    if matches!(self.phase, Phase::CaptureSent) =>
                {
                    let receiver =
                        transfer::Receiver::new(client.authority().unwrap(), description)
                            .map_err(|r| anyhow!("invalid capture description: {r:?}"))?;
                    self.phase = Phase::Reading(Read {
                        receiver,
                        object: None,
                        received: 0,
                        requested: 0,
                    });
                    self.request(client, transfer::Request::Describe { key, object: 0 })?;
                }
                transfer::Reply::Object { object, .. }
                    if matches!(self.phase, Phase::Reading(_)) =>
                {
                    let Phase::Reading(read) = &mut self.phase else {
                        unreachable!()
                    };
                    read.receiver
                        .apply(
                            client.authority().unwrap(),
                            transfer::Request::Object { key, object },
                        )
                        .map_err(|r| anyhow!("invalid object description: {r:?}"))?;
                    read.object = Some(object);
                    read.received = 0;
                    read.requested = 0;
                    self.finish_read_object(client)?;
                }
                transfer::Reply::Chunk {
                    object,
                    offset,
                    bytes,
                    ..
                } if matches!(self.phase, Phase::Reading(_)) => {
                    let Phase::Reading(read) = &mut self.phase else {
                        unreachable!()
                    };
                    let next = offset + bytes.len() as u64;
                    read.receiver
                        .apply(
                            client.authority().unwrap(),
                            transfer::Request::Chunk {
                                key,
                                object,
                                offset,
                                bytes,
                            },
                        )
                        .map_err(|r| anyhow!("invalid captured chunk: {r:?}"))?;
                    read.received = next;
                    self.finish_read_object(client)?;
                }
                transfer::Reply::Ready { .. } if matches!(self.phase, Phase::Staging) => {
                    self.phase = Phase::Ready
                }
                transfer::Reply::Refused { reason, .. }
                    if matches!(
                        self.phase,
                        Phase::CaptureSent | Phase::Reading(_) | Phase::Staging | Phase::Ready
                    ) =>
                {
                    bail!("Persistence refused ({reason:?}). Stop active or scheduled recording, settle pending edits and retry; unsupported files and capacity limits leave the live session unchanged");
                }
                _ => {}
            }
        }
        self.read_window(client)?;
        if let Some(key) = self.pending_cancel.take() {
            self.messages.push_front(
                client
                    .persistence(transfer::Request::Cancel { key })
                    .map_err(|error| anyhow!("{error:?}"))?,
            );
        }
        if matches!(self.phase, Phase::Capture) && !client.has_unfinished_operations() {
            let task = self.task.as_mut().unwrap();
            // The capture barrier includes commands sent earlier in this client turn.
            if matches!(task.target, transfer::Target::Session { .. }) {
                task.target = transfer::Target::Session {
                    revision: client.snapshot().unwrap().revision,
                };
            }
            self.messages.push_back(
                client
                    .capture_image(task.key, task.target)
                    .map_err(|e| anyhow!("{e:?}"))?,
            );
            task.view.message = "Capturing a coherent immutable session cut".into();
            self.phase = Phase::CaptureSent;
        }
        if matches!(self.phase, Phase::Ready) && !client.has_unfinished_operations() {
            self.current(client.snapshot().unwrap())?;
            let key = self.task.as_ref().unwrap().key;
            match client.command(Command::CommitPersistence(key), None) {
                Ok(message) => {
                    let ClientMessage::Submit { operation, .. } = &message else {
                        unreachable!()
                    };
                    self.phase = Phase::Committed(*operation);
                    self.messages.push_back(message);
                    self.task.as_mut().unwrap().view.message =
                        "Awaiting the host-owned commit result".into();
                }
                Err(Error::Full) => {}
                Err(error) => bail!("could not admit persistence commit: {error:?}"),
            }
        }
        for _ in 0..MAX_MESSAGES_PER_TURN {
            let Some(message) = self.messages.front().cloned() else {
                break;
            };
            match delivery.send(message) {
                Ok(()) => { self.messages.pop_front(); }
                Err(Error::Full) => break,
                Err(error) => bail!("connection lost during file transfer ({error:?}); inspect the host session for the actual outcome"),
            }
        }
        self.reconcile_commit(client, scripts);
        Ok(())
    }
}
