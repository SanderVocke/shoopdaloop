#[cfg(not(target_arch = "wasm32"))]
fn main() {
    if let Err(error) = run() {
        eprintln!("session host: {error:#}");
        std::process::exit(1);
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn run() -> anyhow::Result<()> {
    use clap::Parser;
    use std::{io::Read, path::PathBuf};
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();
    #[derive(Parser)]
    struct Args {
        #[arg(long, required_unless_present = "generate_credential")]
        config: Option<PathBuf>,
        #[arg(long, conflicts_with = "config")]
        generate_credential: Option<PathBuf>,
        #[arg(long)]
        lan: bool,
        #[arg(long, default_value = "127.0.0.1:0")]
        management_endpoint: std::net::SocketAddr,
        #[arg(long)]
        advertise_data: Option<String>,
        #[arg(long)]
        advertise_management: Option<String>,
        #[arg(long)]
        export_profile: Option<PathBuf>,
        #[arg(long, default_value_t = 1000)]
        heartbeat_interval_ms: u64,
        #[arg(long, default_value_t = 5000)]
        heartbeat_timeout_ms: u64,
        #[arg(long, default_value_t = 30)]
        snapshot_hz: u32,
        #[arg(long)]
        registry: Option<PathBuf>,
        #[arg(long, default_value = "127.0.0.1:0")]
        endpoint: std::net::SocketAddr,
        #[arg(long)]
        credential_file: Option<PathBuf>,
        #[arg(long, default_value = "ShoopDaLoop")]
        name: String,
    }
    let args = Args::parse();
    if let Some(path) = args.generate_credential {
        shoop_host::remote::create_credential(&path)?;
        println!("Credential file created");
        return Ok(());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(args.config.as_ref().unwrap())?
        .take(65537)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(bytes.len() <= 65536, "host configuration exceeds 64 KiB");
    let config = serde_json::from_slice(&bytes)?;
    let mut options = shoop_host::serve::StartOptions::new(
        args.registry
            .map(Ok)
            .unwrap_or_else(shoop_host::registry::Registry::default_path)?,
    )?;
    options.endpoint = args.endpoint;
    options.management_endpoint = args.management_endpoint;
    options.lan = args.lan;
    options.advertised_endpoint = args.advertise_data;
    options.advertised_management = args.advertise_management;
    options.export_profile = args.export_profile;
    options.network = shoop_host::remote::NetworkOptions {
        heartbeat_interval_ms: args.heartbeat_interval_ms,
        heartbeat_timeout_ms: args.heartbeat_timeout_ms,
        snapshot_hz: args.snapshot_hz,
    };
    options.credential = args.credential_file;
    options.name = args.name;
    shoop_host::serve::run(config, options)
}
#[cfg(target_arch = "wasm32")]
fn main() {}
