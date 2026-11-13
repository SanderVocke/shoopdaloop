#[cfg(not(target_arch = "wasm32"))]
fn main() {
    if let Err(error) = run() {
        tracing::error!("session host: {error:#}");
        eprintln!("session host: {error:#}");
        std::process::exit(1);
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn parse_snapshot_hz(value: &str) -> Result<u32, String> {
    match value {
        "unlimited" => Ok(0),
        value => value
            .parse()
            .ok()
            .filter(|hz| *hz <= 60)
            .ok_or_else(|| "expected 1-60, or 0 or `unlimited`".into()),
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn run() -> anyhow::Result<()> {
    use clap::Parser;
    use std::{io::Read, path::PathBuf};
    #[derive(Parser)]
    struct Args {
        #[command(flatten)]
        audio: shoop_host::audio_args::AudioArgs,
        /// Audio graph backend: new_engine or fpga_rtl_sim.
        #[arg(long)]
        audio_backend: Option<shoop_app_api::AudioBackendKind>,
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        driver_preferences: Option<PathBuf>,
        #[arg(long, hide = true, conflicts_with_all = ["config", "audio_options"])]
        launch_stdin: bool,
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
        /// Session snapshots per second on each data connection, 1-60, or 0 or `unlimited`.
        #[arg(long, default_value = "30", value_parser = parse_snapshot_hz)]
        snapshot_hz: u32,
        #[arg(long, default_value = "127.0.0.1:0")]
        endpoint: std::net::SocketAddr,
        #[arg(long)]
        log_file: Option<PathBuf>,
        #[arg(long, default_value = "ShoopDaLoop")]
        name: String,
    }
    let args = Args::parse();
    let (startup_config, mut options) = if args.launch_stdin {
        let startup: shoop_host::launch::Startup =
            shoop_host::launch::read_message(std::io::stdin().lock())?;
        (Some(startup.config), startup.options)
    } else {
        let mut options = shoop_host::serve::StartOptions::new()?;
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
        options.log_file = args.log_file;
        options.name = args.name;
        (None, options)
    };
    if !args.launch_stdin {
        options.driver_preferences = args.driver_preferences.clone().or_else(|| {
            args.config
                .as_ref()
                .map(|path| path.with_extension("audio-driver.json"))
        });
    }
    if options.driver_preferences.is_none() {
        options.driver_preferences =
            directories::ProjectDirs::from("org", "ShoopDaLoop", "ShoopDaLoop")
                .map(|dirs| dirs.config_dir().join("native-host-audio.json"));
    }
    // Only an explicitly requested log file creates persistent diagnostics.
    if let Some(path) = &options.log_file {
        let log = shoop_host::logging::Writer::open(path)?;
        options.log_file = Some(path.canonicalize()?);
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "info".into()),
            )
            .with_writer(move || log.clone())
            .with_ansi(false)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "info".into()),
            )
            .with_writer(std::io::stderr)
            .with_ansi(false)
            .init();
    }
    let mut config = match startup_config {
        Some(config) => config,
        None if args.config.is_none() => shoop_host::config::HostConfig::default(),
        None => {
            let mut bytes = Vec::new();
            std::fs::File::open(args.config.as_ref().unwrap())?
                .take(65537)
                .read_to_end(&mut bytes)?;
            anyhow::ensure!(bytes.len() <= 65536, "host configuration exceeds 64 KiB");
            serde_json::from_slice(&bytes)?
        }
    };
    if config.backend == shoop_app_api::AudioBackendKind::NewEngine {
        if let Some(path) = &options.driver_preferences {
            if let Some(profile) = shoop_host::audio_driver::read_preferences(path)? {
                config.settings.driver = profile;
            }
        }
    }
    if let Some(backend) = args.audio_backend {
        config.settings.backend = backend;
        if backend == shoop_app_api::AudioBackendKind::FpgaRtlSim
            && args.audio.audio_driver.is_none()
        {
            config.settings.driver =
                shoop_app_api::AudioDriverConfig::Simulated(Default::default());
        }
    }
    config.settings.driver = args.audio.apply(config.settings.driver)?;
    config.settings.processors = args.audio.apply_processors(config.settings.processors)?;
    config.validate()?;
    tracing::info!(
        "Starting host {} (PID {})",
        options.name,
        std::process::id()
    );
    shoop_host::serve::run_with_ready(config, options, |target| {
        if args.launch_stdin {
            shoop_host::launch::write_message(std::io::stdout().lock(), &target)?;
        }
        Ok(())
    })?;
    tracing::info!("Host cleanup confirmed; exiting");
    Ok(())
}
#[cfg(target_arch = "wasm32")]
fn main() {}
