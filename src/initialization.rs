use std::env;
use log::info;
use crate::{DEBUG_MODE, LOGGER_INITIALIZED};
use crate::backup::{load_last_charge, load_active_block};
use crate::charge::LastCharge;
use crate::config::{load_config, Config};
use crate::consumption::Consumption;
use crate::errors::{MyGridInitError};
use crate::logging::setup_logger;
use crate::manager_fox_cloud::Fox;
use crate::manager_mail::Mail;
use crate::manager_nordpool::NordPool;
use crate::manager_smhi::SMHI;
use crate::production_legacy::PVProduction;
use crate::scheduling::{Block, Schedule};

pub struct Mgr {
    pub fox: Fox,
    pub nordpool: NordPool,
    pub smhi: SMHI,
    pub pv: PVProduction,
    pub cons: Consumption,
    pub mail: Mail,
    pub schedule: Schedule,
}

/// Initializes and returns configuration, a Mgr struct holding various of initialized structs, 
/// an optional LastCharge struct, and an optional active block
///
pub fn init() -> Result<(Config, Mgr, Option<LastCharge>, Option<Block>), MyGridInitError> {
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
    
    // Instantiate structs
    let fox = Fox::new(&config.fox_ess);
    let nordpool = NordPool::new();
    let smhi = SMHI::new(&config);
    let pv = PVProduction::new(&config.production, &config.geo_ref);
    let cons = Consumption::new(&config.consumption);
    let mail = Mail::new(&config.mail)?;
    let schedule = Schedule::new(&config.charge);

    let mgr = Mgr {
        fox,
        nordpool,
        smhi,
        pv,
        cons,
        mail,
        schedule,
    };
 
    let last_charge = load_last_charge(&config.files.backup_dir)?;
    let active_block = load_active_block(&config.files.backup_dir)?;

    Ok((config, mgr, last_charge, active_block))
}