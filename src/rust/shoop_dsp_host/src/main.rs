use clap::{Parser, Subcommand};
mod client;
mod control;
mod polling;
use shoop_dsp_host::server;

#[derive(Parser)]
#[command(about = "Timestamped network DSP worker and sample-clock requester")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Serve(server::Options),
    Client(client::Options),
    GenerateCredential { path: std::path::PathBuf },
}
fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Serve(options) => server::run(options),
        Command::Client(options) => client::run(options),
        Command::GenerateCredential { path } => shoop_dsp_link_native::create_credential(&path),
    }
}
