use super::*;

pub(super) struct Context {
    decoded: Option<Decoded>,
    remaining: VecDeque<LoopId>,
    captured: Vec<Arc<image::Image>>,
}

pub(super) fn next_transfer() -> Result<TransferId> {
    let mut id = NEXT_FILE_TASK.load(Ordering::Relaxed);
    loop {
        let next = id
            .checked_add(1)
            .ok_or_else(|| anyhow!("file transfer identity exhausted"))?;
        match NEXT_FILE_TASK.compare_exchange_weak(id, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return Ok(TransferId(id)),
            Err(current) => id = current,
        }
    }
}

impl Provider {
    pub(super) fn convert(&mut self, decoded: Decoded) -> Result<()> {
        let task = self.task.as_ref().unwrap();
        if let Some(plan) = &task.family {
            self.family = Some(Context {
                decoded: Some(decoded),
                remaining: plan.changed.iter().copied().collect(),
                captured: Vec::new(),
            });
            self.next_family_capture()
        } else {
            self.work(Work::Convert {
                decoded,
                target: task.format,
                update_length: task.update_length,
            })
        }
    }

    fn next_family_capture(&mut self) -> Result<()> {
        let task = self.task.as_mut().unwrap();
        let plan = task.family.as_ref().unwrap();
        task.key.transfer = next_transfer()?;
        if let Some(id) = self.family.as_mut().unwrap().remaining.pop_front() {
            let member = plan
                .members
                .iter()
                .find(|member| member.loop_id == id)
                .unwrap();
            task.target = transfer::Target::Loop {
                loop_id: id,
                content_revision: member.expected_revision,
            };
            task.view.message = "Capturing retained channels and synchronized loops".into();
            self.phase = Phase::Capture;
            return Ok(());
        }
        let mut context = self.family.take().unwrap();
        let mut members = plan.members.clone();
        let mut payloads = Vec::new();
        for capture in context.captured {
            let image::Manifest::Loop {
                sample_rate,
                content,
            } = &capture.manifest
            else {
                bail!("expected loop capture");
            };
            ensure!(
                *sample_rate == plan.format.sample_rate,
                "capture sample rate changed"
            );
            let member = members
                .iter_mut()
                .find(|member| member.loop_id == content.id)
                .ok_or_else(|| anyhow!("unexpected captured member"))?;
            ensure!(
                member.content.is_none() && content.frames == member.expected_frames,
                "family content changed during capture"
            );
            let mut content = content.clone();
            let offset = u32::try_from(payloads.len())?;
            for channel in content
                .audio
                .iter_mut()
                .chain(content.midi.iter_mut().map(|channel| &mut channel.channel))
            {
                if let Some(object) = &mut channel.object {
                    *object = object
                        .checked_add(offset)
                        .ok_or_else(|| anyhow!("family object count overflow"))?;
                }
            }
            payloads.extend(capture.payloads.iter().cloned());
            member.content = Some(content);
        }
        ensure!(
            plan.changed.iter().all(|id| members
                .iter()
                .any(|member| member.loop_id == *id && member.content.is_some())),
            "missing family capture"
        );
        let root = members
            .iter()
            .find(|member| member.loop_id == plan.root)
            .unwrap();
        task.target = transfer::Target::Loop {
            loop_id: plan.root,
            content_revision: root.expected_revision,
        };
        task.view.message = "Converting media and synchronized loop timings".into();
        let base = Arc::new(image::Image {
            manifest: image::Manifest::FamilyUpdate {
                format: plan.format,
                root: plan.root,
                members,
            },
            payloads,
        });
        self.work(Work::ConvertFamily {
            decoded: context.decoded.take().unwrap(),
            base,
        })
    }

    pub(super) fn family_captured(&mut self, image: Arc<image::Image>) -> Result<()> {
        let task = self.task.as_ref().unwrap();
        let transfer::Target::Loop { loop_id, .. } = task.target else {
            bail!("expected loop capture target");
        };
        ensure!(
            matches!(&image.manifest, image::Manifest::Loop { content, .. } if content.id == loop_id),
            "captured the wrong family member"
        );
        self.family.as_mut().unwrap().captured.push(image);
        self.next_family_capture()
    }
}
