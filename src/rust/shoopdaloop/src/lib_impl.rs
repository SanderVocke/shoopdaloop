use anyhow;
use common::logging::macros::*;
use config::config::ShoopConfig;
use cxx_qt_lib_shoop::qobject::QObject;
use frontend::cxx_qt_shoop::qobj_application_bridge::{Application, ApplicationStartupSettings};
use once_cell::sync::OnceCell;
use pyo3::types::{PyDict, PyList, PyNone, PyString, PyTuple};
use pyo3::{prelude::*, IntoPyObjectExt};
use std::env;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use frontend::cxx_qt_shoop::qobj_qmlengine::QmlEngine;
use cxx_qt_lib::QString;

use crate::audio_driver_names::get_audio_driver_from_name;
use crate::global_qml_settings::GlobalQmlSettings;

shoop_log_unit!("Main");

static GLOBAL_QML_SETTINGS: OnceCell<GlobalQmlSettings> = OnceCell::new();

// fn shoopdaloop_main_impl<'py>(config: ShoopConfig) -> Result<i32, anyhow::Error> {
//     // Set up PYTHONPATH.
//     env::set_var(
//         "PYTHONPATH",
//         &config.pythonpaths.join(common::util::PATH_LIST_SEPARATOR),
//     );
//     env::set_var("PYTHONHOME", &config.pythonhome);

//     let qt_plugins_path = &config.qt_plugins_dir;
//     if !qt_plugins_path.is_empty() {
//         env::set_var("QT_PLUGIN_PATH", qt_plugins_path);
//     }

//     let mut qml_dirs: Vec<String> = (&config).additional_qml_dirs.clone();
//     qml_dirs.insert(0, (&config).qml_dir.clone());
//     let qml_dirs_string = qml_dirs.join(common::util::PATH_LIST_SEPARATOR);
//     env::set_var("SHOOP_QML_PATHS", &qml_dirs_string);

//     debug!("PYTHONPATH={:?}", env::var("PYTHONPATH"));
//     debug!("PYTHONHOME={:?}", env::var("PYTHONHOME"));
//     debug!("SHOOP_QML_PATHS={:?}", env::var("SHOOP_QML_PATHS"));
//     debug!("QT_PLUGIN_PATH={:?}", env::var("QT_PLUGIN_PATH"));

//     // Get the command-line arguments
//     let args: Vec<String> = env::args().collect();

//     // Initialize the Python interpreter
//     Python::with_gil(|py| {
//         // Forward command-line arguments
//         let sys = py.import("sys")?;
//         let os = py.import("os")?;
//         let py_args: Vec<_> = args
//             .into_iter()
//             .map(|arg| PyString::new(py, &arg))
//             .collect();
//         sys.setattr("argv", PyList::new(py, &py_args)?)?;

//         // Print system path
//         let runtime_link_path_var = if cfg!(target_os = "windows") {
//             "PATH"
//         } else if cfg!(target_os = "macos") {
//             "DYLD_LIBRARY_PATH"
//         } else {
//             "LD_LIBRARY_PATH"
//         };
//         debug!(
//             "{runtime_link_path_var}: {:?}",
//             env::var(runtime_link_path_var).unwrap()
//         );

//         // Print python configuration information
//         let pythonpaths = sys.getattr("path")?;
//         debug!("[python] Python paths: {:?}", pythonpaths);
//         let version_info = sys.getattr("version_info")?;
//         debug!("[python] Python version: {:?}", version_info);

//         // Print env vars once more from within python
//         let qml_import_path = os.getattr("getenv")?.call1(("SHOOP_QML_PATHS",))?;
//         debug!("[python] QML paths: {:?}", qml_import_path);
//         let qt_plugins_path = os.getattr("getenv")?.call1(("QT_PLUGIN_PATH",))?;
//         debug!("[python] Qt plugin path: {:?}", qt_plugins_path);

//         // Explicitly add DLL search paths for extension modules
//         if cfg!(target_os = "windows") {
//             for path in &config.dynlibpaths {
//                 debug!("Add DLL directory: {}", path);
//                 os.getattr("add_dll_directory")?.call1((path.as_str(),))?;
//             }
//         }

