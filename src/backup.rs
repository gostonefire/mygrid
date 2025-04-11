use std::fs;
use std::fs::File;
use std::io::Write;
use std::ops::Add;
use std::path::Path;
use std::thread;
use chrono::{DateTime, Datelike, DurationRound, Local, TimeDelta, Timelike, Utc};
use serde::{Deserialize, Serialize};
use crate::errors::BackupError;
use crate::manager_fox_cloud::Fox;
use crate::models::smhi_forecast::TimeValues;
use crate::{retry, wrapper};
use crate::charge::LastCharge;
use crate::scheduling::{Block};


#[derive(Serialize, Deserialize)]
pub struct BaseData {
    date_time: DateTime<Local>,
    pub forecast: [TimeValues; 24],
    production: [f64; 24],
    consumption: [f64; 24],
}

#[derive(Deserialize)]
struct PVDiagram {
    pv_data: Vec<f64>,
}

#[derive(Deserialize)]
pub struct ConsumptionDiagram {
    pub day: [[f64; 24];7],
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
pub fn save_base_data(
    backup_dir: &str,
    date_time: DateTime<Local>,
    forecast: [TimeValues; 24],
    production: [f64; 24],
    consumption: [f64; 24]) -> Result<(), BackupError> {

    if Local::now().timestamp() >= date_time.timestamp() {
        let file_path = format!("{}base_data.json", backup_dir);

        let backup = BaseData {
            date_time,
            forecast,
            production,
            consumption,
        };

        let json = serde_json::to_string_pretty(&backup)?;
        fs::write(file_path, json)?;
    }

    Ok(())
}

/// Loads last saved base data from file
///
/// # Arguments
///
/// * 'backup_dir' - the directory to save the file to
/// * 'date_time' - the date the base data must represent
pub fn load_base_data(backup_dir: &str, date_time: DateTime<Local>) -> Result<Option<BaseData>, BackupError> {
    let file_path = format!("{}base_data.json", backup_dir);

    let path = Path::new(&file_path);
    if path.exists() {
        let json = fs::read_to_string(path)?;
        let state: BaseData = serde_json::from_str(&json)?;

        if state.date_time.ordinal0() == date_time.ordinal0() {
            return Ok(Some(state));
        }
    }

    Ok(None)
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
/// If none is found or if the data is older than 23 hours then None is returned.
/// 23 hours since getting device history data is limited to max 23 hours, 59minutes and 59 seconds.
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
/// * 'date_time' - the date and time that the state must cover to be returned
pub fn load_active_block(backup_dir: &str, date_time: DateTime<Local>) -> Result<Option<Block>, BackupError> {
    let file_path = format!("{}active_block.json", backup_dir);

    let path = Path::new(&file_path);
    if path.exists() {
        let json = fs::read_to_string(path)?;
        let block: Block = serde_json::from_str(&json)?;

        let date = date_time.date_naive();
        let hour = date_time.hour() as usize;
        if block.date == date && hour >= block.start_hour && hour <= block.end_hour {
            return Ok(Some(block))
        }
    }

    Ok(None)
}

/// Loads PV Diagram data
///
/// # Arguments
///
/// * 'config_dir' - the directory where to find config files
pub fn load_pv_diagram(config_dir: &str) -> Result<[f64;1440], BackupError> {
    let file_path = format!("{}pv_diagram.json", config_dir);

    let path = Path::new(&file_path);
    if path.exists() {
        let mut result: [f64;1440] = [0.0;1440];

        let json = fs::read_to_string(path)?;
        let pv_diagram: PVDiagram = serde_json::from_str(&json)?;

        if pv_diagram.pv_data.len() != 1440 {
            return Err(BackupError::from("PV diagram length mismatch"))
        }

        for (i, p) in pv_diagram.pv_data.iter().enumerate() {
            result[i] = *p;
        }

        Ok(result)
    } else {
        Err(BackupError::from("PV diagram file not found"))
    }
}

/// Loads consumption diagram configuration
///
/// # Arguments
///
/// * 'config_dir' - the directory where to find config files
pub fn load_consumption_diagram(config_dir: &str) -> Result<[[f64;24];7], BackupError> {
    let file_path = format!("{}consumption_diagram.json", config_dir);

    let path = Path::new(&file_path);
    if path.exists() {
        let json = fs::read_to_string(path)?;
        let consumption_diagram: ConsumptionDiagram = serde_json::from_str(&json)?;

        Ok(consumption_diagram.day)
    } else {
        Err(BackupError::from("consumption diagram file not found"))
    }
}

/// Gat and saves statistics from yesterday
///
/// # Arguments
///
/// * 'stats_dir' - the directory to save the file to
/// * 'fox' - reference to the Fox struct
pub fn save_yesterday_statistics(stats_dir: &str, fox: &Fox) -> Result<(), BackupError> {
    let start = Local::now()
        .add(chrono::Duration::days(-1))
        .duration_trunc(TimeDelta::days(1))?
        .with_timezone(&Utc);
    let end =  start
        .add(chrono::Duration::days(1))
        .add(chrono::Duration::seconds(-1));
    let device_history = retry!(||fox.get_device_history_data(start, end))?;

    let file_path = format!("{}{}.csv", stats_dir, device_history.date.format("%Y%m%d"));

    let x =device_history.pv_power
        .iter()
        .zip(device_history.ld_power.iter())
        .zip(device_history.time.iter()).map(|((&p, &l), t)| (t.clone(), p, l))
        .collect::<Vec<(String, f64, f64)>>();

    let mut f = File::create(file_path)?;
    write!(f, "time,pvPower,ldPower\n")?;
    for l in x {
        write!(f, "{},{},{}\n", l.0, l.1, l.2)?
    }

    Ok(())
}
