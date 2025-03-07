use std::env;
use std::str::FromStr;
use chrono::Local;
use crate::{DEBUG_MODE, LAT, LONG};
use crate::errors::{MyGridInitError};
use crate::manager_fox_cloud::Fox;
use crate::manager_nordpool::NordPool;
use crate::manager_smhi::SMHI;
use crate::scheduling::{create_new_schedule, load_schedule, Schedule};
use crate::worker::print_schedule;

/// Initializes and returns Fox, NordPool, SMHI and Schedule structs and backup dir
///
pub fn init() -> Result<(Fox, NordPool, SMHI, Schedule, String), MyGridInitError> {
    let api_key = env::var("FOX_ESS_API_KEY")
        .map_err(|e|MyGridInitError(format!("getting API key: {}", e)))?;

    let inverter_sn = env::var("FOX_ESS_INVERTER_SN")
        .map_err(|e|MyGridInitError(format!("getting inverter_sn: {}", e)))?;

    let backup_dir = env::var("BACKUP_DIR")
        .map_err(|e|MyGridInitError(format!("getting backup dir: {}", e)))?;

    let debug_mode = env::var("DEBUG_MODE").unwrap_or("false".to_string());
    unsafe {
        DEBUG_MODE = bool::from_str(debug_mode.as_str()).unwrap_or(false);
        if DEBUG_MODE {
            println!("Running in Debug Mode!!");
        }
    }

    // Instantiate structs
    let fox = Fox::new(api_key, inverter_sn);
    let nordpool = NordPool::new();
    let mut smhi = SMHI::new(LAT, LONG);

    // Create a first base schedule given only tariffs, charge level will later be updated
    let mut schedule = create_new_schedule(&nordpool, &mut smhi, Local::now())?;
    print_schedule(&schedule, "Startup");

    // Check if we have an existing schedule for the day that then may be updated with
    // already started/running blocks
    if let Some(s) = load_schedule(&backup_dir)? {
        schedule = s;
    }
    print_schedule(&schedule, "From Backup");

    Ok((fox, nordpool, smhi, schedule, backup_dir))
}