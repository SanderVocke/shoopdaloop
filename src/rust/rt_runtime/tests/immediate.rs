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
