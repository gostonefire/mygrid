use std::fs;
use std::path::Path;
use chrono::{DateTime, Datelike, Duration, Local, NaiveDateTime, Utc};
use glob::glob;
use serde::Serialize;
use crate::errors::BackupError;
use crate::models::forecast::ForecastValues;
use crate::consumption::ConsumptionValues;
use crate::models::nordpool_tariffs::TariffValues;
use crate::manager_production::ProductionValues;
use crate::scheduler::Block;


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

/// Saves scheduled blocks to file
/// 
/// # Arguments
/// 
/// * 'backup_dir' - the directory to save the file to
/// * 'blocks' - schedule blocks to save
pub fn save_schedule_blocks(backup_dir: &str, blocks: &Vec<Block>) -> Result<(), BackupError> {
    let file_path = format!("{}schedule.json", backup_dir);
    
    let json = serde_json::to_string_pretty(blocks)?;
    
    fs::write(file_path, json)?;
    
    Ok(())
}

/// Loads scheduled blocks from file
/// 
/// # Arguments
/// 
/// * 'backup_dir' - the directory to load the file from
/// * 'date_time' - datetime object used to check if the loaded schedule blocks are valid for the given day
pub fn load_schedule_blocks(backup_dir: &str, date_time: DateTime<Local>) -> Result<Option<Vec<Block>>, BackupError> {
    let file_path = format!("{}schedule.json", backup_dir);
    let day = date_time.ordinal0();
    
    if Path::new(&file_path).exists() {
        let json = fs::read_to_string(file_path)?;
        let blocks: Vec<Block> = serde_json::from_str(&json)?;
        
        if blocks.iter().any(|b| b.start_time.ordinal0() == day) {
            Ok(Some(blocks))
        } else {
            Ok(None)
        }
    } else {
        Ok(None)
    }
}