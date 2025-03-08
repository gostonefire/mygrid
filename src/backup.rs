use std::{fmt, fs};
use std::fmt::Formatter;
use std::fs::File;
use std::io::Read;
use chrono::{DateTime, Local};
use glob::glob;
use serde::{Deserialize, Serialize};
use crate::models::smhi_forecast::TimeValues;
use crate::scheduling::{Schedule};

pub struct BackupError(String);
impl fmt::Display for BackupError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "BackupError: {}", self.0)
    }
}
impl From<std::io::Error> for BackupError {
    fn from(e: std::io::Error) -> Self {
        BackupError(e.to_string())
    }
}
impl From<serde_json::Error> for BackupError {
    fn from(e: serde_json::Error) -> Self {
        BackupError(e.to_string())
    }
}
impl From<glob::PatternError> for BackupError {
    fn from(e: glob::PatternError) -> Self {
        BackupError(e.to_string())
    }
}
impl From<glob::GlobError> for BackupError {
    fn from(e: glob::GlobError) -> Self {
        BackupError(e.to_string())
    }
}

#[derive(Serialize, Deserialize)]
pub struct Backup {
    date_time: DateTime<Local>,
    pub forecast: [TimeValues; 24],
    production: [f64; 24],
    consumption: [f64; 24],
    pub schedule: Schedule,
}

/// Saves a backup to file as json if the given date_time is not in the future
/// The filename gives at most one unique file per hour
///
/// # Arguments
///
/// * 'backup_dir' - the directory to save the file to
/// * 'date_time' - the date and time the backup represents
/// * 'forecast' - the smhi forecast to save
/// * 'production' - the production estimates to save
/// * 'consumption' - the consumption estimates to save
/// * 'schedule' - the schedule to save
pub fn save_backup(
    backup_dir: &str,
    date_time: DateTime<Local>,
    forecast: [TimeValues; 24],
    production: [f64; 24],
    consumption: [f64; 24],
    schedule: &Schedule) -> Result<(), BackupError> {

    if Local::now().timestamp() >= date_time.timestamp() {
        let file_path = format!("{}{}.json", backup_dir, date_time.format("%Y%m%d_%H"));
        let s = schedule.clone();

        let backup = Backup {
            date_time,
            forecast,
            production,
            consumption,
            schedule: s,
        };

        let json = serde_json::to_string(&backup)?;
        fs::write(file_path, json)?;
    }

    Ok(())
}

/// Loads backup from json on file
///
/// it will look for the most resent backup for the current day
///
/// # Arguments
///
/// * 'backup_dir' - the directory to save the file to
pub fn load_backup(backup_dir: &str) -> Result<Option<Backup>, BackupError> {
    let mut entries: Vec<String> = Vec::new();
    let file_path = format!("{}{}.json", backup_dir, Local::now().format("%Y%m%d*"));
    for entry in glob(&file_path)? {
        let path = entry?;
        if path.is_file() {
            if let Some(os_path) = path.to_str() {
                entries.push(os_path.to_string());
            }
        }
    }

    entries.sort();

    if entries.len() > 0 {
        let mut file = File::open(&entries[entries.len() - 1])?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let backup: Backup = serde_json::from_str(&contents)?;

        Ok(Some(backup))
    } else {
        Ok(None)
    }
}
