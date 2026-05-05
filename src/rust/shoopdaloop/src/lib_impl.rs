use crate::cli_args::CliArgs;
use anyhow::anyhow;
use backend_bindings::AudioDriverType;
use common::logging::macros::*;
use config::config::ShoopConfig;
// use cxx_qt_lib::QString;
// use cxx_qt_lib_shoop::connect::connect_or_report;
// use cxx_qt_lib_shoop::connection_types;
// use cxx_qt_lib_shoop::qobject::ffi::qobject_register_qml_singleton_instance;
// use cxx_qt_lib_shoop::qobject::AsQObject;
// use cxx_qt_lib_shoop::qvariant_helpers::qobject_ptr_to_qvariant;
// use frontend::cxx_qt_shoop::qobj_application_bridge::{Application, ApplicationStartupSettings};
// Commented out for migration testing
use glob::glob;
use once_cell::sync::OnceCell;
use std::env;
use std::path::PathBuf;

use crate::audio_driver_names::get_audio_driver_from_name;
use crate::global_qml_settings::GlobalQmlSettings;

shoop_log_unit!("Main");

static GLOBAL_QML_SETTINGS: OnceCell<GlobalQmlSettings> = OnceCell::new();

fn crash_info_callback() -> Vec<crashhandling::AdditionalCrashAttachment> {
    Vec::new()
}

// Stub for migration testing - returns error for now
fn app_main(cli_args: &CliArgs, config: ShoopConfig) -> Result<i32, anyhow::Error> {
    info!("app_main stub for migration testing");

    // Stub: just do basic setup and return
    let backend_type = match &cli_args.backend {
        Some(backend) => get_audio_driver_from_name(backend.as_str()),
        None => match cli_args.self_test_options.self_test {
            true => AudioDriverType::Dummy,
            false => AudioDriverType::Jack,
        },
    };

    let global_qml_settings = GlobalQmlSettings {
        backend_type: backend_type,
        load_session_on_startup: cli_args.session_filename.as_ref().map(|s| PathBuf::from(s)),
        test_grab_screens_dir: cli_args
            .developer_options
            .test_grab_screens
            .as_ref()
            .map(|s| PathBuf::from(s)),
        developer_mode: cli_args.developer_options.developer,
        quit_after: cli_args.developer_options.quit_after,
        monkey_tester: cli_args.developer_options.monkey_tester,
        lua_dir: config.lua_dir.clone(),
        qml_dir: config.qml_dir.clone(),
        resource_dir: config.resource_dir.clone(),
        schemas_dir: config.schemas_dir.clone(),
        version_string: config._version.clone(),
    };
    if GLOBAL_QML_SETTINGS.set(global_qml_settings).is_err() {
        return Err(anyhow!("GLOBAL_QML_SETTINGS already initialized"));
    }

    // Stub for migration testing - application not available
    info!("Migration testing stub: would run app here");
    Ok(0)
}

fn entry_point<'py>(config: ShoopConfig) -> Result<i32, anyhow::Error> {
    let qt_plugins_path = &config.qt_plugins_dir;
    if !qt_plugins_path.is_empty() {
        env::set_var("QT_PLUGIN_PATH", qt_plugins_path);
    }

    let mut qml_dirs: Vec<String> = (&config).additional_qml_dirs.clone();
    qml_dirs.insert(0, (&config).qml_dir.clone());
    let qml_dirs_string = qml_dirs.join(common::util::PATH_LIST_SEPARATOR);
    env::set_var("SHOOP_QML_PATHS", &qml_dirs_string);

    debug!("SHOOP_QML_PATHS={:?}", env::var("SHOOP_QML_PATHS"));
    debug!("QT_PLUGIN_PATH={:?}", env::var("QT_PLUGIN_PATH"));

    // Get and parse the command-line arguments
    let args: Vec<String> = env::args().collect();
    let is_crashhandling_server = args.iter().any(|arg| arg == "--crash-handling-server");
    let cli_args = if is_crashhandling_server {
        None
    } else {
        Some(crate::cli_args::parse_arguments(args.iter()))
    };

    if !cli_args
        .as_ref()
        .map_or(false, |args| args.developer_options.no_crash_handling)
    {
        crashhandling::init_crashhandling(
            std::env::args().any(|arg| arg == "--crash-handling-server"),
            "--crash-handling-server",
            Some(crash_info_callback),
        );
        let args: Vec<String> = std::env::args().collect();
        crashhandling::set_crash_json_extra("cmdline", serde_json::json!(args.join(" ")));
        crashhandling::set_crash_json_tag("shoop_phase", "startup".into());
        crashhandling::registered_threads::register_thread("gui");
    }
    let cli_args = match cli_args {
        Some(a) => a,
        None => {
            error!(
                "CLI args missing (unexpected in client mode) - crash handling server mode exited?"
            );
            return Ok(1);
        }
    };

    if cli_args.print_backends {
        println!("Available backends:\n");
        let all_audio_driver_types = crate::audio_driver_names::all_audio_driver_types();
        for driver_type in all_audio_driver_types {
            println!(
                "{}",
                crate::audio_driver_names::get_audio_driver_name(driver_type)
            );
        }
        return Ok(0);
    }

    if cli_args.developer_options.print_main_windows {
        let qmldir = config.qml_dir;
        println!("Available main windows:\n");
        for file in glob(format!("{qmldir}/applications/*.qml").as_str())? {
            let file = file?;
            let file = file
                .file_name()
                .ok_or(anyhow!("Could not determine filename"))?
                .to_str()
                .ok_or(anyhow!("Could not determine filename"))?
                .strip_suffix(".qml")
                .ok_or(anyhow!("Could not determine filename"))?;
            println!("- {file}");
        }
        return Ok(0);
    }

    if cli_args.info {
        let version = config._version;
        let install_info = config._install_info;
        println!("ShoopDaLoop {version}");
        println!("Installation: {install_info}");
        return Ok(0);
    }

    if cli_args.version {
        let version = config._version;
        println!("{version}");
        return Ok(0);
    }

    app_main(&cli_args, config)
}

#[cfg(not(feature = "prebuild"))]
pub fn shoopdaloop_main(config: ShoopConfig) -> i32 {
    cxx_qt::init_crate!(frontend);
    cxx_qt::init_crate!(cxx_qt_lib_shoop);

    // Note: shoopdaloop crate has no #[qobject] types, so no init_crate for itself
    // cxx_qt::init_crate!(cxx_qt_lib);
    // cxx_qt::init_crate!(cxx_qt);

    match entry_point(config) {
        Ok(r) => {
            return r;
        }
        Err(e) => {
            error!("Error: {:?}\nBacktrace:\n{:?}", e, e.backtrace());
            return 1;
        }
    }
}
