use crate::cli_args::CliArgs;
use anyhow;
use backend_bindings::AudioDriverType;
use common::logging::macros::*;
use config::config::ShoopConfig;
use cxx_qt_lib::QString;
use cxx_qt_lib_shoop::connect::connect_or_report;
use cxx_qt_lib_shoop::connection_types;
use cxx_qt_lib_shoop::qobject::ffi::qobject_register_qml_singleton_instance;
use cxx_qt_lib_shoop::qobject::AsQObject;
use cxx_qt_lib_shoop::qvariant_qobject::qobject_ptr_to_qvariant;
use frontend::cxx_qt_shoop::qobj_application_bridge::{Application, ApplicationStartupSettings};
use frontend::cxx_qt_shoop::qobj_qmlengine::QmlEngine;
use frontend::cxx_qt_shoop::test::qobj_test_file_runner::TestFileRunner;
use glob::glob;
use once_cell::sync::OnceCell;
use pyo3::prelude::*;
use std::env;
use std::path::PathBuf;
use std::pin::Pin;

use crate::audio_driver_names::get_audio_driver_from_name;
use crate::global_qml_settings::GlobalQmlSettings;

shoop_log_unit!("Main");

static GLOBAL_QML_SETTINGS: OnceCell<GlobalQmlSettings> = OnceCell::new();

thread_local! {
static TEST_RUNNER: OnceCell<*mut TestFileRunner> = OnceCell::new();
}

