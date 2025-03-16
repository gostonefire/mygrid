use std::path::Path;
use chrono::{DateTime, Local, NaiveDate};
use serde::Deserialize;
use crate::errors::SkipError;
use crate::SKIP_DAY;

#[derive(Deserialize)]
struct SkipDates {
    dates: Vec<NaiveDate>,
}

/// Checks whether there is any skip file and if so read skip dates.
/// After that it determines whether to go in to skip mode or not.
///
/// The return value depends on the skip mode when entering the function,
/// if skip mode changes then it returns Some(true/false) otherwise it returns None
///
///
/// # Arguments
///
/// * 'date' - date to check if to set skip mode
pub fn check_skip(skip_file: &str, date: DateTime<Local>) -> Result<Option<bool>, SkipError> {
    let was_skip = *SKIP_DAY.read()?;

    let path = Path::new(skip_file);
    if path.exists() {
        let date = date.date_naive();
        let json = std::fs::read_to_string(&path)?;
        let skip: SkipDates = serde_json::from_str(&json)?;

        if skip.dates.contains(&date) {
            *SKIP_DAY.write()? = true;
            return if !was_skip { Ok(Some(true)) } else { Ok(None) }
        }
    }

    *SKIP_DAY.write()? = false;
    if was_skip { Ok(Some(false)) } else { Ok(None) }
}