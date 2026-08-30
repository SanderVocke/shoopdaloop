use clap::Parser;

#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]
pub enum SettingsTest {
    Write,
    Verify,
    Rejected,
    Invalid,
    SaveFailure,
    Unavailable,
}

#[derive(Clone, Debug, Default, Parser)]
#[command(name = "shoopdaloop", about = "ShoopDaLoop application")]
pub struct AppArgs {
    /// Open a .shoop session from a filesystem path or URL on startup.
    #[arg(long)]
    pub session: Option<String>,
    /// Fetch a startup session URL without asking for confirmation.
    #[arg(long, requires = "session")]
    pub force_url_session: bool,
    /// Capture Perfetto profiling data.
    #[arg(long)]
    pub tracing: bool,
    /// Add detailed per-node engine zones. Requires tracing.
    #[arg(long, requires = "tracing")]
    pub tracing_engine_detail: bool,
    #[arg(long, hide = true, requires = "tracing")]
    pub tracing_smoke_test: bool,
    #[cfg(not(target_arch = "wasm32"))]
    /// Validate packaged external built-in scripts and exit.
    #[arg(long)]
    pub probe_builtins: bool,
    #[cfg(all(not(target_arch = "wasm32"), feature = "native-fx"))]
    /// Validate the bundled Carla runtime and exit without opening the GUI.
    #[arg(long)]
    pub probe_carla_native: bool,
    #[cfg(all(not(target_arch = "wasm32"), feature = "native-fx"))]
    /// Open, idle, hide, and reopen every bundled Carla external UI, then exit.
    #[arg(long)]
    pub probe_carla_native_ui: bool,
    #[cfg(target_arch = "wasm32")]
    /// Use the worker audio backend without requesting browser audio access.
    #[arg(long)]
    pub offline: bool,
    #[cfg(target_arch = "wasm32")]
    /// Use the worker audio backend.
    #[arg(long)]
    pub worker: bool,
    #[cfg(target_arch = "wasm32")]
    #[arg(long)]
    pub self_test: bool,
    #[cfg(target_arch = "wasm32")]
    #[arg(long)]
    pub web_midi_test: bool,
    #[cfg(target_arch = "wasm32")]
    #[arg(long, value_enum)]
    pub settings_test: Option<SettingsTest>,
    #[cfg(target_arch = "wasm32")]
    #[arg(long)]
    pub stress: bool,
    #[cfg(target_arch = "wasm32")]
    #[arg(long)]
    pub session_only: bool,
    #[cfg(target_arch = "wasm32")]
    #[arg(long, hide = true)]
    pub settings_save_failure: bool,
    #[cfg(target_arch = "wasm32")]
    #[arg(long, hide = true)]
    pub instance: Option<usize>,
}

#[cfg(target_arch = "wasm32")]
pub fn parse_web_query(query: &str) -> Result<AppArgs, clap::Error> {
    use clap::CommandFactory;
    let command = AppArgs::command();
    let mut argv = vec!["shoopdaloop".to_owned()];
    for pair in query
        .trim_start_matches('?')
        .split('&')
        .filter(|p| !p.is_empty())
    {
        let (raw_name, raw_value) = pair.split_once('=').unwrap_or((pair, ""));
        let name = decode_query_component(raw_name).map_err(clap::Error::new)?;
        let Some(argument) = command
            .get_arguments()
            .find(|argument| argument.get_long() == Some(name.as_str()))
        else {
            continue;
        };
        let value = decode_query_component(raw_value).map_err(clap::Error::new)?;
        if argument.get_action().takes_values() {
            argv.push(format!("--{name}={value}"));
        } else {
            match value.as_str() {
                "" | "1" | "true" => argv.push(format!("--{name}")),
                "0" | "false" => {}
                _ => return Err(clap::Error::new(clap::error::ErrorKind::InvalidValue)),
            }
        }
    }
    AppArgs::try_parse_from(argv)
}

#[cfg(target_arch = "wasm32")]
fn decode_query_component(value: &str) -> Result<String, clap::error::ErrorKind> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => decoded.push(b' '),
            b'%' if index + 2 < bytes.len() => {
                decoded.push(hex_digit(bytes[index + 1])? * 16 + hex_digit(bytes[index + 2])?);
                index += 2;
            }
            b'%' => return Err(clap::error::ErrorKind::InvalidUtf8),
            byte => decoded.push(byte),
        }
        index += 1;
    }
    String::from_utf8(decoded).map_err(|_| clap::error::ErrorKind::InvalidUtf8)
}

#[cfg(target_arch = "wasm32")]
fn hex_digit(value: u8) -> Result<u8, clap::error::ErrorKind> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(clap::error::ErrorKind::InvalidUtf8),
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod native_tests {
    use super::*;

    #[shoop_wasm_test_support::shoop_test]
    fn cli_parses_session_source() {
        let args = AppArgs::try_parse_from([
            "shoopdaloop",
            "--session",
            "https://example.com/demo.shoop",
            "--force-url-session",
        ])
        .unwrap();
        assert_eq!(
            args.session.as_deref(),
            Some("https://example.com/demo.shoop")
        );
        assert!(args.force_url_session);
        assert!(AppArgs::try_parse_from(["shoopdaloop", "--force-url-session"]).is_err());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn cli_rejects_unknown_arguments() {
        assert!(AppArgs::try_parse_from(["shoopdaloop", "--unknown=1"]).is_err());
    }
}

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use super::*;

    #[shoop_wasm_test_support::shoop_test]
    fn web_query_parses_flags_and_typed_values() {
        let args = parse_web_query(
            "?offline=1&settings-test=verify&session-only=true&instance=2&session=https%3A%2F%2Fexample.com%2Fdemo.shoop&force-url-session=1",
        )
        .unwrap();
        assert!(args.offline);
        assert_eq!(args.settings_test, Some(SettingsTest::Verify));
        assert!(args.session_only);
        assert_eq!(args.instance, Some(2));
        assert_eq!(
            args.session.as_deref(),
            Some("https://example.com/demo.shoop")
        );
        assert!(args.force_url_session);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn web_query_ignores_unknown_arguments_and_validates_known_arguments() {
        let args = parse_web_query(
            "?settings%2Dtest=save%2Dfailure&worker=false&unknown=1&cache-bust=%ZZ",
        )
        .unwrap();
        assert_eq!(args.settings_test, Some(SettingsTest::SaveFailure));
        assert!(!args.worker);
        assert!(parse_web_query("?offline=maybe").is_err());
        assert!(parse_web_query("?force-url-session=1").is_err());
        assert!(parse_web_query("?settings-test=write&settings-test=verify").is_err());
    }
}
