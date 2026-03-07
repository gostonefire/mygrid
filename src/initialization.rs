use std::{env, fs};
use std::path::PathBuf;
use log::info;
use foxess::{Fox, FoxError};
use thiserror::Error;
use crate::{UtcNow, DEBUG_MODE, LOGGER_INITIALIZED};
use crate::config::{load_config, Config, ConfigError};
use crate::logging::{setup_logger, LoggingError};
use crate::manager_mail::errors::MailError;
use crate::manager_mail::Mail;

pub struct Mgr {
    pub fox: Fox,
    pub mail: Mail,
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
    if !*LOGGER_INITIALIZED.read().map_err(|e| MyGridInitError::LockPoisonRead(e.to_string()))? {
        let _ = setup_logger(&config.general.log_path, config.general.log_level, config.general.log_to_stdout)?;
    }
    *LOGGER_INITIALIZED.write().map_err(|e| MyGridInitError::LockPoisonWrite(e.to_string()))? = true;

    // Print version
    info!("mygrid version: {}", env!("CARGO_PKG_VERSION"));

    // Set debug mode on/off
    *DEBUG_MODE.write().map_err(|e| MyGridInitError::LockPoisonWrite(e.to_string()))? = config.general.debug_mode;
    if *DEBUG_MODE.read().map_err(|e| MyGridInitError::LockPoisonRead(e.to_string()))? {
        info!("running in Debug Mode!!");
    }

    // Instantiate time object
    let time = UtcNow::new(config.general.debug_run_time);

    // Instantiate structs
    let fox = Fox::new(&config.fox_ess.api_key, &config.fox_ess.inverter_sn, 30)?;
    let mail = Mail::new(&config.mail)?;


    let mgr = Mgr {
        fox,
        mail,
        time,
    };
 
    Ok((config, mgr))
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

#[derive(Error, Debug)]
pub enum MyGridInitError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Logger(#[from] LoggingError),
    #[error(transparent)]
    Mailer(#[from] MailError),
    #[error("lock poison error: {0}")]
    LockPoisonRead(String),
    #[error("lock poison error: {0}")]
    LockPoisonWrite(String),
    #[error(transparent)]
    FoxESS(#[from] FoxError),
    #[error("error while reading credential: {0}")]
    ReadCredentialIO(#[from] std::io::Error),
    #[error("error while reading credential: {0}")]
    ReadCredentialEnv(#[from] env::VarError),
    #[error("error while reading credential: {0}")]
    ReadCredentialUtf8(#[from] std::string::FromUtf8Error),
}
