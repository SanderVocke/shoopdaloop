use clap::{Parser, Subcommand};
mod audio;
mod client;
mod control;
mod polling;
mod server;

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
        Command::GenerateCredential { path } => {
            let mut token = [0; 32];
            getrandom::fill(&mut token).map_err(anyhow::Error::msg)?;
            shoop_transport_socket::private::create(&path, &token)
        }
    }
}
