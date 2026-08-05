use crate::cli_args::CliArgs;
use anyhow::anyhow;
use common::logging::macros::*;
use config::config::ShoopConfig;
use cxx_qt_lib::QString;
use cxx_qt_lib_shoop::connect::connect_or_report;
use cxx_qt_lib_shoop::connection_types;
use cxx_qt_lib_shoop::qobject::AsQObject;
use cxx_qt_lib_shoop::qvariant_helpers::qobject_ptr_to_qvariant;
use frontend::cxx_qt_shoop::qobj_application_bridge::{Application, ApplicationStartupSettings};
use frontend::cxx_qt_shoop::qobj_qmlengine::{
    get_qml_engine_stack, get_registered_qml_engine, QmlEngine,
};
use frontend::cxx_qt_shoop::test::qobj_test_file_runner::TestFileRunner;
use glob::glob;
use once_cell::sync::OnceCell;
use shoop_engine::AudioDriverType;
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

fn crash_info_callback_impl() -> Result<Vec<crashhandling::AdditionalCrashAttachment>, anyhow::Error>
{
    let mut attachments = Vec::new();
    if let Some(location) = shoop_engine::realtime_lock_guard::first_violation() {
        attachments.push(crashhandling::AdditionalCrashAttachment {
            id: "realtime_lock_violation".to_string(),
            contents: format!(
                "{}:{}:{}",
                location.file(),
                location.line(),
                location.column()
            ),
        });
    }

    let maybe_qml_engine = match unsafe { get_registered_qml_engine() } {
        Ok(engine) => engine,
        Err(_) if !attachments.is_empty() => return Ok(attachments),
        Err(error) => return Err(error),
    };
    if !maybe_qml_engine.is_null() {
        unsafe {
            let maybe_qml_engine = std::pin::Pin::new_unchecked(&mut *maybe_qml_engine);
            let qml_stack = get_qml_engine_stack(maybe_qml_engine);
            attachments.push(crashhandling::AdditionalCrashAttachment {
                id: "qml_stack".to_string(),
                contents: qml_stack,
            });
        }
    }
    Ok(attachments)
}

fn crash_info_callback() -> Vec<crashhandling::AdditionalCrashAttachment> {
    match crash_info_callback_impl() {
        Ok(r) => return r,
        Err(e) => {
            error!("Could not gather additional crash info: {e}");
        }
    }
    return Vec::new();
}

