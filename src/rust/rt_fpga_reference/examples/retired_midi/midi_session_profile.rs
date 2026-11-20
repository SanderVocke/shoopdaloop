#[path = "../tests/support/midi_session.rs"]
pub mod support;

fn main() {
    if std::env::args().nth(1).as_deref() == Some("grab") {
        println!(
            "{:#?}",
            support::repeated_grab_workload(101, |operation| operation())
        );
        return;
    }
    let events = std::env::args()
        .nth(1)
        .map_or(3000, |s| s.parse().expect("event count"));
    let evidence = support::dense_session_workload(events, |operation| operation());
    println!("{evidence:#?}");
}
