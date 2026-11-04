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
        let has_jack_options = self.jack_client_name.is_some();
        if let AudioDriverConfig::Jack(jack) = &mut config {
            if let Some(name) = &self.jack_client_name {
                jack.client_name = name.clone();
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
    fn jack_cli_only_configures_the_client_name() {
        let cli = Cli::parse_from([
            "host",
            "--audio-driver",
            "jack",
            "--jack-client-name",
            "Studio",
        ]);
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
            assert!(
                Cli::try_parse_from(["host", "--audio-driver", "jack", option, "obsolete"])
                    .is_err()
            );
        }
        assert!(Cli::parse_from([
            "host",
            "--audio-driver",
            "jack",
            "--jack-client-name",
            "bad:name"
        ])
        .audio
        .apply(Default::default())
        .is_err());
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