fn app_main(cli_args: &CliArgs, config: ShoopConfig) -> Result<i32, anyhow::Error> {
    let _app_span = tracing::info_span!(
        "app.lifecycle",
        self_test = cli_args.self_test_options.self_test
    )
    .entered();
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
        backend_type_explicit: cli_args.backend.is_some(),
        cpal_host: cli_args.cpal_midir_options.cpal_host.clone(),
        cpal_output_device: cli_args.cpal_midir_options.cpal_output_device.clone(),
        cpal_input_device: cli_args.cpal_midir_options.cpal_input_device.clone(),
        cpal_sample_rate: cli_args.cpal_midir_options.cpal_sample_rate,
        cpal_buffer_size: cli_args.cpal_midir_options.cpal_buffer_size,
        cpal_input_channels: cli_args.cpal_midir_options.cpal_input_channels.clone(),
        cpal_output_channels: cli_args.cpal_midir_options.cpal_output_channels.clone(),
        cpal_capture_ring_frames: cli_args.cpal_midir_options.cpal_capture_ring_frames,
        midir_input: if cli_args.cpal_midir_options.midir_input.is_empty() {
            "all".to_string()
        } else {
            cli_args.cpal_midir_options.midir_input.join(",")
        },
        midir_output: if cli_args.cpal_midir_options.midir_output.is_empty() {
            "all".to_string()
        } else {
            cli_args.cpal_midir_options.midir_output.join(",")
        },
        load_session_on_startup: cli_args.session_filename.as_ref().map(|s| PathBuf::from(s)),
        test_grab_screens_dir: cli_args
            .developer_options
            .test_grab_screens
            .as_ref()
            .map(|s| PathBuf::from(s)),
        developer_mode: cli_args.developer_options.developer,
        quit_after: cli_args.developer_options.quit_after,
        panic_after: cli_args.developer_options.panic_after,
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

    frontend::egui_loop_widget::initialize();
    let mut app = Application::make_unique();
    {
        let mut app = app
            .as_mut()
            .ok_or(anyhow!("Failed to get application handle"))?;

        if cli_args.self_test_options.self_test {
            // Let Qt manage the lifetime of our test runner by parenting it
            // to the application object. Also, register it as a singleton
            // in QML-land.
            unsafe {
                let app_qobj: *mut cxx_qt::QObject = app.as_mut().pin_mut_qobject_ptr();
                let testrunner_ptr = TestFileRunner::make_raw(app_qobj);
                let mut testrunner = std::pin::Pin::new_unchecked(&mut *testrunner_ptr);

                {
                    let testrunner_qobj = testrunner.as_mut().pin_mut_qobject_ptr();
                    // Connect to application slots
                    connect_or_report(
                        &*testrunner_qobj,
                        "reload_qml(QString)",
                        &*app_qobj,
                        "reload_qml(QString)",
                        connection_types::QUEUED_CONNECTION,
                    );
                    connect_or_report(
                        &*testrunner_qobj,
                        "unload_qml()",
                        &*app_qobj,
                        "unload_qml()",
                        connection_types::QUEUED_CONNECTION,
                    );

                    // Register as a singleton so it can be found from QML
                    cxx_qt_lib_shoop::qobject::qobject_register_qml_singleton_instance(
                        testrunner_qobj,
                        "ShoopDaLoop.Rust",
                        1,
                        0,
                        "ShoopTestFileRunner",
                    )?;
                }

                TEST_RUNNER.with(|c| {
                    if c.set(testrunner.get_unchecked_mut() as *mut TestFileRunner)
                        .is_err()
                    {
                        error!("Failed to set TEST_RUNNER");
                        // If this fails, we probably shouldn't proceed with self-test related logic dependent on it,
                        // but we are in unsafe block and likely deep in initialization.
                        // Returning error from here is hard because we are in a block that doesn't propagate easily
                        // actually we are in `if cli_args.self_test_options.self_test`.
                        // We can return Err if we wrap this block or verify flow.
                        // For now, logging error is safer than panic.
                    }
                });
            }
        }

        let initialize_span = tracing::info_span!("app.qt.initialize");
        let initialize_entered = initialize_span.enter();
        app.as_mut().initialize(
            config.clone(),
            |mut qml_engine: Pin<&mut QmlEngine>| {
                // Set global QML arguments
                let global_args: &GlobalQmlSettings = match GLOBAL_QML_SETTINGS.get() {
                    Some(s) => s,
                    None => {
                        error!("GLOBAL_QML_SETTINGS not initialized");
                        return;
                    }
                };
                let global_args = global_args.as_qvariantmap();
                let global_args =
                    match cxx_qt_lib_shoop::qvariant_helpers::qvariantmap_to_qvariant(&global_args)
                    {
                        Ok(v) => v,
                        Err(e) => {
                            error!("Failed to convert global_args to QVariant: {}", e);
                            return;
                        }
                    };
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
                            let runner_qvariant = match qobject_ptr_to_qvariant(&runner_qobj) {
                                Ok(v) => v,
                                Err(e) => {
                                    error!("Failed to convert runner_qobj to QVariant: {}", e);
                                    return;
                                }
                            };
                            qml_engine.as_mut().set_root_context_property(
                                &QString::from("shoop_test_file_runner"),
                                &runner_qvariant,
                            );
                            let qml_engine_qobj = qml_engine.as_mut().pin_mut_qobject_ptr();
                            connect_or_report(
                                &*qml_engine_qobj,
                                "destroyed(QObject*)",
                                &*runner_qobj,
                                "on_qml_engine_destroyed()",
                                connection_types::QUEUED_CONNECTION,
                            );
                        }
                    })
                }
            },
            qml,
            startup_settings,
        )?;
        drop(initialize_entered);

        if cli_args.self_test_options.self_test {
            // use frontend::cxx_qt_shoop::test::qobj_test_file_runner::TestFileRunner;
            // Let Qt manage the lifetime of our test runner by parenting it
            // to the application object. Also, register it as a singleton
            // in QML-land.
            unsafe {
                let app_qobj: *mut cxx_qt::QObject = app.as_mut().pin_mut_qobject_ptr();

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
                                "done(::std::int32_t)",
                                &*app_qobj,
                                "rust_exit(::std::int32_t)",
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

        let exit_code = {
            let _event_loop_span = tracing::info_span!("app.qt.event_loop").entered();
            unsafe { app.exec() }
        };
        tracing::info_span!("app.shutdown").in_scope(|| {});
        Ok(exit_code)
    }
}

