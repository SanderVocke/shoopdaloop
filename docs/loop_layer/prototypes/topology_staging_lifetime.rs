#![forbid(unsafe_code)]
use shoop_loop_engine::{
    topology::staging::{Stager, RECORD_BYTES},
    *,
};

fn main() {
    let limits = Limits::default();
    let compiled = compile(
        limits,
        &TopologyDescription {
            loops: vec![LoopSpec {
                id: LoopId::from_parts(0, 1).unwrap(),
                kind: LoopKind::Primitive,
                length: 4,
                sync_source: None,
            }],
            triggers: vec![],
            programs: vec![],
        },
    )
    .unwrap();
    let mut stager = Stager::prepare(limits).unwrap();
    let mut header = [0; RECORD_BYTES];
    compiled.encode_header(&mut header);
    let token = stager.begin(&header).unwrap();
    for index in 0..compiled.encoded_records() {
        let mut row = [0; RECORD_BYTES];
        compiled.encode_record(index, &mut row).unwrap();
        stager.append(token, index, &row, 1).unwrap();
    }
    while !stager.validate(token, 1).unwrap().ready {}
    #[cfg(compile_fail_cancel)]
    {
        let held = stager.view(token).unwrap().nodes();
        stager.cancel(token).unwrap();
        assert_eq!(held[0].length, 4);
    }
    #[cfg(compile_fail_take)]
    let held = stager.view(token).unwrap().nodes();
    #[cfg(not(any(compile_fail_cancel, compile_fail_take)))]
    {
        let held = stager.view(token).unwrap().nodes();
        assert_eq!(held[0].length, 4);
    }
    let owner = stager.take(token).unwrap();
    #[cfg(compile_fail_take)]
    assert_eq!(held[0].length, 4);
    #[cfg(compile_fail_recycle)]
    let held = owner.nodes();
    stager.recycle(owner).unwrap();
    #[cfg(compile_fail_recycle)]
    assert_eq!(held[0].length, 4);
    assert_eq!(
        stager.status(),
        shoop_loop_engine::topology::staging::Status::Idle
    );
}
