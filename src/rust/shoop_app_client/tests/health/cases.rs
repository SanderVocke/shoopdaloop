use super::*;
use profiling_protocol::{health, Descriptor, Record, Transfer};

fn descriptor(client: &mut Client, value: Descriptor) {
    wire(
        client,
        WireMessage::Diagnostic {
            authority: AUTHORITY,
            record: Record::Descriptor(value),
        },
    )
    .unwrap();
}
fn report(client: &mut Client, epoch: u64, sequence: u64, sample: &health::Snapshot) {
    let mut transfer = Transfer::new(
        health::stream(epoch),
        sequence,
        profiling_protocol::encode_report(sample).unwrap(),
    )
    .unwrap();
    while let Some(chunk) = transfer.next_chunk() {
        wire(
            client,
            WireMessage::Diagnostic {
                authority: AUTHORITY,
                record: Record::Data(chunk),
            },
        )
        .unwrap();
    }
}

#[shoop_test]
fn health_projection_validates_reports_tracks_age_and_clears_on_epoch_or_attachment_change() {
    let mut client = ready();
    descriptor(&mut client, health::descriptor(7));
    let mut sample = health::Snapshot::default();
    sample.temperatures[0] = Some(health::Temperature {
        name: profiling_protocol::Name::new("die").unwrap(),
        current_millicelsius: Some(42500),
        peak_millicelsius: None,
    });
    report(&mut client, 7, 1, &sample);
    assert_eq!(client.diagnostics().health, Some(sample));
    client.advance_diagnostics(std::time::Duration::from_millis(42));
    assert_eq!(client.diagnostics().sources[0].age_ms, 42);
    let mut invalid = sample;
    invalid.temperatures[0]
        .as_mut()
        .unwrap()
        .current_millicelsius = Some(i32::MAX);
    report(&mut client, 7, 2, &invalid);
    assert_eq!(client.diagnostics().health, Some(sample));
    assert_eq!(client.diagnostics().sources[0].age_ms, 42);
    assert!(client.diagnostics().sources[0].assembly_losses > 0);
    descriptor(&mut client, health::descriptor(8));
    assert!(client.diagnostics().health.is_none());
    report(&mut client, 7, 3, &sample);
    assert!(client.diagnostics().health.is_none());
    report(&mut client, 8, 1, &health::Snapshot::default());
    assert_eq!(
        client.diagnostics().health,
        Some(health::Snapshot::default())
    );
    assert_eq!(client.diagnostics().sources[0].age_ms, 0);
    client.disconnect();
    assert!(client.diagnostics().health.is_none());
}

#[shoop_test]
fn unknown_health_schemas_and_oversize_reports_are_not_decoded() {
    let mut client = ready();
    let mut unknown = health::descriptor(9);
    unknown.stream.schema += 1;
    descriptor(&mut client, unknown);
    assert!(!client.diagnostics().sources[0].supported);
    assert!(client.diagnostics().health.is_none());
    descriptor(&mut client, health::descriptor(10));
    let mut transfer =
        Transfer::new(health::stream(10), 1, vec![0; health::MAX_REPORT_BYTES + 1]).unwrap();
    wire(
        &mut client,
        WireMessage::Diagnostic {
            authority: AUTHORITY,
            record: Record::Data(transfer.next_chunk().unwrap()),
        },
    )
    .unwrap();
    assert!(client.diagnostics().health.is_none());
    assert_eq!(client.diagnostics().sources[0].assembly_losses, 1);
}
