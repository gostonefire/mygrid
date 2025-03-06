use chrono::{Datelike, Local, NaiveDate, TimeZone};
use crate::{manager_sun};
use crate::models::smhi_forecast::TimeValues;

/// Max expected mean output from PV in watts during one hour
const MAX_PV_POWER: f64 = 7000.0;

/// The factor on how much of the sun elevation should contribute to the overall PV output
/// in comparison to cloud index
const PRODUCTION_SUN_FACTOR: f64 = 0.5;

/// Struct for calculating and holding PV production per hour given a whether forecast
///
/// The implementation includes business logic for the factor between sun elevation and
/// cloud index. This business logic is implemented in the get_production_factor function.
pub struct PVProduction {
    hours: [f64;24],
    lat: f64,
    long: f64,
}

impl PVProduction {
    /// Returns a new PVProduction struct with calculated/estimated PV production levels per hour.
    ///
    /// # Arguments
    ///
    /// * 'forecast' - whether forecast including cloud index and temperatures per hour
    /// * 'lat' - latitude for the point where the PV plant is
    /// * 'long' - longitude for the point where the PV plant is
    pub fn new(forecast: &[TimeValues;24], lat: f64, long: f64) -> PVProduction {
        let mut pv_prod = PVProduction { hours: [0.0;24], lat, long };
        pv_prod.calculate_hour_pv_production(forecast);

        pv_prod
    }

    /// Returns the calculated hourly PV production estimates
    pub fn get_production(&self) -> [f64;24] {
        self.hours
    }

    /// Calculates hourly PV production based on cloud forecast and sun elevations
    ///
    /// # Arguments
    ///
    /// * 'forecast' - the cloud forecast
    fn calculate_hour_pv_production(&mut self, forecast: &[TimeValues;24]) {
        let mut pv_production: [f64;24] = [0.0;24];
        let max_elevation = self.get_max_sun_elevations();

        for (h, v) in forecast.iter().enumerate() {
            let declination = manager_sun::get_declination(v.valid_time);
            let elevation = manager_sun::get_elevation(v.valid_time, self.lat, self.long, declination);

            if elevation > 0.0 {
                let production_index = PVProduction::get_production_factor(elevation, max_elevation, v.cloud);
                pv_production[h] = production_index * MAX_PV_POWER;
            }
        }
        self.hours = pv_production;
    }


    /// Calculates the production factor for a specific sun elevation and cloud index.
    ///
    /// If elevation is zero/negative, it succeeds over cloud index and production factor
    /// is determined to 0.0
    ///
    ///
    /// # Arguments
    ///
    /// * 'elevation' - the specific sun elevation to calculate for
    /// * 'max_elevation' - the calculated max sun elevation of the year (at solstice)
    /// * 'cloud_index' - the cloud index given from SMHI (0-8)
    fn get_production_factor(elevation: f64, max_elevation: f64, cloud_index: f64) -> f64 {
        if elevation > 0.0 {
            let sun_index = PRODUCTION_SUN_FACTOR * (elevation / max_elevation);
            let cloud_index = (1.0 - PRODUCTION_SUN_FACTOR) * ((8.0 - cloud_index) / 8.0);

            sun_index + cloud_index
        } else {
            0.0
        }
    }

    /// Calculates max sun elevation over the year, which happens on the day of solstice
    fn get_max_sun_elevations(&self) -> f64 {
        let mut max_elevation = 0.0;

        let solstice_day = NaiveDate::from_ymd_opt(Local::now().year(), 6, 21).unwrap();
        for hour in 0..=23 {
            let date_time = Local.from_local_datetime(&solstice_day.and_hms_opt(hour, 0, 0).unwrap()).unwrap();
            let declination = manager_sun::get_declination(date_time);
            let elevation = manager_sun::get_elevation(date_time, self.lat, self.long, declination);
            if elevation > max_elevation {
                max_elevation = elevation;
            }
        }

        max_elevation
    }
}