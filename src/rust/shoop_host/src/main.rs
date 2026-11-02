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
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        registry: Option<PathBuf>,
        #[arg(long, default_value = "127.0.0.1:0")]
        endpoint: std::net::SocketAddr,
        #[arg(long)]
        credential_file: Option<PathBuf>,
        #[arg(long)]
        host_id: Option<u128>,
        #[arg(long)]
        session_id: Option<u128>,
        #[arg(long, default_value = "ShoopDaLoop")]
        name: String,
    }
    let args = Args::parse();
    let mut bytes = Vec::new();
    std::fs::File::open(args.config)?
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
    options.credential = args.credential_file;
    options.name = args.name;
    if let Some(id) = args.host_id {
        options.host = shoop_app_protocol::HostId(id);
    }
    if let Some(id) = args.session_id {
        options.session = shoop_app_protocol::SessionId(id);
    }
    shoop_host::serve::run(config, options)
}
#[cfg(target_arch = "wasm32")]
fn main() {}
