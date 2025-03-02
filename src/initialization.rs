use std::env;
use crate::{LAT, LONG};
use crate::manager_fox_cloud::Fox;
use crate::manager_nordpool::NordPool;
use crate::manager_smhi::SMHI;
use crate::scheduling::{create_new_schedule, load_schedule, Schedule};

/// Initializes and returns Fox, NordPool, SMHI and Schedule structs and backup dir
///
pub fn init() -> Result<(Fox, NordPool, SMHI, Schedule, String), String> {
    let api_key = match env::var("FOX_ESS_API_KEY") {
        Ok(v) => v,
        Err(e) => { return Err(format!("Error getting API key: {}", e)); }
    };
    let inverter_sn = match env::var("FOX_ESS_INVERTER_SN") {
        Ok(v) => v,
        Err(e) => { return Err(format!("Error getting inverter SN: {}", e)); }
    };
    let backup_dir = match env::var("BACKUP_DIR") {
        Ok(v) => v,
        Err(e) => { return Err(format!("Error getting backup directory: {}", e)); }
    };

    // Instantiate structs
    let fox = Fox::new(api_key, inverter_sn);
    let nordpool = NordPool::new();
    let smhi = SMHI::new(LAT, LONG);

    // Create a first base schedule given only tariffs, charge level will later be updated
    let mut schedule: Schedule;
    match create_new_schedule(&nordpool, &smhi, None) {
        Ok(s) => schedule = s,
        Err(e) => { return Err(format!("Error creating new schedule during init: {}", e)); }
    }
    for s in &schedule.blocks {
        println!("{}", s);
    }
    println!("Startup ========================================================================");

    // Check if we have an existing schedule for the day that then may be updated with
    // already started/running blocks
    match load_schedule(&backup_dir) {
        Ok(option) => {
            if let Some(s) = option {
                schedule = s;
            };
        },
        Err(e) => { return Err(format!("Error loading backup schedule: {}", e)); }
    }
    for s in &schedule.blocks {
        println!("{}", s);
    }
    println!("Backup =========================================================================");

    Ok((fox, nordpool, smhi, schedule, backup_dir))
}