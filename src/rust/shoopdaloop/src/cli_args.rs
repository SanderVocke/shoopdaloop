use clap::{Args, Parser};

/// An Audio+MIDI looper with DAW features
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

    /// Choose an audio backend to use.
    #[clap(short = 'b', long = "backend")]
    pub backend: Option<String>,

    /// Print available audio backends.
    #[clap(short = 'p', long = "print-backends")]
    pub print_backends: bool,

    #[clap(flatten)]
    pub developer_options: DeveloperOptions,

    #[clap(flatten)]
    pub self_test_options: SelfTestOptions,
}

/// Developer options group.
#[derive(Args, Debug)]
#[group()]
pub struct DeveloperOptions {
    /// Choose a specific app main window to open. Any choice other than the default is usually for debugging.
    #[clap(short = 'm', long = "main", help_heading = "Developer options")]
    pub main: Option<String>,

    /// Print the available main windows.
    #[clap(long = "print-main-windows", help_heading = "Developer options")]
    pub print_main_windows: bool,

    /// Start QML debugging on PORT
    #[clap(
        short = 'd',
        long = "qml-debug",
        value_name = "PORT",
        help_heading = "Developer options"
    )]
    pub qml_debug: Option<u32>,

    /// With QML debugging enabled, will wait until debugger connects.
    #[clap(
        short = 'w',
        long = "debug-wait",
        help_heading = "Developer options",
        requires = "qml_debug"
    )]
    pub debug_wait: bool,

    /// Enable developer functionality in the UI.
    #[clap(short = 'e', long = "developer", help_heading = "Developer options")]
    pub developer: bool,

    /// For debugging: will open several windows, grab and save screenshots of them,
    /// store them in the given directory (will create if not existing) and then exit.
    #[clap(
        long = "test-grab-screens",
        help_heading = "Developer options",
        value_name = "DIRECTORY"
    )]
    pub test_grab_screens: Option<String>,

    /// For debugging: quit X seconds after app is fully loaded.
    #[clap(short = 'q', long = "quit-after", help_heading = "Developer options")]
    pub quit_after: Option<i32>,

    /// Start the monkey tester, which will randomly, rapidly perform actions on the session.
    #[clap(long = "monkey-tester", help_heading = "Developer options")]
    pub monkey_tester: bool,

    /// Don't couple the poll rate of back-end engine state to screen refresh
    #[clap(
        long = "dont-refresh-with-gui",
        help_heading = "Developer options",
        default_value = "false"
    )]
    pub dont_refresh_with_gui: bool,

    /// Ensures a minimum (back-up) refresh rate of the back-end engine state.
    #[clap(
        long = "max-backend-refresh-interval-ms",
        default_value = "25",
        help_heading = "Developer options"
    )]
    pub max_backend_refresh_interval_ms: u32,

    // Disables the crash handler.
    #[clap(long = "no-crash-handling", help_heading = "Developer options")]
    pub no_crash_handling: bool,
}

/// Developer options group.
#[derive(Args, Debug)]
#[group(requires = "self_test")]
pub struct SelfTestOptions {
    /// Run QML tests and exit. See additional options under "QML Self-Test Options".
    #[clap(short = 't', long = "self-test", help_heading = "Self-test options")]
    pub self_test: bool,

    /// The (wildcard pattern of) test file(s) to run
    #[clap(
        short = 'f',
        long = "test-files-pattern",
        help_heading = "Self-test options"
    )]
    pub files_pattern: Option<String>,

    /// Don't run, but list all found test functions
    #[clap(
        short = 'l',
        long = "list",
        help_heading = "Self-test options",
        default_value = "false"
    )]
    pub list: bool,

    /// Regex filter for testcases to run
    #[clap(long = "filter", help_heading = "Self-test options")]
    pub filter: Option<String>,

    /// Output file path for JUnit XML test report
    #[clap(long = "junit-xml", help_heading = "Self-test options")]
    pub junit_xml: Option<String>,
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
