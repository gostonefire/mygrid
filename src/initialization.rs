use std::env;
use chrono::Local;
use log::info;
use crate::{DEBUG_MODE, LOGGER_INITIALIZED};
use crate::backup::load_schedule_blocks;
use crate::config::{load_config, Config};
use crate::errors::{MyGridInitError};
use crate::logging::setup_logger;
use crate::manager_fox_cloud::Fox;
use crate::manager_mail::Mail;
use crate::scheduler::Schedule;

pub struct Mgr {
    pub fox: Fox,
    pub mail: Mail,
    pub schedule: Schedule,
}

/// Initializes and returns configuration, a Mgr struct holding various of initialized structs, 
/// an optional LastCharge struct, and an optional active block
///
pub fn init() -> Result<(Config, Mgr), MyGridInitError> {
    let args: Vec<String> = env::args().collect();
    let config_path = args.iter()
        .find(|p| p.starts_with("--config="))
        .expect("config file argument should be present");
    let config_path = config_path
        .split_once('=')
        .expect("config file argument should be correct")
        .1;


    // Load configuration
    let config = load_config(&config_path)?;

    // Setup logging
    if !*LOGGER_INITIALIZED.read()? {
        let _ = setup_logger(&config.general.log_path, config.general.log_level, config.general.log_to_stdout)?;
    }
    *LOGGER_INITIALIZED.write()? = true;

    // Print version
    info!("mygrid version: {}", env!("CARGO_PKG_VERSION"));

    // Set debug mode on/off
    *DEBUG_MODE.write()? = config.general.debug_mode;
    if *DEBUG_MODE.read()? {
        info!("running in Debug Mode!!");
    }
    
    // Load any existing schedule blocks
    let schedule_blocks = load_schedule_blocks(&config.files.schedule_dir, Local::now())?;
    
    // Instantiate structs
    let fox = Fox::new(&config.fox_ess);
    let mail = Mail::new(&config.mail)?;
    let schedule = Schedule::new(&config.files.schedule_dir, config.charge.soc_kwh, schedule_blocks);

    let mgr = Mgr {
        fox,
        mail,
        schedule,
    };
 
    Ok((config, mgr))
}