use serde::Serialize;
use std::{cell::RefCell, collections::BTreeMap, time::Instant};

#[derive(Default, Serialize)]
pub struct Total {
    elapsed_ns: u64,
    calls: u64,
}

#[derive(Serialize)]
pub struct Profile {
    schema_version: u32,
    elapsed_ns: u64,
    pub phases: BTreeMap<&'static str, Total>,
}

struct Active {
    started: Instant,
    phases: BTreeMap<&'static str, Total>,
}

thread_local! {
    static ACTIVE: RefCell<Option<Active>> = const { RefCell::new(None) };
}

pub fn enable() {
    ACTIVE.with(|active| {
        *active.borrow_mut() = Some(Active {
            started: Instant::now(),
            phases: BTreeMap::new(),
        });
    });
}

pub fn finish() -> Option<Profile> {
    ACTIVE.with(|active| {
        active.borrow_mut().take().map(|active| Profile {
            schema_version: 1,
            elapsed_ns: active.started.elapsed().as_nanos() as u64,
            phases: active.phases,
        })
    })
}

pub struct Span {
    name: &'static str,
    started: Option<Instant>,
}

pub fn span(name: &'static str) -> Span {
    Span {
        name,
        started: ACTIVE.with(|active| active.borrow().is_some().then(Instant::now)),
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        if let Some(started) = self.started {
            let elapsed = started.elapsed().as_nanos() as u64;
            ACTIVE.with(|active| {
                if let Some(active) = active.borrow_mut().as_mut() {
                    let total = active.phases.entry(self.name).or_default();
                    total.elapsed_ns += elapsed;
                    total.calls += 1;
                }
            });
        }
    }
}
