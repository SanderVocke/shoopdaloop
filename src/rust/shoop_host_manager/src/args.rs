use clap::Parser;
use std::path::PathBuf;

#[derive(Clone, Debug, Default, Parser)]
#[command(
    name = "shoop_host_manager",
    version,
    about = "Manage ShoopDaLoop hosts"
)]
pub struct ManagerArgs {
    /// Import an explicitly selected connection profile.
    #[arg(
        long = "host-profile",
        visible_alias = "remote-profile",
        group = "target",
        help_heading = "Advanced"
    )]
    pub remote_profile: Option<PathBuf>,
    /// Add a host by data or management address (HOST:PORT).
    #[arg(long, group = "target", help_heading = "Advanced")]
    pub host_address: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[shoop_wasm_test_support::shoop_test]
    fn standalone_manager_has_optional_advanced_inputs_and_rejects_looper_roles() {
        let args = ManagerArgs::try_parse_from(["shoop_host_manager"]).unwrap();
        assert!(args.remote_profile.is_none() && args.host_address.is_none());
        assert!(ManagerArgs::try_parse_from([
            "shoop_host_manager",
            "--host-profile",
            "advanced.json"
        ])
        .is_ok());
        assert!(ManagerArgs::try_parse_from([
            "shoop_host_manager",
            "--host-address",
            "127.0.0.1:12345"
        ])
        .is_ok());
        for flag in [
            "--host-only",
            "--manage-hosts",
            "--ui-only",
            "--tracing",
            "--host-registry",
        ] {
            assert!(ManagerArgs::try_parse_from(["shoop_host_manager", flag]).is_err());
        }
        assert!(ManagerArgs::try_parse_from([
            "shoop_host_manager",
            "--host-profile",
            "advanced.json",
            "--host-address",
            "127.0.0.1:12345"
        ])
        .is_err());
    }
}
