use rt_fpga_reference::{fixed::Fixed, Error};
use rt_fpga_testbench::{chunks::run_chunk_case, Document, ExecutionStatus, Payload};
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn chunk_traces_roundtrip_ids_integer_limits_and_continue_after_refusal() {
    let document = Document::from_json(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../fpga/chunk_memory/examples/ordering.json"
    )))
    .unwrap();
    let Payload::ChunkCase(case) = document.payload else {
        panic!("chunk case")
    };
    let trace = run_chunk_case(case).unwrap();
    assert_eq!(trace.results.len(), 9);
    assert_eq!(trace.results[0].id, u32::MAX);
    assert_eq!(
        trace.results[2].samples,
        [Fixed(0), Fixed(i32::MIN), Fixed(i32::MAX), Fixed(3)]
    );
    assert_eq!(
        trace.results[6].samples,
        [Fixed(0), Fixed(i32::MIN), Fixed(i32::MAX), Fixed(-17)]
    );
    assert_eq!(
        trace.results[7].status,
        ExecutionStatus::Failed {
            error: Error::InvalidView
        }
    );
    assert_eq!(trace.results[8].status, ExecutionStatus::Completed);
    let document = Document::new(Payload::ChunkTrace(trace));
    assert_eq!(
        Document::from_json(&document.to_json().unwrap()).unwrap(),
        document
    );
}
