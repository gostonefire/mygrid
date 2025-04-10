use std::env;
use std::str::FromStr;
use chrono::Local;
use crate::{DEBUG_MODE, LAT, LONG};
use crate::backup::{load_base_data, load_last_charge, load_active_block, load_pv_diagram};
use crate::charge::LastCharge;
use crate::errors::{MyGridInitError};
use crate::manager_fox_cloud::Fox;
use crate::manager_mail::Mail;
use crate::manager_nordpool::NordPool;
use crate::manager_smhi::SMHI;
use crate::scheduling::Block;

/// Initializes and returns Fox, NordPool, SMHI and Schedule structs and backup dir
///
pub fn init() -> Result<(Fox, NordPool, SMHI, Mail, [f64;1440], Option<Block>, Option<LastCharge>, String, String, String), MyGridInitError> {
    let api_key = env::var("FOX_ESS_API_KEY")
        .expect("Error getting FOX_ESS_API_KEY");
    let inverter_sn = env::var("FOX_ESS_INVERTER_SN")
        .expect("Error getting FOX_ESS_INVERTER_SN");
    let backup_dir = env::var("BACKUP_DIR")
        .expect("Error getting BACKUP_DIR");
    let stats_dir = env::var("STATS_DIR")
        .expect("Error getting STATS_DIR");
    let config_dir = env::var("CONFIG_DIR")
        .expect("Error getting CONFIG_DIR");
    let manual_file = env::var("MANUAL_FILE")
        .expect("Error getting MANUAL_FILE");
    let mail_api_key = env::var("MAIL_API_KEY")
        .expect("Error getting MAIL_PASSWORD");
    let mail_from = env::var("MAIL_FROM")
        .expect("Error getting MAIL_FROM");
    let mail_to = env::var("MAIL_TO")
        .expect("Error getting MAIL_TO");

    let debug_mode = env::var("DEBUG_MODE").unwrap_or("false".to_string());
    {
        *DEBUG_MODE.write()? = bool::from_str(debug_mode.as_str()).unwrap_or(false);
        if *DEBUG_MODE.read()? {
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

    let pv_diagram = load_pv_diagram(&config_dir)?;

    let local_now = Local::now();
    let last_charge = load_last_charge(&backup_dir)?;
    let active_block = load_active_block(&backup_dir, local_now)?;
    if let Some(base_data) = load_base_data(&backup_dir, local_now)? {
        smhi.set_forecast(base_data.forecast);
    }

    Ok((fox, nordpool, smhi, mail, pv_diagram, active_block, last_charge, backup_dir, stats_dir, manual_file))
}