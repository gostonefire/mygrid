use chrono::{DateTime, Datelike, Local, NaiveDate, TimeZone, Timelike};
use crate::{manager_sun};
use crate::models::smhi_forecast::TimeValues;

/// Max expected mean output from PV in watts during one hour
const MAX_PV_POWER: f64 = 6000.0;

/// Min expected mean output from PV in watts during one hour (when sunny)
const MIN_PV_POWER: f64 = 500.0;

/// The factor on how much clouds should impact on the expected (when sunny) PV power output, i.e.
/// when cloudy power is reduced by up to this factor
const CLOUD_IMPACT_FACTOR: f64 = 0.75;

/// Date when summer solstice occurs. Used to figure out max south sun elevation
const SUMMER_SOLSTICE: (u32, u32) = (6, 21);

/// Date when winter solstice occurs. Used to figure out min south sun elevation
const WINTER_SOLSTICE: (u32, u32) = (12, 21);

/// Sun elevation angle from where sunrise time is calculated
const SUNRISE_ANGLE: f64 = 0.0;

/// Sun elevation angle from where sunset time is calculated
const SUNSET_ANGLE: f64 = 0.0;

/// Struct for calculating and holding PV production per hour given a whether forecast
///
/// The implementation includes business logic for the factor between sun elevation and
/// cloud index. This business logic is implemented in the get_production_factor function.
pub struct PVProduction {
    hours: [f64;24],
    lat: f64,
    long: f64,
    pv_diagram: [f64; 1440],
}

impl PVProduction {
    /// Returns a new PVProduction struct with calculated/estimated PV production levels per hour.
    ///
    /// # Arguments
    ///
    /// * 'forecast' - whether forecast including cloud index and temperatures per hour
    /// * 'lat' - latitude for the point where the PV plant is
    /// * 'long' - longitude for the point where the PV plant is
    /// * 'pv_diagram' - a normalized PV power output diagram from sunrise to sunset
    /// * 'date_time' - the date to calculate for
    pub fn new(forecast: &[TimeValues;24], lat: f64, long: f64, pv_diagram: [f64;1440], date_time: DateTime<Local>) -> PVProduction {
        let mut pv_prod = PVProduction { hours: [0.0;24], lat, long, pv_diagram };
        pv_prod.calculate_hour_pv_production(forecast, date_time);

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
    /// * 'date_time' - the date time to calculate for
    fn calculate_hour_pv_production(&mut self, forecast: &[TimeValues;24], date_time: DateTime<Local>) {
        let mut pv_production: [f64;24] = [0.0;24];
        let max_south_elev = self.get_max_sun_elevation(SUMMER_SOLSTICE);
        let min_south_elev = self.get_max_sun_elevation(WINTER_SOLSTICE);
        let (day_south_elev, sunrise, sunset) = self.get_sun_extremes((date_time.month(), date_time.day()));

        let max_day_power = PVProduction::get_max_day_power(day_south_elev, min_south_elev, max_south_elev);
        let factor = 1439.0 / (sunset - sunrise);

        for (h, v) in forecast.iter().enumerate() {
            let cloud_factor = PVProduction::get_cloud_factor(v.cloud);
            let mut start = (v.valid_time.hour() * 60) as f64;
            let mut end = start + 59.0;

            if (end >= sunrise || start >= sunrise) && (start <= sunset || end <= sunset)  {
                let mut border_factor = 1.0;
                if sunrise > start && sunrise <= end {
                    border_factor = (end - sunrise) / (end - start);
                    start = sunrise;
                }
                if sunset < end && sunset >= start {
                    border_factor = (sunset - start) / (end - start);
                    end = sunset;
                }

                //let factor = 1439.0 / (max_azimuth - min_azimuth);
                let start_idx = ((start - sunrise) * factor).round().max(0.0) as usize;
                let end_idx = ((end - sunrise) * factor).round().min(1439.0) as usize;
                let sum = self.pv_diagram[start_idx..end_idx].iter().map(|p| p * max_day_power).sum::<f64>();
                let power = sum / (end_idx - start_idx) as f64 * border_factor * cloud_factor;

                pv_production[h] = power;
            }
        }

        self.hours = pv_production;
    }

    /// Calculates the top sun power production given the sun top elevation for the day
    ///
    /// # Arguments
    ///
    /// * 'day_south_elev' - max elevation for the day we are calculating for
    /// * 'min_south_elev' - min sun south elevation of the year (at winter solstice)
    /// * 'max_south_elev' - max sun south elevation of the year (at summer solstice)
    fn get_max_day_power(day_south_elev: f64, min_south_elev: f64, max_south_elev: f64) -> f64 {
        let sun_top_factor = (day_south_elev - min_south_elev).max(0.0) / (max_south_elev - min_south_elev);
        let sun_top_power = (MAX_PV_POWER - MIN_PV_POWER) * sun_top_factor + MIN_PV_POWER;

        sun_top_power
    }

    /// Calculates the cloud factor given cloud index.
    ///
    /// # Arguments
    ///
    /// * 'cloud_index' - the cloud index given from SMHI (0-5)
    fn get_cloud_factor(cloud_index: f64) -> f64 {
        (5.0 - cloud_index) / 5.0 * CLOUD_IMPACT_FACTOR + (1.0 - CLOUD_IMPACT_FACTOR)
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
            let (elevation, _) = manager_sun::get_elevation_and_azimuth(date_time, self.lat, self.long, declination);
            if elevation > max_elevation {
                max_elevation = elevation;
            }
        }

        max_elevation
    }

    /// Calculates sun extremes (elevation and azimuth) for the given month and day
    ///
    /// # Arguments
    ///
    /// * 'month_day' - tuple containing the month and day
    fn get_sun_extremes(&self, month_day: (u32, u32)) -> (f64, f64, f64) {
        let mut max_elevation: f64 = 0.0;
        let mut min_azimuth: f64 = 360.0;
        let mut max_azimuth: f64 = 0.0;
        let mut sunrise: f64 = 0.0;
        let mut sunset: f64 = 0.0;

        let solstice_day = NaiveDate::from_ymd_opt(Local::now().year(), month_day.0, month_day.1).unwrap();
        for hour in 0..=23 {
            for minute in 0..=59 {
                let date_time = Local.from_local_datetime(&solstice_day.and_hms_opt(hour, minute, 0).unwrap()).unwrap();
                let declination = manager_sun::get_declination(date_time);
                let (elevation, azimuth) = manager_sun::get_elevation_and_azimuth(date_time, self.lat, self.long, declination);
                if elevation > max_elevation {
                    max_elevation = elevation;
                }

                if elevation > SUNRISE_ANGLE && azimuth < min_azimuth {
                    min_azimuth = azimuth;
                    sunrise = (date_time.hour() * 60 + date_time.minute()) as f64;
                }
                if elevation > SUNSET_ANGLE && azimuth > max_azimuth {
                    max_azimuth = azimuth;
                    sunset = (date_time.hour() * 60 + date_time.minute()) as f64;
                }
            }
        }

        (max_elevation, sunrise, sunset)
    }
}