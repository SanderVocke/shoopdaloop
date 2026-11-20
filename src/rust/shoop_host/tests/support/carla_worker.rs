pub fn kill(parent: u32, chain_id: &[u8]) {
    let pid = std::fs::read_dir("/proc")
        .unwrap()
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok())
        .filter(|pid| {
            std::fs::read_to_string(format!("/proc/{pid}/status")).is_ok_and(|status| {
                status.lines().any(|line| {
                    line.strip_prefix("PPid:")
                        .is_some_and(|value| value.trim().parse() == Ok(parent))
                })
            })
        })
        .find(|pid| {
            std::fs::read(format!("/proc/{pid}/cmdline")).is_ok_and(|cmdline| {
                let mut args = cmdline.split(|byte| *byte == 0);
                args.find(|arg| *arg == b"--carla-worker-chain-id")
                    .is_some()
                    && args.next() == Some(chain_id)
            })
        })
        .expect("prepared Carla child");
    let killed = std::process::Command::new("kill")
        .args(["-KILL", &pid.to_string()])
        .status()
        .unwrap();
    assert!(
        killed.success(),
        "could not terminate the isolated test worker"
    );
}