fn entry_point<'py>(config: ShoopConfig) -> Result<i32, anyhow::Error> {
    let _entry_span = tracing::info_span!("app.entry_point").entered();
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
        let _span = tracing::info_span!("app.parse_arguments").entered();
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
            // This case should theoretically be unreachable if init_crashhandling acts as a server loop,
            // or if it returns, we shouldn't proceed as a client without args.
            error!(
                "CLI args missing (unexpected in client mode) - crash handling server mode exited?"
            );
            return Ok(1);
        }
    };

    if cli_args.developer_options.tracing_engine_detail
        && !(cli_args.developer_options.tracing || cli_args.developer_options.tracing_capture)
    {
        return Err(anyhow!(
            "--tracing-engine-detail requires --tracing or --tracing-capture"
        ));
    }
    if cli_args.developer_options.tracing || cli_args.developer_options.tracing_capture {
        common::tracing_helpers::set_tracing_enabled(true);
    }
    common::tracing_helpers::set_engine_detail_enabled(
        cli_args.developer_options.tracing_engine_detail,
    );

    shoop_engine::realtime_alloc_guard::set_enabled(cli_args.developer_options.rt_alloc_guard);
    if cli_args.developer_options.rt_alloc_guard {
        info!("Realtime allocation guard enabled for top-level process calls");
    }
    shoop_engine::realtime_lock_guard::set_enabled(cli_args.developer_options.rt_lock_guard);
    if cli_args.developer_options.rt_lock_guard {
        info!("Realtime project mutex guard enabled for process callbacks");
    }

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

    if cli_args.list_cpal_hosts {
        println!("CPAL hosts:");
        for name in shoop_engine::cpal_host_names() {
            println!("- {name}");
        }
        return Ok(0);
    }

    if cli_args.list_audio_devices {
        println!("Audio output devices:");
        for (idx, name) in
            shoop_engine::cpal_output_device_names_for_host(&cli_args.cpal_midir_options.cpal_host)
                .iter()
                .enumerate()
        {
            println!("[{idx}] {name}");
        }
        println!("\nAudio input devices:");
        for (idx, name) in
            shoop_engine::cpal_input_device_names_for_host(&cli_args.cpal_midir_options.cpal_host)
                .iter()
                .enumerate()
        {
            println!("[{idx}] {name}");
        }
        return Ok(0);
    }

    if cli_args.list_midi_devices {
        println!("MIDI input ports:");
        for (idx, name) in shoop_engine::midir_input_port_names().iter().enumerate() {
            println!("[{idx}] {name}");
        }
        println!("\nMIDI output ports:");
        for (idx, name) in shoop_engine::midir_output_port_names().iter().enumerate() {
            println!("[{idx}] {name}");
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

    if cli_args.developer_options.tracing_capture {
        let tool = common::tracing_capture::resolve_capture_tool(
            cli_args.developer_options.tracing_capture_tool.as_deref(),
        )?;
        let output_dir = cli_args
            .developer_options
            .tracing_capture_output_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("traces"));
        common::tracing_capture::configure(common::tracing_capture::CaptureConfig::new(
            tool, output_dir,
        ))?;
        if !cli_args.self_test_options.self_test {
            common::tracing_capture::start_default_capture()?;
            tracing::info_span!("app.startup.configuration_ready").in_scope(|| {});
            tracing::info_span!(
                "app.startup.crash_handler_ready",
                enabled = !cli_args.developer_options.no_crash_handling
            )
            .in_scope(|| {});
        }
    }

    app_main(&cli_args, config)
}

struct TracingCaptureCleanupGuard;

impl Drop for TracingCaptureCleanupGuard {
    fn drop(&mut self) {
        if let Err(error) = common::tracing_capture::shutdown() {
            error!("Failed to shut down Tracy capture: {error}");
        }
    }
}

#[cfg(not(feature = "prebuild"))]
pub fn shoopdaloop_main(config: ShoopConfig) -> i32 {
    let _tracing_capture_cleanup = TracingCaptureCleanupGuard;
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