fn app_main(cli_args: &CliArgs, config: ShoopConfig) -> Result<i32, anyhow::Error> {
    let title: String = match cli_args.self_test_options.self_test {
        true => "ShoopDaLoop Self-Test".to_string(),
        false => "ShoopDaLoop".to_string(),
    };

    let startup_settings = ApplicationStartupSettings {
        refresh_backend_on_frontend_refresh: !cli_args.developer_options.dont_refresh_with_gui,
        backend_backup_refresh_interval_ms: cli_args
            .developer_options
            .max_backend_refresh_interval_ms as u64,
        qml_debug_port: cli_args.developer_options.qml_debug,
        qml_debug_wait: if cli_args.developer_options.qml_debug.is_some() {
            Some(cli_args.developer_options.debug_wait)
        } else {
            None
        },
        title: title,
    };

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
    };
    GLOBAL_QML_SETTINGS.set(global_qml_settings).unwrap();

    let main_qml =
        if cli_args.developer_options.main.is_none() && !cli_args.self_test_options.self_test {
            Some(String::from("shoopdaloop_main"))
        } else {
            cli_args.developer_options.main.clone()
        };
    let qml: Option<PathBuf> = match main_qml {
        Some(name) => Some(PathBuf::from(&config.qml_dir).join(format!("applications/{name}.qml"))),
        None => None,
    };
    let qml = qml.as_ref();
    let qml = qml.map(|p| p.as_path());

    let mut app = Application::make_unique();
    {
        let mut app = app
            .as_mut()
            .ok_or(anyhow::anyhow!("Failed to get application handle"))?;

        if cli_args.self_test_options.self_test {
            // Let Qt manage the lifetime of our test runner by parenting it
            // to the application object. Also, register it as a singleton
            // in QML-land.
            unsafe {
                let app_qobj: *mut cxx_qt_lib_shoop::qobject::QObject =
                    app.as_mut().pin_mut_qobject_ptr();
                let testrunner_ptr = TestFileRunner::make_raw(app_qobj);
                let mut testrunner = std::pin::Pin::new_unchecked(&mut *testrunner_ptr);

                {
                    let testrunner_qobj = testrunner.as_mut().pin_mut_qobject_ptr();
                    // Connect to application slots
                    connect_or_report(
                        &*testrunner_qobj,
                        "reload_qml(QString)".to_string(),
                        &*app_qobj,
                        "reload_qml(QString)".to_string(),
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        &*testrunner_qobj,
                        "unload_qml()".to_string(),
                        &*app_qobj,
                        "unload_qml()".to_string(),
                        connection_types::QUEUED_CONNECTION,
                    );

                    // Register as a singleton so it can be found from QML
                    qobject_register_qml_singleton_instance(
                        testrunner_qobj,
                        &mut String::from("ShoopDaLoop.Rust"),
                        1,
                        0,
                        &mut String::from("ShoopTestFileRunner"),
                    )?;
                }

                TEST_RUNNER.with(|c| {
                    c.set(testrunner.get_unchecked_mut() as *mut TestFileRunner)
                        .unwrap()
                });
            }
        }

        app.as_mut().initialize(
            config.clone(),
            |mut qml_engine: Pin<&mut QmlEngine>| {
                // Set global QML arguments
                let global_args: &GlobalQmlSettings = GLOBAL_QML_SETTINGS.get().unwrap();
                let global_args = global_args.as_qvariantmap();
                let global_args =
                    cxx_qt_lib_shoop::qvariant_qvariantmap::qvariantmap_as_qvariant(&global_args)
                        .unwrap();
                unsafe {
                    qml_engine
                        .as_mut()
                        .set_root_context_property(&QString::from("global_args"), &global_args);
                }

                unsafe {
                    TEST_RUNNER.with(|c| {
                        if let Some(runner) = c.get() {
                            let mut runner_pin = std::pin::Pin::new_unchecked(&mut **runner);
                            let runner_qobj = runner_pin.as_mut().pin_mut_qobject_ptr();
                            let runner_qvariant = qobject_ptr_to_qvariant(runner_qobj);
                            qml_engine.as_mut().set_root_context_property(
                                &QString::from("shoop_test_file_runner"),
                                &runner_qvariant,
                            );
                            let qml_engine_qobj = qml_engine.as_mut().pin_mut_qobject_ptr();
                            connect_or_report(
                                &*qml_engine_qobj,
                                "destroyed(QObject*)".to_string(),
                                &*runner_qobj,
                                "on_qml_engine_destroyed()".to_string(),
                                connection_types::QUEUED_CONNECTION,
                            );
                        }
                    })
                }

                Python::with_gil(|py| -> PyResult<()> {
                    // Python-side setup
                    let qml_helpers = py.import("shoopdaloop.lib.qml_helpers")?;
                    let create_and_populate_root_context_with_engine_addr =
                        qml_helpers.getattr("create_and_populate_root_context_with_engine_addr")?;
                    unsafe {
                        let engine_addr =
                            qml_engine.get_unchecked_mut() as *mut QmlEngine as usize as u64;
                        let args = (engine_addr,);
                        create_and_populate_root_context_with_engine_addr.call1(args)?;
                    }

                    Ok(())
                })
                .map_err(|e| anyhow::anyhow!("Unable to initialize QML Python state: {e}"))
                .unwrap();
            },
            qml,
            startup_settings,
        )?;

        if cli_args.self_test_options.self_test {
            // use frontend::cxx_qt_shoop::test::qobj_test_file_runner::TestFileRunner;
            // Let Qt manage the lifetime of our test runner by parenting it
            // to the application object. Also, register it as a singleton
            // in QML-land.
            unsafe {
                let app_qobj: *mut cxx_qt_lib_shoop::qobject::QObject =
                    app.as_mut().pin_mut_qobject_ptr();

                TEST_RUNNER.with(|c| {
                    if let Some(testrunner) = c.get() {
                        let mut testrunner = std::pin::Pin::new_unchecked(&mut **testrunner);
                        let qmldir = &config.qml_dir;
                        let files_pattern = match &cli_args.self_test_options.files_pattern {
                            Some(pattern) => pattern,
                            None => &format!("{qmldir}/test/**/tst*.qml"),
                        };

                        {
                            let testrunner_qobj = testrunner.as_mut().pin_mut_qobject_ptr();
                            connect_or_report(
                                &*testrunner_qobj,
                                "done(::std::int32_t)".to_string(),
                                &*app_qobj,
                                "rust_exit(::std::int32_t)".to_string(),
                                connection_types::QUEUED_CONNECTION,
                            );
                        }

                        testrunner.as_mut().start(
                            QString::from(files_pattern),
                            QString::from(
                                cli_args
                                    .self_test_options
                                    .filter
                                    .as_ref()
                                    .unwrap_or(&".*".to_string()),
                            ),
                            app_qobj,
                            cli_args.self_test_options.list,
                            match cli_args.self_test_options.junit_xml.as_ref() {
                                Some(path) => QString::from(path),
                                None => QString::from(""),
                            },
                        );
                    }
                });
            }
        }

        unsafe { Ok(app.exec()) }
    }
}

fn entry_point<'py>(config: ShoopConfig) -> Result<i32, anyhow::Error> {
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
                .ok_or(anyhow::anyhow!("Could not determine filename"))?
                .to_str()
                .ok_or(anyhow::anyhow!("Could not determine filename"))?
                .strip_suffix(".qml")
                .ok_or(anyhow::anyhow!("Could not determine filename"))?;
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

    app_main(&cli_args, config)
}

#[cfg(not(feature = "prebuild"))]
pub fn shoopdaloop_main(config: ShoopConfig) -> i32 {
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
