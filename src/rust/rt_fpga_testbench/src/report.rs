//! Per-run artifacts and provenance; configuration is local to the runner thread.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
};
#[derive(Default)]
struct Context {
    output: Option<PathBuf>,
    waves: bool,
    models: Vec<Value>,
}
thread_local! {static CONTEXT:RefCell<Context> = RefCell::new(Context::default());}
pub fn sha256(path: &Path) -> crate::Result<String> {
    Ok(Sha256::digest(std::fs::read(path)?)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}
pub fn configure(output: &Path, waves: bool) -> crate::Result<()> {
    std::fs::create_dir_all(output)?;
    let output = output.canonicalize()?;
    CONTEXT.with(|c| {
        *c.borrow_mut() = Context {
            output: Some(output),
            waves,
            ..Default::default()
        }
    });
    Ok(())
}
pub fn directory(component: &str) -> PathBuf {
    CONTEXT.with(|c| {
        let c = c.borrow();
        let base = c
            .output
            .clone()
            .unwrap_or_else(|| crate::root().join("work/marlin-suite"));
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        base.join(component)
            .join(format!("run-{}-{n}", std::process::id()))
    })
}
pub fn waveform(model: &str) -> Option<PathBuf> {
    CONTEXT
        .with(|c| c.borrow().waves)
        .then(|| directory("waves").join(format!("{model}.vcd")))
}
pub fn record_model(design: &crate::model::Design) -> crate::Result<()> {
    if !CONTEXT.with(|c| c.borrow().output.is_some()) {
        return Ok(());
    }
    let mut sources = serde_json::Map::new();
    let mut paths = design.sources.clone();
    for include in &design.includes {
        for entry in std::fs::read_dir(include)? {
            let path = entry?.path();
            if path.extension().is_some_and(|ext| ext == "svh") {
                paths.push(path);
            }
        }
    }
    for value in design.parameters.values() {
        if let Some(file) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
            let file = PathBuf::from(file);
            if file.is_file() {
                paths.push(file);
            }
        }
    }
    paths.sort();
    paths.dedup();
    for path in &paths {
        let name = path
            .strip_prefix(crate::root())
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();
        sources.insert(name, json!(sha256(path)?));
    }
    let row = json!({"top":design.top,"parameters":design.parameters,"sources_sha256":sources,"native_memory":design.memory.as_ref().map(|m| json!({"words":m.words,"embedded":m.embedded,"physical_base":m.embedded.then_some(m.base)})),"probes":design.probes.iter().map(|p| json!({"path":p.path,"injection":p.inject})).collect::<Vec<_>>()});
    CONTEXT.with(|c| {
        let mut c = c.borrow_mut();
        if !c.models.contains(&row) {
            c.models.push(row);
        }
    });
    Ok(())
}
pub fn provenance() -> Value {
    let command = |name: &str, args: &[&str]| {
        std::process::Command::new(name)
            .args(args)
            .current_dir(crate::root())
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
    };
    json!({"revision":command("git",&["rev-parse","HEAD"]),"worktree":command("git",&["status","--porcelain"]),"verilator":command("verilator",&["--version"]),"rustc":command("rustc",&["--version"]),"simulator":"Marlin 0.21.1 / Verilator","models":CONTEXT.with(|c| c.borrow().models.clone())})
}
