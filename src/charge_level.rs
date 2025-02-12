use std::collections::HashMap;
use chrono::{DateTime, Datelike, Local, NaiveDate, TimeZone, Timelike};
use crate::{manager_smhi, manager_sun, LAT, LONG};

/// Get charge level for a given day and selected hours
///
/// # Arguments
///
/// * 'date_time' - the date (time is not used) to get charge level for
/// * 'selected_hours' - hours of the date to include in calculation
pub fn get_charge_level(date_time: DateTime<Local>, selected_hours: Vec<u32>) -> Result<u32, String> {
    let max_soc: [u32;11] = [50, 55, 60, 65, 70, 75, 80, 85, 90, 95, 100];
    let charge_indexes = get_hour_sun_indexes(date_time)?;

    let segment = charge_indexes
        .iter()
        .filter(|(k, _)| selected_hours.contains(*k))
        .map(|v| *v.1)
        .sum::<f64>();

    let segment_index = ((10.0  - segment * 10.0).round() as i32)
        .max(0)
        .min(10) as usize;

    Ok(max_soc[segment_index])
}

/// Calculates hourly sun indexes based on cloud forecast and sun elevations
///
/// # Arguments
///
/// * 'date_time' - the date time to get forecast and indexes for
fn get_hour_sun_indexes(date_time: DateTime<Local>) -> Result<HashMap<u32, f64>, String> {
    let mut sun_indexes: HashMap<u32, f64> = HashMap::new();
    let (max_elevations, max_elevation) = get_max_sun_elevations();
    let norm_value = get_charge_norm_value(max_elevations, max_elevation);

    let smhi = manager_smhi::SMHI::new(LAT, LONG);
    let forecast = smhi.get_cloud_forecast(date_time)?;
    for h in forecast.time_series {
        let declination = manager_sun::get_declination(h.valid_time);
        let elevation = manager_sun::get_elevation(h.valid_time, LAT, LONG, declination);

        if elevation > 0.0 {
            let charge_index = (elevation / max_elevation + (8.0 - h.parameters.value) / 8.0) / 2.0;
            sun_indexes.insert(h.valid_time.hour(), charge_index / norm_value);
        } else {
            sun_indexes.insert(h.valid_time.hour(), 0.0);
        }
    }

    Ok(sun_indexes)
}

/// Calculates the charge norm value for normalizing the sum of hourly day charge indexes
/// to a value between 0 and 1
fn get_charge_norm_value(max_elevations: HashMap<u32, f64>, max_elevation: f64) -> f64 {
    let mut norm_value = 0.0;
    for hour in 0..=23 {
        let elevation = *max_elevations.get(&hour).unwrap();
        norm_value += (elevation / max_elevation + 1.0) / 2.0;
    }

    norm_value
}

/// Calculates a map over sun elevations on the day of solstice
///
fn get_max_sun_elevations() -> (HashMap<u32, f64>, f64) {
    let mut max_elevation = 0.0;
    let mut max_sun_elevations: HashMap<u32, f64> = HashMap::new();

    let solstice_day = NaiveDate::from_ymd_opt(Local::now().year(), 6, 21).unwrap();
    for hour in 0..=23 {
        let date_time = Local.from_local_datetime(&solstice_day.and_hms_opt(hour, 0, 0).unwrap()).unwrap();
        let declination = manager_sun::get_declination(date_time);
        let elevation = manager_sun::get_elevation(date_time, LAT, LONG, declination);
        if elevation > max_elevation {
            max_elevation = elevation;
        }
        max_sun_elevations.insert(hour, elevation);
    }

    (max_sun_elevations, max_elevation)
}