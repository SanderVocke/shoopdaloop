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
    #[cfg(not(target_arch = "wasm32"))]
    #[command(flatten)]
    pub audio: shoop_host::audio_args::AudioArgs,
    #[cfg(not(target_arch = "wasm32"))]
    #[arg(long)]
    pub log_file: Option<std::path::PathBuf>,
    #[cfg(not(target_arch = "wasm32"))]
    #[arg(long)]
    pub endpoint: Option<std::net::SocketAddr>,
    #[cfg(not(target_arch = "wasm32"))]
    #[arg(long)]
    pub export_profile: Option<std::path::PathBuf>,
    #[cfg(not(target_arch = "wasm32"))]
    #[arg(
        long = "host-profile",
        visible_alias = "remote-profile",
        group = "host_target",
        conflicts_with = "audio_options"
    )]
    pub remote_profile: Option<std::path::PathBuf>,
    #[cfg(not(target_arch = "wasm32"))]
    /// Connect to a host's data or management address (HOST:PORT).
    #[arg(long, group = "host_target", conflicts_with = "audio_options")]
    pub host_address: Option<String>,
    #[cfg(not(target_arch = "wasm32"))]
    #[arg(
        long,
        hide = true,
        group = "host_target",
        conflicts_with = "audio_options",
        requires = "ui_only"
    )]
    pub host_stdin: bool,
    #[cfg(not(target_arch = "wasm32"))]
    #[arg(long, requires = "host_target")]
    pub ui_only: bool,
    #[cfg(not(target_arch = "wasm32"))]
    /// Shut down the selected ownerless host using current management authority.
    #[arg(long, requires = "host_target", conflicts_with = "ui_only")]
    pub shutdown_host: bool,
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
    /// Compatibility option: new_engine is the sole application backend.
    #[arg(long, value_parser = |value: &str| value.parse::<shoop_egui::AudioBackendKind>())]
    pub audio_backend: Option<shoop_egui::AudioBackendKind>,
    /// Session delivery: in_process, socket (recoverable) or shared_memory (restart required).
    #[arg(long = "session-transport", visible_alias = "engine-transport", value_parser = |value: &str| value.parse::<shoop_egui::EngineTransportKind>())]
    pub engine_transport: Option<shoop_egui::EngineTransportKind>,
    #[cfg(not(target_arch = "wasm32"))]
    /// Validate packaged external built-in scripts and exit.
    #[arg(long)]
    pub probe_builtins: bool,
    #[cfg(not(target_arch = "wasm32"))]
    /// Exercise session attachment, recovery, media and host shutdown without a window.
    #[arg(long, requires = "engine_transport")]
    pub probe_engine_transport: bool,
    #[cfg(not(target_arch = "wasm32"))]
    /// Exercise large session files and persistence lifetime through the companion.
    #[arg(
        long,
        requires = "engine_transport",
        conflicts_with = "probe_engine_transport"
    )]
    pub probe_persistence: bool,
    #[cfg(not(target_arch = "wasm32"))]
    /// Exercise public composite authoring, recording, files and client lifetime.
    #[arg(long, requires = "engine_transport", conflicts_with_all = ["probe_engine_transport", "probe_persistence"])]
    pub probe_composites: bool,
    #[cfg(target_arch = "wasm32")]
    /// Disable browser audio (the session remains unavailable).
    #[arg(long)]
    pub offline: bool,
    #[cfg(target_arch = "wasm32")]
    /// Retired worker mode: rejected with a diagnostic.
    #[arg(long)]
    pub worker: bool,
    #[cfg(target_arch = "wasm32")]
    #[arg(long)]
    pub self_test: bool,
    #[cfg(target_arch = "wasm32")]
    #[arg(long)]
    pub web_midi_test: bool,
    #[cfg(target_arch = "wasm32")]
    #[arg(long, hide = true)]
    pub new_engine_self_test: bool,
    #[cfg(target_arch = "wasm32")]
    #[arg(long, hide = true)]
    pub persistence_large: bool,
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

