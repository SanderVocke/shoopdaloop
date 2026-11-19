//! Native audio startup overrides shared by the application and standalone host.

use anyhow::{bail, ensure, Result};
use shoop_app_api::{AudioDriverConfig, AudioDriverKind, NewEnginePools};
use std::ffi::OsString;

fn parse_quantum(value: &str) -> Result<u32, String> {
    match value.parse::<u32>() {
        Ok(quantum @ (1 | 16 | 32 | 64 | 128)) => Ok(quantum),
        _ => Err("processing quantum must be 1, 16, 32, 64 or 128 frames".into()),
    }
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum NativeDriver {
    Dummy,
    Jack,
    Cpal,
}
impl NativeDriver {
    pub fn kind(self) -> AudioDriverKind {
        match self {
            Self::Dummy => AudioDriverKind::Dummy,
            Self::Jack => AudioDriverKind::Jack,
            Self::Cpal => AudioDriverKind::Cpal,
        }
    }
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum ProcessorChoice {
    Auto,
    On,
    Off,
}
impl From<ProcessorChoice> for shoop_app_api::ProcessorPolicy {
    fn from(value: ProcessorChoice) -> Self {
        match value {
            ProcessorChoice::Auto => Self::Auto,
            ProcessorChoice::On => Self::On,
            ProcessorChoice::Off => Self::Off,
        }
    }
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum CarlaChoice {
    Disabled,
    InProcess,
    Subprocess,
}
impl From<CarlaChoice> for shoop_app_api::CarlaHosting {
    fn from(value: CarlaChoice) -> Self {
        match value {
            CarlaChoice::Disabled => Self::Disabled,
            CarlaChoice::InProcess => Self::InProcess,
            CarlaChoice::Subprocess => Self::Subprocess,
        }
    }
}

#[derive(Clone, Debug, Default, clap::Args)]
#[group(id = "audio_options", multiple = true)]
pub struct AudioArgs {
    /// Built-in FX availability for the new host.
    #[arg(long, value_enum, help_heading = "Track processors")]
    pub builtin_fx: Option<ProcessorChoice>,
    /// Built-in synth availability for the new host.
    #[arg(long, value_enum, help_heading = "Track processors")]
    pub oxisynth: Option<ProcessorChoice>,
    #[arg(long, help_heading = "Track processors")]
    pub fx_instances: Option<u16>,
    #[arg(long, help_heading = "Track processors")]
    pub synth_instances: Option<u16>,
    #[arg(long, help_heading = "Track processors")]
    pub processor_memory_mb: Option<u32>,
    /// Carla Native hosting mode for this host launch.
    #[arg(long, value_enum, help_heading = "Track processors")]
    pub carla_hosting: Option<CarlaChoice>,
    #[arg(long, help_heading = "Track processors")]
    pub carla_rack_instances: Option<u16>,
    #[arg(long, help_heading = "Track processors")]
    pub carla_patchbay_instances: Option<u16>,

    /// Audio driver for this launch; does not change saved preferences.
    #[arg(long = "driver", value_enum, help_heading = "Backend")]
    pub audio_driver: Option<NativeDriver>,
    /// Processing quantum for this launch (1, 16, 32, 64 or 128 frames).
    #[arg(long, help_heading = "Backend", value_name = "FRAMES", value_parser = parse_quantum)]
    pub quantum: Option<u32>,
    /// Dummy pacing: paced, unpaced or controlled.
    #[arg(long, help_heading="Dummy Driver",value_parser=|v:&str|v.parse::<shoop_app_api::DummyMode>())]
    pub dummy_mode: Option<shoop_app_api::DummyMode>,
    /// Dummy driver buffer in audio frames for this launch.
    #[arg(long, help_heading = "Dummy Driver", value_name = "FRAMES", value_parser = clap::value_parser!(u32).range(1..=16_384))]
    pub dummy_buffer_size: Option<u32>,
    /// JACK client-name hint.
    #[arg(long, requires = "audio_driver", help_heading = "JACK Driver")]
    pub jack_client_name: Option<String>,
}
impl AudioArgs {
    pub fn apply_pools(&self, mut pools: NewEnginePools) -> NewEnginePools {
        pools.quantum = self.quantum.unwrap_or(pools.quantum);
        pools
    }

    pub fn apply_processors(
        &self,
        mut base: shoop_app_api::ProcessorSettings,
    ) -> Result<shoop_app_api::ProcessorSettings> {
        if let Some(value) = self.builtin_fx {
            base.builtin_fx = value.into();
        }
        if let Some(value) = self.oxisynth {
            base.oxisynth = value.into();
        }
        if let Some(value) = self.fx_instances {
            base.fx_instances = value;
        }
        if let Some(value) = self.synth_instances {
            base.synth_instances = value;
        }
        if let Some(value) = self.processor_memory_mb {
            base.memory_mb = value;
        }
        if let Some(value) = self.carla_hosting {
            base.carla.hosting = value.into();
        }
        if let Some(value) = self.carla_rack_instances {
            base.carla.rack_instances = value;
        }
        if let Some(value) = self.carla_patchbay_instances {
            base.carla.patchbay_instances = value;
        }
        base.validate()?;
        Ok(base)
    }

    pub fn apply(&self, base: AudioDriverConfig) -> Result<AudioDriverConfig> {
        let kind = self
            .audio_driver
            .map_or_else(|| base.kind(), |driver| driver.kind());
        ensure!(
            self.audio_driver.is_none() || kind != AudioDriverKind::Jack || cfg!(feature = "jack"),
            "JACK is unavailable in this build"
        );
        ensure!(
            self.audio_driver.is_none() || kind != AudioDriverKind::Cpal || cfg!(feature = "cpal"),
            "CPAL is unavailable in this build"
        );
        let mut config = if base.kind() == kind {
            base
        } else {
            match kind {
                AudioDriverKind::Dummy => AudioDriverConfig::Dummy(Default::default()),
                AudioDriverKind::Jack => AudioDriverConfig::Jack(Default::default()),
                AudioDriverKind::Cpal => AudioDriverConfig::Cpal(Default::default()),
                AudioDriverKind::WebAudio | AudioDriverKind::Simulated => unreachable!(),
            }
        };
        let has_jack_options = self.jack_client_name.is_some();
        if let AudioDriverConfig::Jack(jack) = &mut config {
            if let Some(name) = &self.jack_client_name {
                jack.client_name = name.clone();
            }
            jack.validate().map_err(anyhow::Error::msg)?;
        } else if has_jack_options {
            bail!("JACK options require --driver jack");
        }
        if let Some(mode) = self.dummy_mode {
            let AudioDriverConfig::Dummy(dummy) = &mut config else {
                bail!("--dummy-mode requires the dummy audio driver")
            };
            dummy.mode = mode;
        }
        if let Some(buffer_size) = self.dummy_buffer_size {
            let AudioDriverConfig::Dummy(dummy) = &mut config else {
                bail!("--dummy-buffer-size requires the dummy audio driver")
            };
            dummy.buffer_size = buffer_size;
        }
        Ok(config)
    }

    pub fn without_overrides(arguments: impl IntoIterator<Item = OsString>) -> Vec<OsString> {
        let command = <Self as clap::Args>::augment_args(clap::Command::new("audio"));
        let mut arguments = arguments.into_iter();
        let mut output = Vec::new();
        while let Some(argument) = arguments.next() {
            let option = argument
                .to_str()
                .and_then(|arg| arg.strip_prefix("--"))
                .map(|arg| {
                    arg.split_once('=')
                        .map_or((arg, false), |(name, _)| (name, true))
                });
            if let Some((_, inline)) = option.filter(|(name, _)| {
                *name == "backend"
                    || command
                        .get_arguments()
                        .any(|arg| arg.get_long() == Some(*name))
            }) {
                if !inline {
                    arguments.next();
                }
            } else {
                output.push(argument);
            }
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use shoop_wasm_test_support::shoop_test;

    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        audio: AudioArgs,
    }

    #[shoop_test]
    fn backend_override_is_not_forwarded_to_an_attaching_ui() {
        for arguments in [
            vec!["--backend", "rust-fpga", "--session", "take.shoop"],
            vec!["--backend=rust-fpga", "--session", "take.shoop"],
        ] {
            assert_eq!(
                AudioArgs::without_overrides(arguments.into_iter().map(OsString::from)),
                vec![OsString::from("--session"), OsString::from("take.shoop")]
            );
        }
    }

    #[shoop_test]
    fn quantum_override_is_validated_and_not_forwarded_to_attaching_ui() {
        let pools = NewEnginePools {
            quantum: 64,
            ..Default::default()
        };
        assert_eq!(AudioArgs::default().apply_pools(pools), pools);
        for quantum in [1, 16, 32, 64, 128] {
            let args = Cli::try_parse_from(["host", "--quantum", &quantum.to_string()]).unwrap();
            assert_eq!(
                args.audio.apply_pools(pools),
                NewEnginePools { quantum, ..pools }
            );
        }
        for quantum in ["0", "17", "256"] {
            assert!(Cli::try_parse_from(["host", "--quantum", quantum]).is_err());
        }
        for arguments in [
            vec!["--quantum", "16", "--session", "take.shoop"],
            vec!["--quantum=16", "--session", "take.shoop"],
        ] {
            assert_eq!(
                AudioArgs::without_overrides(arguments.into_iter().map(OsString::from)),
                vec![OsString::from("--session"), OsString::from("take.shoop")]
            );
        }
    }

    #[shoop_test]
    fn dummy_pacing_override_is_validated_and_not_forwarded_to_attaching_ui() {
        let args = Cli::try_parse_from(["host", "--dummy-mode", "unpaced"]).unwrap();
        let config = args.audio.apply(Default::default()).unwrap();
        assert!(matches!(
            config,
            AudioDriverConfig::Dummy(shoop_app_api::DummyAudioDriverConfig {
                mode: shoop_app_api::DummyMode::Unpaced,
                ..
            })
        ));
        assert_eq!(
            AudioArgs::without_overrides(
                ["--dummy-mode=controlled", "--session", "x"]
                    .into_iter()
                    .map(OsString::from)
            ),
            vec![OsString::from("--session"), OsString::from("x")]
        );
        assert!(Cli::try_parse_from(["host", "--dummy-mode", "invalid"]).is_err());
    }

    #[shoop_test]
    fn dummy_buffer_override_is_validated_and_not_forwarded_to_attaching_ui() {
        let args = Cli::try_parse_from(["host", "--dummy-buffer-size", "64"]).unwrap();
        let AudioDriverConfig::Dummy(dummy) = args.audio.apply(Default::default()).unwrap() else {
            panic!("dummy driver expected");
        };
        assert_eq!(dummy.buffer_size, 64);
        assert!(args
            .audio
            .apply(AudioDriverConfig::Cpal(Default::default()))
            .is_err());
        for size in ["0", "16385"] {
            assert!(Cli::try_parse_from(["host", "--dummy-buffer-size", size]).is_err());
        }
        for arguments in [
            vec!["--dummy-buffer-size", "64", "--session", "take.shoop"],
            vec!["--dummy-buffer-size=64", "--session", "take.shoop"],
        ] {
            assert_eq!(
                AudioArgs::without_overrides(arguments.into_iter().map(OsString::from)),
                vec![OsString::from("--session"), OsString::from("take.shoop")]
            );
        }
    }

    #[cfg(feature = "jack")]
    #[shoop_test]
    fn jack_cli_only_configures_the_client_name() {
        let cli = Cli::parse_from(["host", "--driver", "jack", "--jack-client-name", "Studio"]);
        let AudioDriverConfig::Jack(jack) = cli.audio.apply(Default::default()).unwrap() else {
            panic!("direct JACK expected");
        };
        assert_eq!(jack.client_name, "Studio");
        for option in [
            "--jack-audio-inputs",
            "--jack-audio-outputs",
            "--jack-midi-inputs",
            "--jack-midi-outputs",
            "--jack-connect",
        ] {
            assert!(Cli::try_parse_from(["host", "--driver", "jack", option, "obsolete"]).is_err());
        }
        assert!(
            Cli::parse_from(["host", "--driver", "jack", "--jack-client-name", "bad:name"])
                .audio
                .apply(Default::default())
                .is_err()
        );
    }

    #[shoop_test]
    fn explicit_selection_rejects_wrong_driver_options_and_missing_build_features() {
        assert!(Cli::try_parse_from(["host", "--jack-client-name", "Studio"]).is_err());
        let wrong = Cli::parse_from(["host", "--driver", "dummy", "--jack-client-name", "Studio"]);
        assert!(wrong.audio.apply(Default::default()).is_err());
        for (name, available) in [
            ("jack", cfg!(feature = "jack")),
            ("cpal", cfg!(feature = "cpal")),
            ("dummy", true),
        ] {
            let cli = Cli::parse_from(["host", "--driver", name]);
            assert_eq!(cli.audio.apply(Default::default()).is_ok(), available);
        }
        assert!(Cli::try_parse_from(["host", "--driver", "webaudio"]).is_err());
        assert!(
            Cli::try_parse_from(["host", "--driver", "jack", "--sample-rate", "48000"]).is_err()
        );
        let base = AudioDriverConfig::Jack(Default::default());
        assert_eq!(AudioArgs::default().apply(base.clone()).unwrap(), base);
    }

    #[shoop_test]
    fn processor_cli_merges_independent_policy_and_capacity_overrides() {
        use shoop_app_api::{ProcessorPolicy, ProcessorSettings};
        let cli = Cli::parse_from([
            "host",
            "--builtin-fx",
            "off",
            "--oxisynth=on",
            "--fx-instances",
            "7",
            "--synth-instances=3",
            "--processor-memory-mb",
            "384",
        ]);
        assert_eq!(
            cli.audio
                .apply_processors(ProcessorSettings::default())
                .unwrap(),
            ProcessorSettings {
                builtin_fx: ProcessorPolicy::Off,
                oxisynth: ProcessorPolicy::On,
                fx_instances: 7,
                synth_instances: 3,
                memory_mb: 384,
                carla: Default::default(),
            }
        );
        assert!(
            Cli::parse_from(["host", "--oxisynth=on", "--synth-instances=0"])
                .audio
                .apply_processors(ProcessorSettings::default())
                .is_err()
        );
        assert!(Cli::parse_from(["host", "--fx-instances=65"])
            .audio
            .apply_processors(ProcessorSettings::default())
            .is_err());
        assert_eq!(
            AudioArgs::default()
                .apply_processors(ProcessorSettings::disabled())
                .unwrap(),
            ProcessorSettings::disabled()
        );
    }

    #[shoop_test]
    fn ui_child_arguments_exclude_all_audio_startup_overrides() {
        let original = [
            "--session-transport=socket",
            "--driver=jack",
            "--jack-client-name",
            "Studio",
            "--session",
            "path with spaces.shoop",
            "--tracing",
            "--builtin-fx=off",
            "--oxisynth",
            "on",
            "--fx-instances",
            "7",
            "--synth-instances=3",
            "--processor-memory-mb",
            "384",
        ];
        assert_eq!(
            AudioArgs::without_overrides(original.map(OsString::from)),
            [
                "--session-transport=socket",
                "--session",
                "path with spaces.shoop",
                "--tracing"
            ]
            .map(OsString::from)
        );
    }
}
