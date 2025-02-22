use crate::consumption::Consumption;
use crate::production::PVProduction;

/// The battery capacity in watts divided by the max SoC (State of Charge). This represents
/// roughly how much each percentage of the SoC is in terms of power (Wh)
const SOC_CAPACITY_W: f64 = 16590.0 / 100.0;

/// Get charge level for a given day and selected hours
///
/// # Arguments
///
/// * 'selected_hours' - hours of the date to include in calculation
/// * 'production' - struct containing estimated hourly production levels
/// * 'consumption' - struct containing estimated hourly load levels
pub fn get_charge_level(selected_hours: Vec<usize>, production: &PVProduction, consumption: &Consumption) -> f64 {
    let segment = production.get_production()
        .iter().enumerate()
        .filter(|(h, _)| selected_hours.contains(h))
        .map(|(h, &p)|
            calculate_spare_capacity(p, consumption.get_consumption(h), consumption.get_min_avg_load())
        )
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
/// * 'min_avg_load' - min average consumption/load in watts over an hour
fn calculate_spare_capacity(production: f64, load: f64, min_avg_load: f64) -> f64 {
    let diff = production - load;
    if diff < 0.0 {
        println!("Under: {}, {}: {}", production, load, (production - min_avg_load).max(0.0) / 2.0);
        (production - min_avg_load).max(0.0) / 2.0
    } else {
        println!("Over:  {}, {}: {}", production, load, diff);
        diff
    }
}

/*
/// Calculates hourly household load based on temperature forecast
///
/// The load is linearly calculated between MAX_AVG_LOAD and MIN_AVG_LOAD where
/// temperature is truncated to the range 0..=20
///
/// # Arguments
///
/// * 'forecast' - the temperature forecast
fn get_hour_load(forecast: &[TimeValues;24]) -> [f64;24] {
    let mut hour_load: [f64;24] = [0.0;24];

    for (h, v) in forecast.iter().enumerate() {
        let load_factor = 1.0 - v.temp.max(0.0).min(20.0) / 20.0;
        let load = load_factor * (MAX_AVG_LOAD - MIN_AVG_LOAD) + MIN_AVG_LOAD;
        hour_load[h] = load;
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
        let sun_index = CHARGE_INDEX_SUN_FACTOR * (elevation / max_elevation);
        let cloud_index = (1.0 - CHARGE_INDEX_SUN_FACTOR) * ((8.0 - cloud_index) / 8.0);

        sun_index + cloud_index
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

 */