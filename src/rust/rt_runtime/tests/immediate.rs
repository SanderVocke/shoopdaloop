#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

use rt_runtime::immediate;
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn paired_responses_reserve_capacity_until_consumed_and_preserve_wrapping() {
    let (mut tx, mut rx) = immediate::paired_channel(8);
    let clock = tx.clock();
    rx.publish_frame(rt_runtime::Frame(7));
    assert_eq!(clock.frame(), rt_runtime::Frame(7));
    tx.push(10, &[1, 2]).unwrap();
    tx.push(20, &[3, 4, 5, 6]).unwrap();
    rx.begin_callback();
    rx.pop().unwrap().respond(11, |value| value + 100);
    assert_eq!(tx.push(30, &[7, 8]), Err(rt_runtime::Error::Full));
    {
        let response = tx.response().unwrap();
        assert_eq!(response.header, 11);
        assert!(response.commands().copied().eq([101, 102]));
        response.leave_unread();
    }
    assert_eq!(tx.push(30, &[7, 8]), Err(rt_runtime::Error::Full));
    drop(tx.response().unwrap());
    tx.push(30, &[7, 8]).unwrap();
    rx.pop().unwrap().respond(21, |value| value + 100);
    assert!(
        rx.pop().is_none(),
        "later publication waits for next callback"
    );
    let response = tx.response().unwrap();
    assert_eq!(response.header, 21);
    assert!(response.commands().copied().eq([103, 104, 105, 106]));
    drop(response);
    rx.begin_callback();
    rx.pop().unwrap().respond(31, |value| value + 100);
    assert!(tx.response().unwrap().commands().copied().eq([107, 108]));
    assert!(tx.response().is_none());
    rx.publish_frame(rt_runtime::Frame(8));
    assert_eq!(clock.frame(), rt_runtime::Frame(8));
}

#[cfg(not(target_arch = "wasm32"))]
#[shoop_test(no_wasm = "paused native response producer")]
fn paired_response_is_invisible_until_every_result_is_written() {
    use std::sync::{Arc, Barrier};
    let (mut tx, mut rx) = immediate::paired_channel(8);
    tx.push(1, &[1, 2, 3]).unwrap();
    let writing = Arc::new(Barrier::new(2));
    let resume = Arc::new(Barrier::new(2));
    let (worker_writing, worker_resume) = (writing.clone(), resume.clone());
    let worker = std::thread::spawn(move || {
        rx.begin_callback();
        rx.pop().unwrap().respond(2, |command| {
            if command == 2 {
                worker_writing.wait();
                worker_resume.wait();
            }
            command + 100
        });
    });
    writing.wait();
    assert!(tx.response().is_none());
    resume.wait();
    worker.join().unwrap();
    let response = tx.response().unwrap();
    assert!(response.commands().copied().eq([101, 102, 103]));
}

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
