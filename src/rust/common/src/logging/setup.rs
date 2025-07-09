use anyhow;
use colored::Colorize;
use fern;
use lazy_static::lazy_static;
use log;
use std::collections::HashSet;
use std::sync::Mutex;

lazy_static! {
    static ref LOG_MODULES: Mutex<HashSet<&'static str>> = Mutex::new(HashSet::new());
}

pub fn register_log_module(name: &'static str) {
    let mut modules = LOG_MODULES.lock().unwrap();
    modules.insert(name);
}

fn parse_level(level: &str) -> log::LevelFilter {
    match level {
        "trace" => log::LevelFilter::Trace,
        "debug" => log::LevelFilter::Debug,
        "info" => log::LevelFilter::Info,
        "warn" => log::LevelFilter::Warn,
        "error" => log::LevelFilter::Error,
        _ => panic!("Unknown log level: {}", level),
    }
}

fn apply_env(dispatch: fern::Dispatch) -> Result<fern::Dispatch, anyhow::Error> {
    let modules = LOG_MODULES
        .lock()
        .map_err(|e| anyhow::anyhow!("Could not lock global modules list: {e:?}"))?;
    let find_module = |module: &str| {
        modules
            .iter()
            .find(|m| **m == module)
            .and_then(|m| Some(*m))
    };

    let mut rval = dispatch;
    if let Ok(value) = std::env::var("SHOOP_LOG") {
        for item in value.split(',') {
            let mut parts = item.splitn(2, '=');
            let module_or_level = parts.next().unwrap_or_default().to_string();
            let level = parts.next().unwrap_or_default().to_string();
            match (module_or_level.is_empty(), level.is_empty()) {
                (false, false) => {
                    let maybe_log_unit = find_module(module_or_level.as_str());
                    if let Some(log_unit) = maybe_log_unit {
                        rval = rval.level_for(log_unit, parse_level(&level));
                    } else {
                        log::warn!(target: "Logging", "Skipping unknown logging module {}", module_or_level);
                    }
                }
                (false, true) => {
                    rval = rval.level(parse_level(&module_or_level));
                }
                _ => log::warn!(target: "Logging", "Skipping invalid log statement: {}", item),
            }
        }
    }
    Ok(rval)
}

pub fn init_logging() -> Result<(), anyhow::Error> {
    let level_colors = fern::colors::ColoredLevelConfig::new()
        .trace(fern::colors::Color::White)
        .debug(fern::colors::Color::Blue)
        .info(fern::colors::Color::Green)
        .warn(fern::colors::Color::Yellow)
        .error(fern::colors::Color::Red);
    let mut dispatch = fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "[{}] [{}] {}",
                record.target().magenta(),
                level_colors.color(record.level()),
                message
            ))
        })
        .level(log::LevelFilter::Info);
    dispatch = apply_env(dispatch)?;
    dispatch
        .chain(fern::Output::call(|record| println!("{}", record.args())))
        .apply()?;
    Ok(())
}
