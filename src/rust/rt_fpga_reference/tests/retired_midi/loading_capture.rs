#[shoop_test]
fn compiled_mixed_images_retain_midi_capture_across_relocation() {
    use rt_fpga_reference::midi::{self, Message};
    let mut allocator = Allocator::new(limits());
    let mut media = MediaStore::new(1).unwrap();
    let views = (0..8)
        .map(|_| media.create_recording().unwrap().0 as u64)
        .collect::<Vec<_>>();
    let mixed = |logical: LogicalStateProgram| {
        logical
            .with_midi(&[rt_fpga_compiler::MidiBinding {
                loop_id: Identity {
                    slot: 1,
                    generation: 1,
                },
                context: 0,
            }])
            .unwrap()
    };
    let first = image(&mut allocator, &mixed(graph(&[0, 1])), &views);
    let layout = first.layout.clone();
    let voice = id(&layout, 1);
    allocator.publish(layout.revision).unwrap();
    allocator.complete(layout.revision).unwrap();
    let mut pipeline = start(first);
    pipeline.trace_jobs(true);
    let mut jobs = JobProcessor::with_history_slots(16, 32, 32);
    let mut midi = midi::MidiSession::prepare(midi::SessionLimits::session()).unwrap();
    midi.bind(
        0,
        midi::Binding {
            loop_id: voice,
            input: 0,
            destination: 0,
            channels: 1,
        },
    )
    .unwrap();
    pipeline
        .write_mode_plan(
            voice,
            ModePlan {
                now: Some(Mode::Recording),
                ..Default::default()
            },
        )
        .unwrap();
    let events = [midi::InputEvent {
        input: 0,
        offset: 15,
        message: Message::managed(&[0x90, 60, 80]).unwrap(),
    }];
    let input = [Fixed(123); 16];
    let mut left = [Fixed::ZERO; 16];
    let mut right = [Fixed::ZERO; 16];
    pipeline
        .process_midi_session(
            &mut jobs,
            &mut media,
            &[&input],
            &mut [&mut left, &mut right],
            16,
            &mut midi,
            &events,
            &mut |_| Ok(()),
        )
        .unwrap();
    let mut next = image(&mut allocator, &mixed(graph(&[0, 1, 2])), &views);
    assert_eq!(
        next.layout.loop_id(Identity {
            slot: 1,
            generation: 1
        }),
        Ok(rt_fpga_compiler::Identity {
            slot: voice.slot,
            generation: voice.generation
        })
    );
    let revision = next.layout.revision;
    assert_no_heap(|| pipeline.switch_image(&mut next).unwrap());
    allocator.publish(revision).unwrap();
    allocator.complete(revision).unwrap();
    pipeline
        .process_midi_session(
            &mut jobs,
            &mut media,
            &[&input],
            &mut [&mut left, &mut right],
            16,
            &mut midi,
            &[],
            &mut |_| Ok(()),
        )
        .unwrap();
    pipeline
        .write_mode_plan(
            voice,
            ModePlan {
                now: Some(Mode::Playing),
                ..Default::default()
            },
        )
        .unwrap();
    let mut emitted = Vec::with_capacity(32);
    assert_no_heap(|| {
        assert_eq!(
            pipeline
                .process_midi_session(
                    &mut jobs,
                    &mut media,
                    &[&input],
                    &mut [&mut left, &mut right],
                    16,
                    &mut midi,
                    &[],
                    &mut |event| {
                        emitted.push(event);
                        Ok(())
                    }
                )
                .unwrap()
                .error,
            None
        );
    });
    assert_eq!(emitted.len(), 1);
    assert_eq!(emitted[0].offset, 15);
    assert_eq!(emitted[0].message, events[0].message);
    let view = midi.content(0).unwrap().unwrap();
    assert_eq!(midi.store().describe(view).unwrap().frames, 32);
    assert!(pipeline
        .emitted_jobs()
        .iter()
        .any(|j| matches!(j, rt_fpga_reference::jobs::Job::ReadView { .. })));
    assert!(pipeline.emitted_jobs().iter().any(|j| matches!(
        j,
        rt_fpga_reference::jobs::Job::Midi(midi::MidiJob::ReadUntil { .. })
    )));
}
