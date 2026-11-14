use rt_runtime::immediate;
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn cutoff_does_not_drain_later_publications_or_split_values() {
    let (mut tx, mut rx) = immediate::channel(4);
    for i in 0..4 {
        tx.push([i; 32]).unwrap();
    }
    assert!(tx.push([9; 32]).is_err());
    rx.begin_callback();
    assert_eq!(rx.pop(), Some([0; 32]));
    tx.push([4; 32]).unwrap();
    for i in 1..4 {
        assert_eq!(rx.pop(), Some([i; 32]));
    }
    assert_eq!(rx.pop(), None);
    rx.begin_callback();
    assert_eq!(rx.pop(), Some([4; 32]));
    assert_eq!(rx.pop(), None);
}

#[cfg(not(target_arch = "wasm32"))]
#[shoop_test(no_wasm = "native producer thread")]
fn racing_publications_are_whole_batches_at_one_cutoff() {
    let (mut tx, mut rx) = immediate::channel(4);
    let producer = std::thread::spawn(move || {
        for sequence in 0..10_000 {
            let mut batch = [sequence; 30];
            while let Err(value) = tx.push(batch) {
                batch = value;
                std::thread::yield_now();
            }
        }
    });
    let mut expected = 0;
    while expected != 10_000 {
        rx.begin_callback();
        let mut count = 0;
        while let Some(batch) = rx.pop() {
            assert_eq!(batch, [expected; 30]);
            expected += 1;
            count += 1;
        }
        assert!(count <= 4);
    }
    producer.join().unwrap();
}
