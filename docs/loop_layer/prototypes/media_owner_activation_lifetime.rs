use rt_runtime::{resource::ResourceStore, Config, Revision};
use shoop_loop_engine::{
    media::{Description, Input, Ledger, OwnerChange},
    LoopId,
};

fn main() {
    let mut store = ResourceStore::new(
        Config {
            resource_slots: 1,
            resource_pages: 1,
            ..Config::default()
        }
        .prepare()
        .unwrap(),
    )
    .unwrap();
    let mut ledger = Ledger::prepare(1, 1, 0, 0).unwrap();
    let owner = LoopId::from_parts(0, 1).unwrap();
    let mut changes = [OwnerChange::replace(
        None,
        Input {
            description: Description {
                owner,
                revision: Revision(0),
                frames: 0,
                channels: 0,
                midi: false,
            },
            audio: None,
            midi: None,
        },
    )];
    let mut io = store.io();
    #[cfg(not(forge))]
    let permit = ledger
        .prepare_owner_activation(&mut io, &mut changes)
        .unwrap();
    #[cfg(forge)]
    let permit = shoop_loop_engine::media::OwnerActivation {
        ledger: &mut ledger,
        io: &mut io,
        changes: &mut changes,
    };
    #[cfg(hold_ledger)]
    let _usage = ledger.usage();
    #[cfg(hold_resources)]
    let _render = io.render();
    #[cfg(hold_input)]
    let _input = changes[0].input();
    #[cfg(move_owner)]
    let _owner = changes.into_iter().next().unwrap().into_input();
    permit.commit();
    assert!(changes[0].is_applied());
    let media = changes[0].published().unwrap();
    assert_eq!(ledger.playback(owner), Ok(Some(media)));
    assert_eq!(ledger.view(media).unwrap().description.owner, owner);
    assert!(changes[0].input().is_none());
    ledger.release_all(&mut io).unwrap();
    assert_eq!(ledger.usage().references, 0);
}
