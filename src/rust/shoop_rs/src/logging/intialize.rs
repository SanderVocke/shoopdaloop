use fern;
use std::time::SystemTime;
use humantime;

pub fn init_logging() -> Result<(), fern::InitError> {
    let colors = fern::colors::ColoredLevelConfig::new()
        .info(fern::colors::Color::Green);
    fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "[{}] [{}] [{}] {}",
                humantime::format_rfc3339_seconds(SystemTime::now()),
                record.target(),
                colors.color(record.level()),
                message
            ))
        })
        .level(log::LevelFilter::Debug)
        .chain(std::io::stdout())
        .apply()?;
    Ok(())
}