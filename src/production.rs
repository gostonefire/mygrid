use chrono::{Datelike, Local, NaiveDate, TimeZone};
use crate::{manager_sun};
use crate::models::smhi_forecast::TimeValues;

/// Max expected mean output from PV in watts during one hour
const MAX_PV_POWER: f64 = 6000.0;

/// Min expected mean output from PV in watts during one hour (when sunny)
const MIN_PV_POWER: f64 = 1000.0;

/// The factor on how much clouds should impact on the expected (when sunny) PV power output, i.e.
/// when cloudy power is reduced by up to this factor
const CLOUD_IMPACT_FACTOR: f64 = 0.75;

/// Date when summer solstice occurs. Used to figure out max south sun elevation
const SUMMER_SOLSTICE: (u32, u32) = (6, 21);

/// Date when winter solstice occurs. Used to figure out min south sun elevation
const WINTER_SOLSTICE: (u32, u32) = (12, 21);

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
        let local_now = Local::now();
        let max_south_elev = self.get_max_sun_elevation(SUMMER_SOLSTICE);
        let min_south_elev = self.get_max_sun_elevation(WINTER_SOLSTICE);
        let day_south_elev = self.get_max_sun_elevation((local_now.month(), local_now.day()));

        for (h, v) in forecast.iter().enumerate() {
            let declination = manager_sun::get_declination(v.valid_time);
            let elevation = manager_sun::get_elevation(v.valid_time, self.lat, self.long, declination);

            if elevation > 0.0 {
                let power = PVProduction::get_production_factor(elevation, day_south_elev, min_south_elev, max_south_elev, v.cloud);
                pv_production[h] = power;
                //pv_production[h] = production_index * MAX_PV_POWER;
            }
        }
        self.hours = pv_production;
        println!("prod: {:?}", self.hours);
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
    /// * 'day_south_elev' - max elevation for then day we are calculating for
    /// * 'min_south_elev' - min sun south elevation of the year (at winter solstice)
    /// * 'max_south_elev' - max sun south elevation of the year (at summer solstice)
    /// * 'cloud_index' - the cloud index given from SMHI (0-8)
    fn get_production_factor(elevation: f64, day_south_elev: f64, min_south_elev: f64, max_south_elev: f64, cloud_index: f64) -> f64 {
        if elevation > 0.0 {
            let sun_top_factor = (day_south_elev - min_south_elev).max(0.0) / (max_south_elev - min_south_elev);
            let sun_top_power = (MAX_PV_POWER - MIN_PV_POWER) * sun_top_factor + MIN_PV_POWER;

            let sun_day_factor = elevation / day_south_elev;
            let sun_power = sun_top_power * sun_day_factor;

            let cloud_factor = (8.0 - cloud_index) / 8.0 * CLOUD_IMPACT_FACTOR + (1.0 - CLOUD_IMPACT_FACTOR);

            sun_power * cloud_factor
        } else {
            0.0
        }
    }

    /// Calculates max sun elevation for the given month and day in the current year
    ///
    /// # Arguments
    ///
    /// * 'month_day' - tuple containing the month and day
    fn get_max_sun_elevation(&self, month_day: (u32, u32)) -> f64 {
        let mut max_elevation = 0.0;

        let solstice_day = NaiveDate::from_ymd_opt(Local::now().year(), month_day.0, month_day.1).unwrap();
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