use std::fs;
use log::LevelFilter;
use serde::Deserialize;
use anyhow::Result;
use chrono::{DateTime, Local};
use crate::errors::ConfigError;

#[derive(Deserialize)]
pub struct Charge {
    pub soc_kwh: f64,
}

#[derive(Deserialize)]
pub struct FoxESS {
    pub api_key: String,
    pub inverter_sn: String,
}

#[derive(Deserialize)]
pub struct MailParameters {
    pub smtp_user: String,
    pub smtp_password: String,
    pub smtp_endpoint: String,
    pub from: String,
    pub to: String,
}

#[derive(Deserialize)]
pub struct Files {
    pub schedule_dir: String,
    pub manual_file: String,
}

#[derive(Deserialize)]
pub struct General {
    pub debug_run_time: Option<DateTime<Local>>,
    pub log_path: String,
    pub log_level: LevelFilter,
    pub log_to_stdout: bool,
    pub debug_mode: bool,
}

#[derive(Deserialize)]
pub struct Config {
    pub charge: Charge,
    pub fox_ess: FoxESS,
    pub mail: MailParameters,
    pub files: Files,
    pub general: General,
}

/// Loads the configuration file and returns a struct with all configuration items
/// 
/// # Arguments
/// 
/// * 'config_path' - path to the configuration file
pub fn load_config(config_path: &str) -> Result<Config, ConfigError> {
    
    let toml = fs::read_to_string(config_path)?;
    let config: Config = toml::from_str(&toml)?;
    
    Ok(config)
}