impl AppArgs {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn configured_audio_driver(
        &self,
        settings: &shoop_settings::SettingsSnapshot,
    ) -> Result<shoop_egui::AudioDriverConfig, String> {
        let kind = self
            .audio
            .audio_driver
            .map(|driver| Ok(driver.kind()))
            .unwrap_or_else(|| shoop_egui::selected_audio_driver(settings))?;
        let config = shoop_egui::audio_driver_config_from_snapshot(settings, kind)?;
        self.audio.apply(config).map_err(|error| error.to_string())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn host_target(&self) -> anyhow::Result<Option<shoop_host::remote::Target>> {
        if self.host_stdin {
            let address: String = shoop_host::launch::read_message(std::io::stdin().lock())?;
            return Ok(Some(shoop_host::connection::describe(
                &address,
                std::time::Duration::from_secs(3),
            )?));
        }
        let address = match (&self.host_address, &self.remote_profile) {
            (Some(address), _) => address.clone(),
            (_, Some(path)) => {
                return Ok(Some(shoop_host::connection::resolve_target(
                    &shoop_host::remote::RemoteProfile::read(path)?,
                    std::time::Duration::from_secs(3),
                )?))
            }
            _ => return Ok(None),
        };
        Ok(Some(shoop_host::connection::describe(
            &address,
            std::time::Duration::from_secs(3),
        )?))
    }
    pub fn selected_engine_transport(
        &self,
        settings: &shoop_settings::SettingsSnapshot,
    ) -> anyhow::Result<shoop_egui::EngineTransportKind> {
        #[cfg(target_arch = "wasm32")]
        use shoop_egui::EngineTransportKind;
        let kind = self
            .engine_transport
            .map(Ok)
            .unwrap_or_else(|| shoop_egui::engine_transport_from_snapshot(settings))
            .map_err(anyhow::Error::msg)?;
        #[cfg(target_arch = "wasm32")]
        anyhow::ensure!(
            kind == EngineTransportKind::InProcess,
            "native engine IPC is unavailable in the browser"
        );
        Ok(kind)
    }
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
        let Some(argument) = command.get_arguments().find(|argument| {
            argument.get_long() == Some(name.as_str())
                || argument
                    .get_all_aliases()
                    .is_some_and(|aliases| aliases.contains(&name.as_str()))
        }) else {
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
    fn native_host_roles_do_not_require_profile_or_config_files() {
        for flag in ["--host-only", "--manage-hosts", "--host-config", "--lan"] {
            assert!(AppArgs::try_parse_from(["shoopdaloop", flag]).is_err());
        }
        for role in ["--ui-only", "--shutdown-host"] {
            assert!(AppArgs::try_parse_from(["shoopdaloop", role]).is_err());
            assert!(AppArgs::try_parse_from([
                "shoopdaloop",
                role,
                "--host-address",
                "127.0.0.1:12345"
            ])
            .is_ok());
            assert!(AppArgs::try_parse_from([
                "shoopdaloop",
                role,
                "--host-profile",
                "advanced.json"
            ])
            .is_ok());
        }
        assert!(AppArgs::try_parse_from([
            "shoopdaloop",
            "--host-address",
            "127.0.0.1:12345",
            "--host-profile",
            "advanced.json"
        ])
        .is_err());
        assert!(AppArgs::try_parse_from(["shoopdaloop", "--ui-only", "--host-stdin"]).is_ok());
        assert!(AppArgs::try_parse_from(["shoopdaloop", "--host-stdin"]).is_err());
        assert!(AppArgs::try_parse_from([
            "shoopdaloop",
            "--ui-only",
            "--host-stdin",
            "--host-address=127.0.0.1:12345"
        ])
        .is_err());
        for flag in [
            "--credential-file",
            "--generate-credential",
            "--host-registry",
        ] {
            assert!(AppArgs::try_parse_from(["shoopdaloop", flag, "removed"]).is_err());
        }
    }

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

    #[shoop_wasm_test_support::shoop_test]
    fn session_transport_cli_overrides_saved_selection() {
        use shoop_egui::EngineTransportKind as T;
        let mut builder = shoop_settings::SettingsRegistryBuilder::default();
        shoop_egui::register_audio_backend_settings(&mut builder).unwrap();
        let registry = builder.finish();
        let mut draft = shoop_settings::SettingsDraft::from_snapshot(&registry.defaults(1));
        draft.set(shoop_egui::ENGINE_TRANSPORT, "shared_memory".into());
        let document = registry
            .document_from_draft(
                &shoop_settings::SettingsDocument::empty("test"),
                &draft,
                "test",
            )
            .unwrap();
        let settings = registry.resolve(&document, 2).snapshot;
        assert_eq!(
            AppArgs::default()
                .selected_engine_transport(&settings)
                .unwrap(),
            T::SharedMemory
        );
        for kind in [T::InProcess, T::Socket, T::SharedMemory] {
            let args =
                AppArgs::try_parse_from(["shoopdaloop", "--engine-transport", kind.id()]).unwrap();
            assert_eq!(args.selected_engine_transport(&settings).unwrap(), kind);
        }
        assert!(AppArgs::try_parse_from(["shoopdaloop", "--engine-transport=ipc"]).is_err());
    }

    #[cfg(feature = "jack")]
    #[shoop_wasm_test_support::shoop_test]
    fn jack_cli_uses_the_saved_jack_profile_without_changing_preferences() {
        let mut builder = shoop_settings::SettingsRegistryBuilder::default();
        shoop_egui::register_audio_settings(&mut builder).unwrap();
        let registry = builder.finish();
        let mut draft = shoop_settings::SettingsDraft::from_snapshot(&registry.defaults(1));
        draft.set(shoop_egui::SELECTED_AUDIO_DRIVER, "dummy".into());
        let document = registry
            .document_from_draft(
                &shoop_settings::SettingsDocument::empty("test"),
                &draft,
                "test",
            )
            .unwrap();
        let settings = registry.resolve(&document, 2).snapshot;
        for transport in ["in_process", "socket", "shared_memory"] {
            let args = AppArgs::try_parse_from([
                "shoopdaloop",
                "--session-transport",
                transport,
                "--audio-driver",
                "jack",
                "--jack-client-name",
                "Live",
            ])
            .unwrap();
            let shoop_egui::AudioDriverConfig::Jack(config) =
                args.configured_audio_driver(&settings).unwrap()
            else {
                panic!("direct JACK expected");
            };
            assert_eq!(config.client_name, "Live");
        }
        assert_eq!(
            settings.get(shoop_egui::SELECTED_AUDIO_DRIVER).unwrap(),
            "dummy"
        );
        assert_eq!(
            settings.get(shoop_egui::JACK_CLIENT_NAME).unwrap(),
            "ShoopDaLoop"
        );
        for target in ["--host-address", "--host-profile", "--remote-profile"] {
            assert!(AppArgs::try_parse_from([
                "shoopdaloop",
                "--audio-driver",
                "jack",
                target,
                "existing-host",
            ])
            .is_err());
        }
        let original = [
            "--session-transport=socket",
            "--audio-driver=jack",
            "--jack-client-name",
            "Live",
        ];
        let mut child = vec![std::ffi::OsString::from("shoopdaloop")];
        child.extend(shoop_host::audio_args::AudioArgs::without_overrides(
            original.map(std::ffi::OsString::from),
        ));
        child.extend(["--ui-only", "--host-stdin"].map(std::ffi::OsString::from));
        let args = AppArgs::try_parse_from(child).unwrap();
        assert!(args.audio.audio_driver.is_none() && args.host_stdin && args.ui_only);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn cli_overrides_the_audio_backend() {
        use shoop_egui::AudioBackendKind;
        assert_eq!(
            AppArgs::try_parse_from(["shoopdaloop"])
                .unwrap()
                .audio_backend,
            None
        );
        for (value, kind) in [("new_engine", AudioBackendKind::NewEngine)] {
            let args = AppArgs::try_parse_from(["shoopdaloop", "--audio-backend", value]).unwrap();
            assert_eq!(args.audio_backend, Some(kind));
        }
        let error = AppArgs::try_parse_from(["shoopdaloop", "--audio-backend=engine"]).unwrap_err();
        assert!(error.to_string().contains("expected new_engine"));
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
    fn browser_rejects_native_ipc_selection() {
        let mut builder = shoop_settings::SettingsRegistryBuilder::default();
        shoop_egui::register_audio_backend_settings(&mut builder).unwrap();
        let settings = builder.finish().defaults(1);
        for kind in ["socket", "shared_memory"] {
            let args = parse_web_query(&format!(
                "?audio-backend=new_engine&engine-transport={kind}"
            ))
            .unwrap();
            assert!(args
                .selected_engine_transport(&settings)
                .unwrap_err()
                .to_string()
                .contains("browser"));
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn web_query_overrides_the_audio_backend() {
        use shoop_egui::AudioBackendKind;
        assert_eq!(parse_web_query("").unwrap().audio_backend, None);
        let args = parse_web_query("?audio-backend=new_engine&new-engine-self-test=1").unwrap();
        assert_eq!(args.audio_backend, Some(AudioBackendKind::NewEngine));
        assert!(args.new_engine_self_test);
        assert!(parse_web_query("?audio-backend=legacy").is_err());
        assert!(parse_web_query("?audio-backend=engine").is_err());
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
