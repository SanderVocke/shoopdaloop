use anyhow::anyhow;
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
    if let Ok(mut modules) = LOG_MODULES.lock() {
        modules.insert(name);
    }
}

fn parse_level(level: &str) -> Option<log::LevelFilter> {
    match level.to_lowercase().as_str() {
        "trace" => Some(log::LevelFilter::Trace),
        "debug" => Some(log::LevelFilter::Debug),
        "info" => Some(log::LevelFilter::Info),
        "warn" => Some(log::LevelFilter::Warn),
        "error" => Some(log::LevelFilter::Error),
        _ => None,
    }
}

fn apply_env(dispatch: fern::Dispatch) -> Result<fern::Dispatch, anyhow::Error> {
    let modules = LOG_MODULES
        .lock()
        .map_err(|e| anyhow!("Could not lock global modules list: {e:?}"))?;
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
                        if let Some(parsed_level) = parse_level(&level) {
                            rval = rval.level_for(log_unit, parsed_level);
                        } else {
                            log::warn!(target: "Logging", "Skipping unknown logging level {} for module {}", level, module_or_level);
                        }
                    } else {
                        log::warn!(target: "Logging", "Skipping unknown logging module {}", module_or_level);
                    }
                }
                (false, true) => {
                    if let Some(parsed_level) = parse_level(&module_or_level) {
                        rval = rval.level(parsed_level);
                    } else {
                        log::warn!(target: "Logging", "Skipping unknown logging level/module {}", module_or_level);
                    }
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
