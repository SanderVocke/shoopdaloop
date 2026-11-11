use rt_fpga_reference::{fixed::Fixed, storage::ViewId, Error};
use rt_fpga_testbench::{
    media::{run_media_case, MediaCase, MediaCommand, MediaValue},
    Document, ExecutionStatus, Payload,
};
use shoop_wasm_test_support::shoop_test;

fn case() -> MediaCase {
    let document = Document::from_json(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../fpga/media_store/examples/shared_ranges.json"
    )))
    .unwrap();
    let Payload::MediaCase(case) = document.payload else {
        panic!("media case")
    };
    case
}

#[shoop_test]
fn serial_media_traces_include_results_samples_and_ownership_after_failed_commands() {
    let mut input = case();
    input.steps.push(MediaCommand::Create);
    let trace = run_media_case(input).unwrap();
    assert_eq!(trace.results.len(), 14);
    assert_eq!(
        trace.results[5].value,
        MediaValue::Samples([8, 4, 5, 6, 7, 8, 4, 5, 6, 7, 8].map(Fixed).to_vec())
    );
    assert_eq!(
        trace.results[12].status,
        ExecutionStatus::Failed {
            error: Error::InvalidView
        }
    );
    assert_eq!(trace.results[12].value, MediaValue::None);
    assert_eq!(trace.results[13].value, MediaValue::View(ViewId(2)));
    assert_eq!(trace.results[13].media.free_chunks.len(), 3);
    let document = Document::new(Payload::MediaTrace(trace));
    assert_eq!(
        Document::from_json(&document.to_json().unwrap()).unwrap(),
        document
    );
}

#[shoop_test]
fn media_command_integer_limits_roundtrip_and_invalid_ranges_leave_ownership_unchanged() {
    let mut input = case();
    input.steps = vec![MediaCommand::Read {
        view: ViewId(u32::MAX as usize),
        position: u64::MAX,
        length: u64::MAX,
        frames: u32::MAX,
    }];
    let document = Document::new(Payload::MediaCase(input.clone()));
    assert_eq!(
        Document::from_json(&document.to_json().unwrap()).unwrap(),
        document
    );
    let refused = run_media_case(input.clone()).unwrap();
    assert_eq!(
        refused.results[0].status,
        ExecutionStatus::Failed {
            error: Error::InvalidView
        }
    );
    assert_eq!(refused.results[0].media, refused.case.media);
    input.media = run_media_case(case()).unwrap().results[1].media.clone();
    input.steps = vec![MediaCommand::Slice {
        source: ViewId(0),
        start: u64::MAX,
        frames: u64::MAX,
    }];
    let trace = run_media_case(input).unwrap();
    assert_eq!(
        trace.results[0].status,
        ExecutionStatus::Failed {
            error: Error::InvalidView
        }
    );
    assert_eq!(trace.results[0].media, trace.case.media);
}
