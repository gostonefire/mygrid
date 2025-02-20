use std::collections::HashMap;
use chrono::{Datelike, Local, NaiveDate, TimeZone, Timelike};
use crate::{manager_sun, LAT, LONG};
use crate::models::smhi_forecast::{TimeValues};

const MAX_PV_W: f64 = 6000.0;
const SOC_CAPACITY_W: f64 = 16590.0 / 100.0;

/// Get charge level for a given day and selected hours
///
/// # Arguments
///
/// * 'selected_hours' - hours of the date to include in calculation
/// * 'forecast' - whether forecast including temperatures and cloud indexes
pub fn get_charge_level(selected_hours: Vec<usize>, forecast: &[TimeValues;24]) -> f64 {
    let pv_production = get_hour_pv_production(forecast);
    let hour_load = get_hour_load(forecast);

    let segment = pv_production
        .iter().enumerate()
        .filter(|(h, _)| selected_hours.contains(h))
        .map(|(h, &p)| calculate_spare_capacity(p, hour_load[h]))
        .sum::<f64>();

    (100.0 - segment / SOC_CAPACITY_W).floor()

}

/// Calculates what spare capacity in watts that is needed to cover for either irregularities
/// in the load when load is greater than production, or room needed when production is greater
/// than load.
///
/// # Arguments
///
/// * 'production' - production in watts from PV
/// * 'load' - the household load in watts (i.e. not grid consumption which may include battery charging)
fn calculate_spare_capacity(production: f64, load: f64) -> f64 {
    let diff = production - load;
    if diff < 0.0 {
        diff.abs() / 2.0
    } else {
        diff
    }
}

/// Calculates hourly household load based on temperature forecast
/// The forecasted load is approximated as (right exclusive and in Wh/h):
/// *    ->  0: 3000
/// *  0 ->  5: 2500
/// *  5 -> 10: 2000
/// * 10 -> 15: 1500
/// * 15 -> 20: 1000
/// * 20 ->   : 500
///
/// # Arguments
///
/// * 'forecast' - the temperature forecast
fn get_hour_load(forecast: &[TimeValues;24]) -> [f64;24] {
    let mean_load: [f64;6] = [3000.0, 2500.0, 2000.0, 1500.0, 1000.0, 500.0];
    let mut hour_load: [f64;24] = [0.0;24];

    for (h, v) in forecast.iter().enumerate() {
        let load = ((v.temp + 0.1) / 5.0).ceil().max(0.0).min(5.0) as usize;
        hour_load[h] = mean_load[load];
    }

    hour_load
}

/// Calculates hourly PV production based on cloud forecast and sun elevations
///
/// # Arguments
///
/// * 'forecast' - the cloud forecast
fn get_hour_pv_production(forecast: &[TimeValues;24]) -> [f64;24] {
    let mut pv_production: [f64;24] = [0.0;24];
    //let mut pv_production: HashMap<u32, f64> = HashMap::new();
    let (_, max_elevation) = get_max_sun_elevations();

    for (h, v) in forecast.iter().enumerate() {
        let declination = manager_sun::get_declination(v.valid_time);
        let elevation = manager_sun::get_elevation(v.valid_time, LAT, LONG, declination);

        if elevation > 0.0 {
            let charge_index = get_charge_index(elevation, max_elevation, v.cloud);
            pv_production[h] = charge_index * MAX_PV_W;
        }
    }

    pv_production
}

/*
/// Calculates hourly sun indexes based on cloud forecast and sun elevations
///
/// # Arguments
///
/// * 'forecast' - the cloud forecast of the given date time
fn get_hour_sun_indexes(forecast: &Forecast) -> Result<HashMap<u32, f64>, String> {
    let mut sun_indexes: HashMap<u32, f64> = HashMap::new();
    let (max_elevations, max_elevation) = get_max_sun_elevations();
    let norm_value = get_charge_norm_value(max_elevations, max_elevation);

    //let smhi = manager_smhi::SMHI::new(LAT, LONG);
    //let forecast = smhi.get_cloud_forecast(date_time)?;
    for h in &forecast.time_series {
        let declination = manager_sun::get_declination(h.valid_time);
        let elevation = manager_sun::get_elevation(h.valid_time, LAT, LONG, declination);

        if elevation > 0.0 {
            let charge_index = get_charge_index(elevation, max_elevation, h.parameters.value);
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
        norm_value += get_charge_index(elevation, max_elevation, 0.0);
    }

    norm_value
}
*/

/// Calculates the charge index for a specific sun elevation and cloud index.
/// This function holds the business rule that is used both for daily
/// calculations and for the calculation of the normalization value.
///
/// If elevation is zero/negative, it succeeds over cloud index and charge index
/// is determined to 0.0
///
///
/// # Arguments
///
/// * 'elevation' - the specific sun elevation to calculate for
/// * 'max_elevation' - the calculated max sun elevation of the year (at solstice)
/// * 'cloud_index' - the cloud index given from SMHI (0-8)
fn get_charge_index(elevation: f64, max_elevation: f64, cloud_index: f64) -> f64 {
    if elevation > 0.0 {
        0.9764 * (elevation / max_elevation) + 0.1236 * ((8.0 - cloud_index) / 8.0)
    } else {
        0.0
    }
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