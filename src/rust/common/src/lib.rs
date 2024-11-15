pub mod logging;
use anyhow;
use anyhow::Context;

pub fn init() -> Result<(), anyhow::Error> {
    logging::init_logging()
             .with_context(|| "Failed to initialize logging")?;
    Ok(())
}