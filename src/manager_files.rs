use std::fs;
use std::path::{Path, PathBuf};
use chrono::{DateTime, Datelike, NaiveDateTime, Utc};
use log::warn;
use anyhow::Result;
use crate::errors::FileManagerError;
use crate::manual_scheduler::{Block, ImportSchedule};

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
pub fn save_scheduled_blocks(schedule_dir: &str, blocks: &Vec<Block>, soc_kwh: f64, mode_scheduler: bool) -> Result<(), FileManagerError> {
    let file_path = format!("{}schedule.json", schedule_dir);

    let import_schedule = ImportSchedule {
        mode_scheduler,
        soc_kwh,
        blocks: blocks.clone(),
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
        .ok_or("Error in schedule file name")?
        .to_str()
        .ok_or("Illegal character in schedule file name")?;

    if file_name.len() != 39 {
        Err("malformed schedule file name")?
    } else {
        let start = NaiveDateTime::parse_from_str(&file_name[0..12], "%Y%m%d%H%M")?.and_utc();
        let end = NaiveDateTime::parse_from_str(&file_name[13..25], "%Y%m%d%H%M")?.and_utc();
        Ok((start, end))
    }
}
