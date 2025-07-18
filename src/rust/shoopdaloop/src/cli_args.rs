use clap::{Parser, Args, Subcommand};

/// An Audio+MIDI looper with DAW features
///
/// This program parses command-line arguments, separating arguments at the '--' delimiter.
#[derive(Parser, Debug)]
#[clap(name = "ShoopDaLoop", about = "An Audio+MIDI looper with DAW features")]
pub struct CliArgs {
    /// (optional) Load a session from a file upon startup.
    #[clap(name = "session_filename")]
    pub session_filename: Option<String>,

    /// Show information about the ShoopDaLoop installation.
    #[clap(short = 'i', long = "info")]
    pub info: bool,

    /// Show version information and exit.
    #[clap(short = 'v', long = "version")]
    pub version: bool,

    /// Choose an audio backend to use. Available backends (default = jack): jack, alsa, coreaudio
    pub backend: String,

    #[clap(flatten)]
    pub developer_options: DeveloperOptions,

    /// Arguments passed after '--'.
    /// This captures all arguments that come after a `--` separator.
    #[clap(trailing_var_arg = true, allow_hyphen_values = true)]
    pub additional_args: Vec<String>,
}

/// Developer options group.
#[derive(Args, Debug)]
#[clap(group = "Developer options")]
pub struct DeveloperOptions {
    /// Choose a specific app main window to open. Any choice other than the default is usually for debugging.
    /// Available windows: shoopdaloop_main, another_window, debug_window
    pub main: String,

    /// Start QML debugging on PORT
    #[clap(short = 'd', long = "qml-debug", value_name = "PORT")]
    pub qml_debug: Option<u16>,

    /// With QML debugging enabled, will wait until debugger connects.
    #[clap(short = 'w', long = "debug-wait")]
    pub debug_wait: bool,

    /// Enable developer functionality in the UI.
    #[clap(short = 'e', long = "developer")]
    pub developer: bool,

    /// For debugging: will open several windows, grab and save screenshots of them,
    /// store them in the given folder (will create if not existing) and then exit.
    #[clap(long = "test-grab-screens")]
    pub test_grab_screens: Option<String>,

    /// For debugging: quit X seconds after app is fully loaded.
    #[clap(long = "quit-after", default_value = "-1.0")]
    pub quit_after: f32,

    /// Start the monkey tester, which will randomly, rapidly perform actions on the session.
    #[clap(long = "monkey-tester")]
    pub monkey_tester: bool,

    /// Run QML tests and exit. Pass additional args to the tester after "--".
    #[clap(long = "qml-self-test")]
    pub qml_self_test: bool,

    /// Don't couple the poll rate of back-end engine state to screen refresh
    #[clap(long = "dont-refresh-with-gui")]
    pub dont_refresh_with_gui: bool,

    /// Ensures a minimum (back-up) refresh rate of the back-end engine state.
    #[clap(long = "max-backend-refresh-interval-ms", default_value = "25")]
    pub max_backend_refresh_interval_ms: u32,
}

// This function will be the entry point for parsing arguments.
// It takes an iterator of strings, typically `std::env::args()`.
pub fn parse_arguments<I>(args_iter: I) -> CliArgs
where
    I: IntoIterator,
    I::Item: Into<std::ffi::OsString> + Clone,
{
    CliArgs::parse_from(args_iter)
}