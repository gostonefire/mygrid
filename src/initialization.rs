use std::env;
use std::str::FromStr;
use chrono::Local;
use crate::{DEBUG_MODE, LAT, LONG};
use crate::backup::load_backup;
use crate::errors::{MyGridInitError};
use crate::manager_fox_cloud::Fox;
use crate::manager_mail::Mail;
use crate::manager_nordpool::NordPool;
use crate::manager_smhi::SMHI;
use crate::scheduling::{create_new_schedule, update_existing_schedule, Schedule};
use crate::worker::print_schedule;

/// Initializes and returns Fox, NordPool, SMHI and Schedule structs and backup dir
///
pub fn init() -> Result<(Fox, NordPool, SMHI, Schedule, Mail, String, String), MyGridInitError> {
    let api_key = env::var("FOX_ESS_API_KEY")
        .expect("Error getting FOX_ESS_API_KEY");
    let inverter_sn = env::var("FOX_ESS_INVERTER_SN")
        .expect("Error getting FOX_ESS_INVERTER_SN");
    let backup_dir = env::var("BACKUP_DIR")
        .expect("Error getting BACKUP_DIR");
    let stats_dir = env::var("STATS_DIR")
        .expect("Error getting STATS_DIR");
    let mail_api_key = env::var("MAIL_API_KEY")
        .expect("Error getting MAIL_PASSWORD");
    let mail_from = env::var("MAIL_FROM")
        .expect("Error getting MAIL_FROM");
    let mail_to = env::var("MAIL_TO")
        .expect("Error getting MAIL_TO");

    let debug_mode = env::var("DEBUG_MODE").unwrap_or("false".to_string());
    unsafe {
        DEBUG_MODE = bool::from_str(debug_mode.as_str()).unwrap_or(false);
        if DEBUG_MODE {
            println!("Running in Debug Mode!!");
        }
    }

    // Print version
    println!("mygrid version: {}", env!("CARGO_PKG_VERSION"));

    // Instantiate structs
    let fox = Fox::new(api_key, inverter_sn);
    let nordpool = NordPool::new();
    let mut smhi = SMHI::new(LAT, LONG);
    let mail = Mail::new(mail_api_key, mail_from, mail_to)?;

    let mut schedule: Schedule;

    // Check if we have an existing schedule for the day that then may be updated with
    // already started/running blocks
    if let Some(b) = load_backup(&backup_dir)? {
        smhi.set_forecast(b.forecast);
        schedule = b.schedule;
        update_existing_schedule(&mut schedule, &mut smhi, &backup_dir)?;
        print_schedule(&schedule, "From Backup", None);
    } else {
        schedule = create_new_schedule(&nordpool, &mut smhi, Local::now(), &backup_dir)?;
        print_schedule(&schedule, "Startup", None);
    }

    Ok((fox, nordpool, smhi, schedule, mail, backup_dir, stats_dir))
}