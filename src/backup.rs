use std::fs;
use std::path::Path;
use chrono::{DateTime, Datelike, Local};
use crate::errors::BackupError;
use crate::scheduler::Block;


/// Saves scheduled blocks to file
/// 
/// # Arguments
/// 
/// * 'schedule_dir' - the directory to save the file to
/// * 'blocks' - schedule blocks to save
pub fn save_schedule_blocks(schedule_dir: &str, blocks: &Vec<Block>) -> Result<(), BackupError> {
    let file_path = format!("{}schedule.json", schedule_dir);
    
    let json = serde_json::to_string_pretty(blocks)?;
    
    fs::write(file_path, json)?;
    
    Ok(())
}

/// Loads scheduled blocks from file
/// 
/// # Arguments
/// 
/// * 'schedule_dir' - the directory to load the file from
/// * 'date_time' - datetime object used to check if the loaded schedule blocks are valid for the given day
pub fn load_schedule_blocks(schedule_dir: &str, date_time: DateTime<Local>) -> Result<Option<Vec<Block>>, BackupError> {
    let file_path = format!("{}schedule.json", schedule_dir);
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