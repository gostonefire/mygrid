use std::fs;
use std::path::Path;
use chrono::{DateTime, Duration, DurationRound, Local, NaiveDateTime, TimeDelta, Utc};
use glob::glob;
use serde::Serialize;
use crate::errors::BackupError;
use crate::models::forecast::ForecastValues;
use crate::charge::LastCharge;
use crate::consumption::ConsumptionValues;
use crate::models::nordpool_tariffs::TariffValues;
use crate::manager_production::ProductionValues;
use crate::scheduling::{Block, Schedule};


#[derive(Serialize)]
pub struct BaseData<'a> {
    date_time: DateTime<Local>,
    forecast: &'a Vec<ForecastValues>,
    production: &'a Vec<ProductionValues>,
    production_kw: &'a Vec<ProductionValues>,
    consumption: &'a Vec<ConsumptionValues>,
    tariffs: &'a Vec<TariffValues>,
}

/// Saves base data used in the creation of a schedule if time is not in the future
///
/// # Arguments
///
/// * 'backup_dir' - the directory to save the file to
/// * 'date_time' - the date and time the state represents
/// * 'forecast' - the smhi forecast to save
/// * 'production' - the production estimates to save
/// * 'consumption' - the consumption estimates to save
/// * 'tariffs' - tariffs from NordPool with VAT and markup
pub fn save_base_data(
    backup_dir: &str,
    date_time: DateTime<Local>,
    forecast: &Vec<ForecastValues>,
    production: &Vec<ProductionValues>,
    production_kw: &Vec<ProductionValues>,
    consumption: &Vec<ConsumptionValues>,
    tariffs: &Vec<TariffValues>) -> Result<(), BackupError> {

    if Local::now().timestamp() >= date_time.timestamp() {
        let file_path = format!("{}{}_base_data.json", backup_dir, Utc::now().format("%Y%m%d%H%M%S"));

        let backup = BaseData {
            date_time,
            forecast,
            production,
            production_kw,
            consumption,
            tariffs,
        };

        let json = serde_json::to_string_pretty(&backup)?;
        fs::write(file_path, json)?;
    }

    // Remove base data files older than 48 hours
    let pattern = format!("{}*_base_data.json", backup_dir);
    for entry in glob(&pattern)? {
        if let Ok(path) = entry {
            if let Some(os_name) = path.file_name() {
                if let Some(filename) = os_name.to_str() {
                    let datetime: DateTime<Utc> = NaiveDateTime::parse_from_str(&filename[0..14], "%Y%m%d%H%M%S")?.and_utc();
                    if Utc::now() - datetime > Duration::hours(48) {
                        fs::remove_file(path)?;
                    }
                }
            }
        }
    }

    Ok(())
}

/// Saves data about the last charge from grid to battery
///
/// # Arguments
///
/// * 'backup_dir' - the directory to save the file to
/// * 'last_charge' - information from the last charge from grid
pub fn save_last_charge(backup_dir: &str, last_charge: &Option<LastCharge>) -> Result<(), BackupError> {
    if let Some(last_charge) = last_charge {
        let file_path = format!("{}last_charge.json", backup_dir);

        let json = serde_json::to_string_pretty(last_charge)?;
        fs::write(file_path, json)?;
    }

    Ok(())
}

/// Loads data about the last charge made from grid to battery.
/// If none is found or if the data is older than 23 hours, then None is returned.
/// 23 hours since getting device history data is limited to max 23 hours, 59 minutes and 59 seconds.
/// By choosing 23 hours sharp gives some wiggle room and easier calculation later on.
///
/// # Arguments
///
/// * 'backup_dir' - the directory to save the file to
pub fn load_last_charge(backup_dir: &str) -> Result<Option<LastCharge>, BackupError> {
    let file_path = format!("{}last_charge.json", backup_dir);

    let path = Path::new(&file_path);
    if path.exists() {
        let json = fs::read_to_string(path)?;
        let last_charge: LastCharge = serde_json::from_str(&json)?;

        if Local::now() - last_charge.date_time_end <= TimeDelta::hours(23) {
            return Ok(Some(last_charge))
        }
    }

    Ok(None)
}

/// Saves a state represented by a Block
///
/// # Arguments
///
/// * 'backup_dir' - the directory to save the file to
/// * 'block' - the block that represents a state to save
pub fn save_active_block(backup_dir: &str, block: &Block) -> Result<(), BackupError> {
    let file_path = format!("{}active_block.json", backup_dir);

    let json = serde_json::to_string_pretty(block)?;
    fs::write(file_path, json)?;

    Ok(())
}

/// Loads state represented by a Block
///
/// # Arguments
///
/// * 'backup_dir' - the directory to save the file to
pub fn load_active_block(backup_dir: &str) -> Result<Option<Block>, BackupError> {
    let file_path = format!("{}active_block.json", backup_dir);

    let path = Path::new(&file_path);
    if path.exists() {
        let json = fs::read_to_string(path)?;
        let block: Block = serde_json::from_str(&json)?;

        let date_hour = Local::now().duration_trunc(TimeDelta::hours(1))?;
        if block.start_time <= date_hour && block.end_time >= date_hour {
            return Ok(Some(block))
        }
    }

    Ok(None)
}

/// Saves scheduled blocks to file
/// 
/// # Arguments
/// 
/// * 'backup_dir' - the directory to save the file to
/// * 'schedule' - schedule containing blocks to save
pub fn save_schedule(backup_dir: &str, schedule: &Schedule) -> Result<(), BackupError> {
    let file_path = format!("{}schedule.json", backup_dir);
    
    let json = serde_json::to_string_pretty(&schedule.blocks)?;
    
    fs::write(file_path, json)?;
    
    Ok(())
}