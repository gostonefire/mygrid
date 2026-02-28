use std::{env, fs};
use std::path::{Path, PathBuf};
use chrono::{DateTime, Datelike, Utc};
use log::info;
use anyhow::Result;
use crate::{UtcNow, DEBUG_MODE, LOGGER_INITIALIZED};
use crate::config::{load_config, Config};
use crate::errors::MyGridInitError;
use crate::logging::setup_logger;
use crate::manager_fox_cloud::Fox;
use crate::manager_mail::Mail;
use crate::scheduler::{ImportSchedule, Schedule};

pub struct Mgr {
    pub fox: Fox,
    pub mail: Mail,
    pub schedule: Schedule,
    pub time: UtcNow,
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
    let mut config = load_config(&config_path)?;
    config.fox_ess.api_key = read_credential("fox_ess_api_key")?;
    config.fox_ess.inverter_sn = read_credential("fox_ess_inverter_sn")?;
    config.mail.smtp_user = read_credential("mail_smtp_user")?;
    config.mail.smtp_password = read_credential("mail_smtp_password")?;

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

    // Instantiate time object
    let time = UtcNow::new(config.general.debug_run_time);

    // Load any existing schedule blocks
    let import_schedule = load_schedule_blocks(&config.files.schedule_dir, time.utc_now())?;
    
    // Instantiate structs
    let fox = Fox::new(&config.fox_ess);
    let mail = Mail::new(&config.mail)?;
    let schedule = Schedule::new(&config.files.schedule_dir, config.charge.soc_kwh, import_schedule);


    let mgr = Mgr {
        fox,
        mail,
        schedule,
        time,
    };
 
    Ok((config, mgr))
}

/// Loads scheduled blocks from file
///
/// # Arguments
///
/// * 'schedule_dir' - the directory to load the file from
/// * 'date_time' - datetime object used to check if the loaded schedule blocks are valid for the given day
pub fn load_schedule_blocks(schedule_dir: &str, date_time: DateTime<Utc>) -> Result<Option<ImportSchedule>, MyGridInitError> {
    let file_path = format!("{}schedule.json", schedule_dir);
    let day = date_time.ordinal0();

    if Path::new(&file_path).exists() {
        let json = fs::read_to_string(file_path)?;
        let import_schedule: ImportSchedule = serde_json::from_str(&json)?;

        if import_schedule.blocks.iter().any(|b| b.start_time.ordinal0() == day) {
            Ok(Some(import_schedule))
        } else {
            Ok(None)
        }
    } else {
        Ok(None)
    }
}

/// Reads a credential from the file system supported by the credstore and
/// given from systemd
///
/// # Arguments
///
/// * 'name' - name of the credential to read
fn read_credential(name: &str) -> Result<String, MyGridInitError> {
    let dir = env::var("CREDENTIALS_DIRECTORY")?;
    let mut p = PathBuf::from(dir);
    p.push(name);
    let bytes = fs::read(p)?;
    Ok(String::from_utf8(bytes)?.trim_end().to_string())
}