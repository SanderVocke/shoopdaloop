//! Native audio startup overrides shared by the application and standalone host.

use anyhow::{bail, ensure, Result};
use shoop_app_api::{AudioDriverConfig, AudioDriverKind};
use std::ffi::OsString;

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

#[derive(Clone, Debug, Default, clap::Args)]
#[group(id = "audio_options", multiple = true)]
pub struct AudioArgs {
    /// Audio driver for this launch; does not change saved preferences.
    #[arg(long, value_enum, help_heading = "Audio")]
    pub audio_driver: Option<NativeDriver>,
    /// JACK client-name hint.
    #[arg(long, requires = "audio_driver", help_heading = "JACK")]
    pub jack_client_name: Option<String>,
    /// Comma-separated local audio input names; an empty value disables inputs.
    #[arg(long, requires = "audio_driver", help_heading = "JACK")]
    pub jack_audio_inputs: Option<String>,
    /// Comma-separated local audio output names; an empty value disables outputs.
    #[arg(long, requires = "audio_driver", help_heading = "JACK")]
    pub jack_audio_outputs: Option<String>,
    /// Comma-separated local MIDI input names; an empty value disables inputs.
    #[arg(long, requires = "audio_driver", help_heading = "JACK")]
    pub jack_midi_inputs: Option<String>,
    /// Comma-separated local MIDI output names; an empty value disables outputs.
    #[arg(long, requires = "audio_driver", help_heading = "JACK")]
    pub jack_midi_outputs: Option<String>,
    /// Startup connection, SOURCE->DESTINATION. Repeat for multiple connections.
    #[arg(
        long,
        requires = "audio_driver",
        help_heading = "JACK",
        value_name = "SOURCE->DESTINATION"
    )]
    pub jack_connect: Vec<String>,
}
impl AudioArgs {
    pub fn apply(&self, base: AudioDriverConfig) -> Result<AudioDriverConfig> {
        let Some(driver) = self.audio_driver else {
            return Ok(base);
        };
        let kind = driver.kind();
        ensure!(
            kind != AudioDriverKind::Jack || cfg!(feature = "jack"),
            "JACK is unavailable in this build"
        );
        ensure!(
            kind != AudioDriverKind::Cpal || cfg!(feature = "cpal"),
            "CPAL is unavailable in this build"
        );
        let mut config = if base.kind() == kind {
            base
        } else {
            match kind {
                AudioDriverKind::Dummy => AudioDriverConfig::Dummy(Default::default()),
                AudioDriverKind::Jack => AudioDriverConfig::Jack(Default::default()),
                AudioDriverKind::Cpal => AudioDriverConfig::Cpal(Default::default()),
                AudioDriverKind::WebAudio => unreachable!(),
            }
        };
        let has_jack_options = self.jack_client_name.is_some()
            || self.jack_audio_inputs.is_some()
            || self.jack_audio_outputs.is_some()
            || self.jack_midi_inputs.is_some()
            || self.jack_midi_outputs.is_some()
            || !self.jack_connect.is_empty();
        if let AudioDriverConfig::Jack(jack) = &mut config {
            if let Some(name) = &self.jack_client_name {
                jack.client_name = name.clone();
            }
            for (value, ports) in [
                (&self.jack_audio_inputs, &mut jack.audio_inputs),
                (&self.jack_audio_outputs, &mut jack.audio_outputs),
                (&self.jack_midi_inputs, &mut jack.midi_inputs),
                (&self.jack_midi_outputs, &mut jack.midi_outputs),
            ] {
                if let Some(value) = value {
                    *ports = value
                        .split(',')
                        .map(str::trim)
                        .filter(|port| !port.is_empty())
                        .map(str::to_owned)
                        .collect();
                }
            }
            if !self.jack_connect.is_empty() {
                jack.connections = self
                    .jack_connect
                    .iter()
                    .map(|entry| {
                        let (source, target) = entry.split_once("->").ok_or_else(|| {
                            anyhow::anyhow!("JACK connections require SOURCE->DESTINATION")
                        })?;
                        ensure!(
                            !target.contains("->"),
                            "JACK connections require one SOURCE->DESTINATION pair"
                        );
                        Ok((source.trim().to_owned(), target.trim().to_owned()))
                    })
                    .collect::<Result<_>>()?;
            }
            jack.validate().map_err(anyhow::Error::msg)?;
        } else if has_jack_options {
            bail!("JACK options require --audio-driver jack");
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
                command
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

    #[cfg(feature = "jack")]
    #[shoop_test]
    fn jack_cli_overrides_preserve_unspecified_json_fields_and_validate_ports() {
        let cli = Cli::parse_from([
            "host",
            "--audio-driver",
            "jack",
            "--jack-client-name",
            "Studio",
            "--jack-audio-inputs",
            "capture_l, capture_r",
            "--jack-midi-inputs=",
            "--jack-connect",
            "system:capture_1->capture_l",
            "--jack-connect",
            "audio_out_1->system:playback_1",
        ]);
        let mut base = shoop_app_api::JackAudioDriverConfig::default();
        base.audio_outputs = vec!["main_l".into(), "main_r".into()];
        let AudioDriverConfig::Jack(jack) = cli.audio.apply(AudioDriverConfig::Jack(base)).unwrap()
        else {
            panic!("direct JACK expected");
        };
        assert_eq!(jack.client_name, "Studio");
        assert_eq!(jack.audio_inputs, ["capture_l", "capture_r"]);
        assert_eq!(jack.audio_outputs, ["main_l", "main_r"]);
        assert!(jack.midi_inputs.is_empty());
        assert_eq!(jack.connections.len(), 2);
        assert_eq!(
            jack.connections[0],
            ("system:capture_1".into(), "capture_l".into())
        );
        for arguments in [
            vec![
                "host",
                "--audio-driver",
                "jack",
                "--jack-audio-inputs",
                "duplicate,duplicate",
            ],
            vec![
                "host",
                "--audio-driver",
                "jack",
                "--jack-connect",
                "missing-arrow",
            ],
            vec![
                "host",
                "--audio-driver",
                "jack",
                "--jack-connect",
                "a->b->c",
            ],
            vec!["host", "--audio-driver", "jack", "--jack-connect", "a->"],
            vec![
                "host",
                "--audio-driver",
                "jack",
                "--jack-client-name",
                "bad:name",
            ],
        ] {
            assert!(Cli::parse_from(arguments)
                .audio
                .apply(Default::default())
                .is_err());
        }
    }

    #[shoop_test]
    fn explicit_selection_rejects_wrong_driver_options_and_missing_build_features() {
        assert!(Cli::try_parse_from(["host", "--jack-client-name", "Studio"]).is_err());
        let wrong = Cli::parse_from([
            "host",
            "--audio-driver",
            "dummy",
            "--jack-client-name",
            "Studio",
        ]);
        assert!(wrong.audio.apply(Default::default()).is_err());
        for (name, available) in [
            ("jack", cfg!(feature = "jack")),
            ("cpal", cfg!(feature = "cpal")),
            ("dummy", true),
        ] {
            let cli = Cli::parse_from(["host", "--audio-driver", name]);
            assert_eq!(cli.audio.apply(Default::default()).is_ok(), available);
        }
        assert!(Cli::try_parse_from(["host", "--audio-driver", "webaudio"]).is_err());
        assert!(
            Cli::try_parse_from(["host", "--audio-driver", "jack", "--sample-rate", "48000"])
                .is_err()
        );
        let base = AudioDriverConfig::Jack(Default::default());
        assert_eq!(AudioArgs::default().apply(base.clone()).unwrap(), base);
    }

    #[shoop_test]
    fn ui_child_arguments_exclude_all_audio_startup_overrides() {
        let original = [
            "--session-transport=socket",
            "--audio-driver=jack",
            "--jack-client-name",
            "Studio",
            "--jack-connect",
            "out->system:playback_1",
            "--jack-connect=a->b",
            "--jack-midi-inputs=",
            "--session",
            "path with spaces.shoop",
            "--tracing",
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
