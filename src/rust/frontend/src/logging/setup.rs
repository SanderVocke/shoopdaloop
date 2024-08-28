use fern;
// use std::time::SystemTime;
// use humantime;
use crate::logging::units::SHOOP_LOG_MODULES;
use colored::Colorize;

fn find_log_unit(target: &str) -> Option<&'static str> {
    for module in SHOOP_LOG_MODULES {
        if target == *module {
            return Some(module);
        }
    }
    None
}

fn parse_level(level : &str) -> log::LevelFilter {
    match level {
        "trace" => log::LevelFilter::Trace,
        "debug" => log::LevelFilter::Debug,
        "info" => log::LevelFilter::Info,
        "warn" => log::LevelFilter::Warn,
        "error" => log::LevelFilter::Error,
        _ => panic!("Unknown log level: {}", level),
    }
}

fn apply_env(dispatch: fern::Dispatch) -> fern::Dispatch {
    let mut rval = dispatch;
    if let Ok(value) = std::env::var("SHOOP_LOG") {
        for item in value.split(',') {
            let mut parts = item.splitn(2, '=');
            let module_or_level = parts.next().unwrap_or_default().to_string();
            let level = parts.next().unwrap_or_default().to_string();
            match (module_or_level.is_empty(), level.is_empty()) {
                (false, false) => {
                    let maybe_log_unit = find_log_unit(module_or_level.as_str());
                    if let Some(log_unit) = maybe_log_unit {
                        rval = rval.level_for(log_unit, parse_level(&level));
                    }
                }
                (false, true) => {
                    rval = rval.level(parse_level(&module_or_level));
                }
                _ => panic!("Invalid log statement: {}", item),
            }
        }
    }
    rval
}

pub fn init_logging() -> Result<(), fern::InitError> {
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
                // humantime::format_rfc3339_seconds(SystemTime::now()),
                record.target().magenta(),
                level_colors.color(record.level()),
                message
            ))
        })
        .level(log::LevelFilter::Info);
    dispatch = apply_env(dispatch);
    dispatch.chain(fern::Output::call(|record| println!("{}", record.args())))
    //dispatch.chain(std::io::stdout())
            .apply()?;
    Ok(())
}