//         // Expose Rust functionality to Python modules
//         {
//             let py_config = crate::py_config::create_py_config_module(&config, py).unwrap();
//             sys.getattr("modules")?
//                 .set_item("shoop_config", py_config)?;
//         }
//         {
//             let shoop_rust_py_module = crate::shoop_rust_py::create_py_module(py).unwrap();
//             sys.getattr("modules")?
//                 .set_item("shoop_rust", shoop_rust_py_module)?;
//         }
//         {
//             let shoop_py_backend_module = shoop_py_backend::create_py_module(py).unwrap();
//             sys.getattr("modules")?
//                 .set_item("shoop_py_backend", shoop_py_backend_module)?;
//         }

//         // Call main
//         let shoop = PyModule::import(py, "shoopdaloop").map_err(|e| {
//             e.print_and_set_sys_last_vars(py);
//             anyhow::anyhow!("Python error (details printed)")
//         })?;
//         let result = shoop.getattr("main")?.call0().map_err(|e| {
//             e.print_and_set_sys_last_vars(py);
//             anyhow::anyhow!("Python error (details printed)")
//         });

//         result?
//             .extract::<i32>()
//             .map_err(|e| anyhow::anyhow!("Python error: {:?}", e))
//     })
// }

fn shoopdaloop_main_impl<'py>(config: ShoopConfig) -> Result<i32, anyhow::Error> {
    // Set up PYTHONPATH.
    env::set_var(
        "PYTHONPATH",
        &config.pythonpaths.join(common::util::PATH_LIST_SEPARATOR),
    );
    env::set_var("PYTHONHOME", &config.pythonhome);

    let qt_plugins_path = &config.qt_plugins_dir;
    if !qt_plugins_path.is_empty() {
        env::set_var("QT_PLUGIN_PATH", qt_plugins_path);
    }

    let mut qml_dirs: Vec<String> = (&config).additional_qml_dirs.clone();
    qml_dirs.insert(0, (&config).qml_dir.clone());
    let qml_dirs_string = qml_dirs.join(common::util::PATH_LIST_SEPARATOR);
    env::set_var("SHOOP_QML_PATHS", &qml_dirs_string);

    debug!("PYTHONPATH={:?}", env::var("PYTHONPATH"));
    debug!("PYTHONHOME={:?}", env::var("PYTHONHOME"));
    debug!("SHOOP_QML_PATHS={:?}", env::var("SHOOP_QML_PATHS"));
    debug!("QT_PLUGIN_PATH={:?}", env::var("QT_PLUGIN_PATH"));

    // Get and parse the command-line arguments
    let args: Vec<String> = env::args().collect();
    let cli_args = crate::cli_args::parse_arguments(args.iter());

    if cli_args.print_backends {
        println!("Available backends:\n");
        let all_audio_driver_types = crate::audio_driver_names::all_audio_driver_types();
        for driver_type in all_audio_driver_types {
            println!("{}", crate::audio_driver_names::get_audio_driver_name(driver_type));
        }
        return Ok(0);
    }

    if cli_args.developer_options.print_main_windows {
        todo!();
    }

    if cli_args.info {
        todo!();
    }

    if cli_args.session_filename.is_some() {
        todo!();
    }

    if cli_args.version {
        todo!();
    }

    // Initialize the Python interpreter
    Python::with_gil(|py| -> PyResult<()> {
        let sys = py.import("sys")?;
        let os = py.import("os")?;

        // Explicitly add DLL search paths for extension modules
        if cfg!(target_os = "windows") {
            for path in &config.dynlibpaths {
                debug!("Add DLL directory: {}", path);
                os.getattr("add_dll_directory")?.call1((path.as_str(),))?;
            }
        }

        // Expose Rust functionality to Python modules
        {
            let py_config = crate::py_config::create_py_config_module(&config, py).unwrap();
            sys.getattr("modules")?
                .set_item("shoop_config", py_config)?;
        }
        {
            let shoop_rust_py_module = crate::shoop_rust_py::create_py_module(py).unwrap();
            sys.getattr("modules")?
                .set_item("shoop_rust", shoop_rust_py_module)?;
        }
        {
            let shoop_py_backend_module = shoop_py_backend::create_py_module(py).unwrap();
            sys.getattr("modules")?
                .set_item("shoop_py_backend", shoop_py_backend_module)?;
        }

        // Expose Python functionality to QML
        let qml_helpers = py.import("shoopdaloop.lib.qml_helpers")?;
        let register_shoopdaloop_qml_classes =
            qml_helpers.getattr("register_shoopdaloop_qml_classes")?;
        register_shoopdaloop_qml_classes.call0()?;

        Ok(())
    })
    .map_err(|e| anyhow::anyhow!("Failed to initialize Python environment: {e}"))?;

    let startup_settings = ApplicationStartupSettings {
        refresh_backend_on_frontend_refresh: !cli_args.developer_options.dont_refresh_with_gui,
        backend_backup_refresh_interval_ms: cli_args.developer_options.max_backend_refresh_interval_ms,
        nsm: false,
        qml_debug_port: cli_args.qml_debug,
        qml_debug_wait: if cli_args.qml_debug {Some(cli_args.debug_wait)} else {None},
        title: "ShoopDaLoop".to_string(),
    };

    if startup_settings.nsm {
        todo!();
    }

    let global_qml_settings = GlobalQmlSettings {
        backend_type: get_audio_driver_from_name(&cli_args.backend),
        load_session_on_startup: cli_args.session_filename.map(|s| PathBuf::from(s)),
        test_grab_screens_dir: cli_args
            .developer_options
            .test_grab_screens
            .map(|s| PathBuf::from(s)),
        developer_mode: cli_args.developer_options.developer,
        quit_after: cli_args.developer_options.quit_after,
        monkey_tester: cli_args.developer_options.monkey_tester,
    };
    GLOBAL_QML_SETTINGS.set(global_qml_settings).unwrap();

    let mut app = Application::make_unique();
    {
        let mut app = app
            .as_mut()
            .ok_or(anyhow::anyhow!("Failed to get application handle"))?;
        let qml = PathBuf::from("src/qml/applications/shoopdaloop_main.qml");
        app.as_mut().initialize(
            config,
            |mut qml_engine: Pin<&mut QmlEngine>| {
                // Set global QML arguments
                let global_args: &GlobalQmlSettings = GLOBAL_QML_SETTINGS.get().unwrap();
                let global_args = global_args.as_qvariantmap();
                let global_args = cxx_qt_lib_shoop::qvariant_qvariantmap::qvariantmap_as_qvariant(&global_args).unwrap();
                unsafe {
                    qml_engine
                        .as_mut()
                        .set_root_context_property(&QString::from("global_args"), &global_args);
                }

                Python::with_gil(|py| -> PyResult<()> {
                    // Python-side setup
                    let qml_helpers = py.import("shoopdaloop.lib.qml_helpers")?;
                    let create_and_populate_root_context_with_engine_addr =
                        qml_helpers.getattr("create_and_populate_root_context_with_engine_addr")?;
                    unsafe {
                        let engine_addr = qml_engine.get_unchecked_mut() as *mut QmlEngine as usize as u64;
                        let args = (engine_addr,);
                        create_and_populate_root_context_with_engine_addr.call1(args)?;
                    }

                    Ok(())
                })
                .map_err(|e| anyhow::anyhow!("Unable to initialize QML Python state: {e}"))
                .unwrap();
            },
            Some(&qml),
            startup_settings,
        )?;

        unsafe { Ok(app.exec()) }
    }
}

#[cfg(not(feature = "prebuild"))]
pub fn shoopdaloop_main(config: ShoopConfig) -> i32 {
    match shoopdaloop_main_impl(config) {
        Ok(r) => {
            return r;
        }
        Err(e) => {
            error!("Error: {:?}\nBacktrace:\n{:?}", e, e.backtrace());
            return 1;
        }
    }
}
