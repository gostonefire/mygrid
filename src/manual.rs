use std::path::Path;
use chrono::{DateTime, Local, NaiveDate, Utc};
use serde::Deserialize;
use anyhow::Result;
use crate::errors::SkipError;
use crate::MANUAL_DAY;

#[derive(Deserialize)]
struct ManualDates {
    dates: Vec<NaiveDate>,
}

/// Checks whether there is any manual file and if so read manual dates.
/// After that it determines whether to go in to manual mode or not.
///
/// The return value depends on the manual mode when entering the function,
/// if manual mode changes then it returns Some(true/false) otherwise it returns None
///
///
/// # Arguments
///
/// * 'manual_file' - the file holding any dates indicating going manual
/// * 'date' - date to check if to set manual mode
pub fn check_manual(manual_file: &str, date: DateTime<Utc>) -> Result<Option<bool>, SkipError> {
    let was_manual = *MANUAL_DAY.read()?;

    let path = Path::new(manual_file);
    if path.exists() {
        let date = date.with_timezone(&Local).date_naive();
        let json = std::fs::read_to_string(&path)?;
        let manual: ManualDates = serde_json::from_str(&json)?;

        if manual.dates.contains(&date) {
            *MANUAL_DAY.write()? = true;
            return if !was_manual { Ok(Some(true)) } else { Ok(None) }
        }
    }

    *MANUAL_DAY.write()? = false;
    if was_manual { Ok(Some(false)) } else { Ok(None) }
}