use std::fs;
use std::path::{Path, PathBuf};
use chrono::{DateTime, Datelike, NaiveDateTime, Utc};
use log::warn;
use thiserror::Error;
use crate::worker_common::{Block, ImportSchedule};

/// Gets schedule (if any) for a given date time
///
/// # Arguments
///
/// * 'date_time' - the date time the schedule needs to include
pub fn get_schedule_for_date(schedule_dir: &str, date_time: DateTime<Utc>) -> Result<Option<ImportSchedule>, FileManagerError> {
    let path = format!("{}*_schedule.json", schedule_dir);
    for entry in glob::glob(&path)? {
        match entry {
            Ok(p) => {
                let (schedule_start, schedule_end) = get_schedule_time(&p)?;

                if date_time >= schedule_start && date_time < schedule_end {
                    let json = fs::read_to_string(p)?;
                    return Ok(Some(serde_json::from_str::<ImportSchedule>(&json)?))
                }
            }
            Err(e) => warn!("{:?}", e),
        }
    }

    Ok(None)
}

/// Loads scheduled blocks from file
///
/// # Arguments
///
/// * 'schedule_dir' - the directory to load the file from
/// * 'date_time' - datetime object used to check if the loaded schedule blocks are valid for the given day
pub fn load_scheduled_blocks(schedule_dir: &str, date_time: DateTime<Utc>) -> Result<Option<ImportSchedule>, FileManagerError> {
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

/// Saves scheduled blocks to file
///
/// # Arguments
///
/// * 'schedule_dir' - the directory to save the file to
/// * 'blocks' - schedule blocks to save
/// * 'mode_scheduler' - whether to use mode scheduler
/// * 'schedule_id' - id of the schedule, shall refer to the original schedule from the mygrid scheduler
pub fn save_scheduled_blocks(schedule_dir: &str, blocks: &Vec<Block>, soc_kwh: f64, mode_scheduler: bool, schedule_id: i64) -> Result<(), FileManagerError> {
    let file_path = format!("{}schedule.json", schedule_dir);

    let import_schedule = ImportSchedule {
        mode_scheduler,
        soc_kwh,
        blocks: blocks.clone(),
        schedule_id,
    };

    let json = serde_json::to_string_pretty(&import_schedule)?;

    fs::write(file_path, json)?;

    Ok(())
}

/// Returns the date time representation of the schedule start time which is
/// encoded in the schedule file name
///
/// # Arguments
///
/// * 'path_buf' - the full path to the schedule file
fn get_schedule_time(path_buf: &PathBuf) -> anyhow::Result<(DateTime<Utc>, DateTime<Utc>), FileManagerError> {
    let file_name = path_buf.file_name()
        .ok_or(FileManagerError::Other("error in schedule file name".to_string()))?
        .to_str()
        .ok_or(FileManagerError::Other("illegal character in schedule file name".to_string()))?;

    if file_name.len() != 39 {
        Err(FileManagerError::Other("malformed schedule file name".to_string()))?
    } else {
        let start = NaiveDateTime::parse_from_str(&file_name[0..12], "%Y%m%d%H%M")?.and_utc();
        let end = NaiveDateTime::parse_from_str(&file_name[13..25], "%Y%m%d%H%M")?.and_utc();
        Ok((start, end))
    }
}

#[derive(Error, Debug)]
#[error("error while managing file operations: {0}")]
pub enum FileManagerError {
    #[error("error while reading/writing file: {0}")]
    IO(#[from] std::io::Error),
    #[error("error while parsing JSON: {0}")]
    JSON(#[from] serde_json::Error),
    #[error("error while parsing glob pattern: {0}")]
    GLOB(#[from] glob::PatternError),
    #[error("error while parsing date: {0}")]
    DATE(#[from] chrono::format::ParseError),
    #[error("other error: {0}")]
    Other(String),
}