#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

use rt_runtime::immediate;
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn actual_record_lengths_wrap_and_cutoff_excludes_later_groups() {
    let (mut tx, mut rx) = immediate::channel(8);
    tx.push(10, &[1, 2]).unwrap();
    tx.push(20, &[3]).unwrap();
    tx.push(30, &[4, 5]).unwrap();
    assert!(tx.push(40, &[6, 7]).is_err());
    rx.begin_callback();
    {
        let group = rx.pop().unwrap();
        assert_eq!(group.header, 10);
        assert!(group.commands().copied().eq([1, 2]));
        assert!(
            tx.push(40, &[6, 7]).is_err(),
            "readable records remain owned by RT"
        );
    }
    tx.push(40, &[6, 7]).unwrap();
    for (header, values) in [(20, &[3][..]), (30, &[4, 5][..])] {
        let group = rx.pop().unwrap();
        assert_eq!(group.header, header);
        assert!(group.commands().eq(values));
    }
    assert!(rx.pop().is_none());
    rx.begin_callback();
    let group = rx.pop().unwrap();
    assert_eq!(group.header, 40);
    assert!(group.commands().copied().eq([6, 7]));
}

#[cfg(not(target_arch = "wasm32"))]
#[shoop_test(no_wasm = "native producer thread")]
fn racing_publications_are_whole_groups_at_one_cutoff() {
    let (mut tx, mut rx) = immediate::channel(67);
    let producer = std::thread::spawn(move || {
        for sequence in 0..10_000 {
            let commands = [sequence; 32];
            let len = (sequence % 32 + 1) as usize;
            while tx.push(sequence, &commands[..len]).is_err() {
                std::thread::yield_now();
            }
        }
    });
    let mut expected = 0;
    while expected != 10_000 {
        rx.begin_callback();
        let mut records = 0;
        while let Some(group) = rx.pop() {
            let len = (expected % 32 + 1) as usize;
            assert_eq!(group.header, expected);
            assert_eq!(group.commands().count(), len);
            assert!(group.commands().all(|command| *command == expected));
            expected += 1;
            records += len + 1;
        }
        assert!(records <= 67);
    }
    producer.join().unwrap();
}
