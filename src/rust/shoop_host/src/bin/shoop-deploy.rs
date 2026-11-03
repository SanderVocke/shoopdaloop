#[cfg(not(target_arch = "wasm32"))]
fn main() {
    if let Err(error) = run() {
        eprintln!("deployment: {error:#}");
        std::process::exit(1);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn run() -> anyhow::Result<()> {
    use clap::{Parser, Subcommand};
    use shoop_host::{
        deployment::{self, Bundle, Fastboot},
        remote::RemoteProfile,
    };
    use std::{net::SocketAddr, path::PathBuf, time::Duration};
    #[derive(Parser)]
    struct Args {
        #[arg(long)]
        updater: Option<SocketAddr>,
        #[arg(long)]
        profile: Option<PathBuf>,
        #[arg(long)]
        journal: Option<PathBuf>,
        #[arg(long, default_value_t = 300)]
        timeout: u64,
        #[command(subcommand)]
        command: Action,
    }
    #[derive(Subcommand)]
    enum Action {
        /// Authenticate to a running host and report durable firmware identity.
        Status,
        /// Cleanly stop the host, install and await durable health confirmation.
        Deploy { bundle: PathBuf },
        /// Inspect an already-running bootloader updater.
        UpdaterStatus,
        /// Stage a signed candidate in recovery; success means pending, not confirmed.
        Stage {
            bundle: PathBuf,
            #[arg(long)]
            reboot: bool,
        },
    }
    let args = Args::parse();
    anyhow::ensure!(
        (1..=3600).contains(&args.timeout),
        "timeout must be 1..=3600 seconds"
    );
    let timeout = Duration::from_secs(args.timeout);
    match args.command {
        Action::Status => {
            let profile = RemoteProfile::read_deployment(
                args.profile
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("--profile is required"))?,
            )?;
            match deployment::management(
                &profile,
                shoop_deployment_protocol::Request::Status,
                timeout,
            )? {
                shoop_deployment_protocol::Reply::Info(info) => println!(
                    "{} build={} slot={:?} confirmed={} trial={} nonce={} busy={} ready={}",
                    info.profile,
                    info.build,
                    info.slot,
                    info.confirmed,
                    info.trial,
                    info.boot_nonce,
                    info.busy,
                    info.ready
                ),
                reply => anyhow::bail!("deployment status: {reply:?}"),
            }
        }
        Action::Deploy { bundle } => {
            let path = args
                .profile
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("--profile is required"))?;
            let profile = RemoteProfile::read_deployment(path)?;
            let updater = if let Some(peer) = args.updater {
                peer
            } else {
                let mut peer = shoop_host::remote::resolve(
                    &profile.management,
                    std::time::Instant::now() + timeout,
                )?[0];
                peer.set_port(5554);
                peer
            };
            let journal = args.journal.unwrap_or_else(|| {
                let mut name = path.as_os_str().to_os_string();
                name.push(".deployment.json");
                PathBuf::from(name)
            });
            let info = deployment::deploy(
                &profile,
                &Bundle::read(&bundle)?,
                updater,
                &journal,
                timeout,
            )?;
            println!(
                "confirmed build={} slot={:?} nonce={}",
                info.build, info.slot, info.boot_nonce
            );
        }
        Action::UpdaterStatus => {
            let mut fastboot = Fastboot::connect(
                args.updater.ok_or_else(|| {
                    anyhow::anyhow!("--updater is required for recovery commands")
                })?,
                timeout,
            )?;
            for name in [
                "shoop-profile",
                "shoop-state",
                "shoop-confirmed",
                "shoop-pending",
                "shoop-client",
                "shoop-operation",
            ] {
                println!("{name}: {}", fastboot.getvar(name)?);
            }
        }
        Action::Stage { bundle, reboot } => {
            let bundle = Bundle::read(&bundle)?;
            let mut fastboot = Fastboot::connect(
                args.updater.ok_or_else(|| {
                    anyhow::anyhow!("--updater is required for recovery commands")
                })?,
                timeout,
            )?;
            fastboot.download(&bundle)?;
            fastboot.install()?;
            println!("candidate verified and pending (health confirmation required)");
            if reboot {
                fastboot.reboot()?;
            }
        }
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn main() {}
