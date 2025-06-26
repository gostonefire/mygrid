use std::collections::HashMap;
use chrono::{DateTime, Datelike, DurationRound, Local, NaiveDate, NaiveTime, TimeDelta, TimeZone, Timelike};
use serde::Serialize;
use crate::{manager_sun};
use crate::config::{GeoRef, ProductionParameters};
use crate::errors::SplineError;
use crate::models::smhi_forecast::ForecastValues;
use crate::spline::MonotonicCubicSpline;

#[derive(Clone, Serialize)]
pub struct ProductionValues {
    pub valid_time: DateTime<Local>,
    pub power: f64
}

struct AzimuthFactor {
    am_m: f64,
    am_b: f64,
    pm_m: f64,
    pm_b: f64,
}

/// Struct for calculating and holding PV production per hour given a whether forecast
///
/// The implementation includes business logic for the factor between sun elevation and
/// cloud index. This business logic is implemented in the get_production_factor function.
pub struct PVProduction {
    production: Vec<ProductionValues>,
    production_kw: Vec<ProductionValues>,
    lat: f64,
    long: f64,
    min_pv_power: f64,
    max_pv_power: f64,
    cloud_impact_factor: f64,
    summer_solstice: (u32, u32),
    winter_solstice: (u32, u32),
    sunrise_angle: f64,
    sunset_angle: f64,
    visibility_alt: f64,
    pv_diagram: [f64; 1440],
    azimuth_factor: AzimuthFactor,
}

impl PVProduction {
    /// Returns a new PVProduction struct
    ///
    /// # Arguments
    ///
    /// * 'config' - ProductionParameters configuration struct
    /// * 'location' - GeoRef configuration struct
    pub fn new(config: &ProductionParameters, location: &GeoRef) -> PVProduction {
        let am_m = (1.0 - 0.0) / (100.0 - 0.0);
        let am_b = 0.0;
        let pm_m = (0.0 - 1.0) / (360.0 - 245.0);
        let pm_b = 0.0 - 360.0 * pm_m;
        
        PVProduction { 
            production: Vec::new(),
            production_kw: Vec::new(),
            lat: location.lat, 
            long: location.long,
            min_pv_power: config.min_pv_power,
            max_pv_power: config.max_pv_power,
            cloud_impact_factor: config.cloud_impact_factor,
            summer_solstice: config.summer_solstice,
            winter_solstice: config.winter_solstice,
            sunrise_angle: config.sunrise_angle,
            sunset_angle: config.sunset_angle,
            visibility_alt: config.visibility_alt,
            pv_diagram: config.diagram.unwrap(),
            azimuth_factor: AzimuthFactor { am_m, am_b, pm_m, pm_b },
        }
    }

    /// Calculate and return new hourly PV production estimates
    /// 
    /// # Arguments
    /// 
    /// * 'forecast' - whether forecast including cloud index and temperatures per hour
    pub fn new_estimates(&mut self, forecast: &Vec<ForecastValues>) -> (&Vec<ProductionValues>, &Vec<ProductionValues>) {
        self.calculate_hour_pv_production(forecast);

        (&self.production, &self.production_kw)
    }
    
    /// Calculates hourly PV production based on cloud forecast and sun elevations
    ///
    /// # Arguments
    ///
    /// * 'forecast' - the cloud forecast
    fn calculate_hour_pv_production(&mut self, forecast: &Vec<ForecastValues>) {
        let mut pv_production: Vec<ProductionValues> = Vec::new();
        let mut pv_production_kw: Vec<ProductionValues> = Vec::new();
        let max_south_elev = self.get_max_sun_elevation(self.summer_solstice);
        let min_south_elev = self.get_max_sun_elevation(self.winter_solstice);

        for v in forecast.iter() {
            let (day_south_elev, sunrise, sunset) = self.get_sun_extremes((v.valid_time.month(), v.valid_time.day()));
            let max_day_power = self.get_max_day_power(day_south_elev, min_south_elev, max_south_elev);
            let factor = 1439.0 / (sunset - sunrise);

            let cloud_factor = v.cloud_factor * self.cloud_impact_factor + (1.0 - self.cloud_impact_factor);
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

                let start_idx = ((start - sunrise) * factor).round().max(0.0) as usize;
                let end_idx = ((end - sunrise) * factor).round().min(1439.0) as usize;
                if start_idx != end_idx {
                    let mut sum = 0.0;
                    for (i, p) in self.pv_diagram[start_idx..end_idx].iter().enumerate() {
                        let (vis, dt) = self.visibility(start_idx + i, factor, sunrise, v.valid_time);
                        let power = p * max_day_power * vis;
                        pv_production_kw.push(ProductionValues{ valid_time: dt, power});
                        sum += power;
                    }
                    let kwh = sum / (end_idx - start_idx) as f64 * border_factor * cloud_factor;
                    pv_production.push(ProductionValues{ valid_time: v.valid_time, power: kwh });
                } else {
                    pv_production_kw.push(ProductionValues{ valid_time: v.valid_time, power: 0.0 });
                    pv_production.push(ProductionValues{ valid_time: v.valid_time, power: 0.0 });
                };
            } else {
                pv_production_kw.push(ProductionValues{ valid_time: v.valid_time, power: 0.0 });
                pv_production.push(ProductionValues{ valid_time: v.valid_time, power: 0.0 });
            }
        }

        self.production_kw = self.factor_in_cloud(self.group_on_time(pv_production_kw), forecast).unwrap_or(Vec::new());
        self.production = pv_production;
    }

    /// Factors in cloud factor while using a spline interpolation on the forecast to avoid a 
    /// jagged line in the backup file 
    /// 
    /// # Arguments
    /// 
    /// * 'data' - data to factor in cloud factor on
    /// * 'forecast' - the cloud forecast
    fn factor_in_cloud(&self, data: Vec<ProductionValues>, forecast: &Vec<ForecastValues>) -> Result<Vec<ProductionValues>, SplineError> {
        let x = forecast.iter().map(|f| f.valid_time.timestamp() as f64).collect::<Vec<f64>>();
        let y = forecast.iter().map(|c| c.cloud_factor * self.cloud_impact_factor + (1.0 - self.cloud_impact_factor)).collect::<Vec<f64>>();
        let spline = MonotonicCubicSpline::new(&x, &y)?;
        
        let r = data.iter().map(|p| {
            ProductionValues{ 
                valid_time: p.valid_time, 
                power: p.power * spline.interpolate(p.valid_time.timestamp() as f64).max(0.0).min(1.0), 
            }
        }).collect::<Vec<ProductionValues>>();
        
        Ok(r)
    }
    
    /// Returns a grouped version of the data input
    /// Data is grouped by every 5 minutes and the group function is average
    /// 
    /// # Arguments
    /// 
    /// * 'data' - data to be grouped
    fn group_on_time(&self, data: Vec<ProductionValues>) -> Vec<ProductionValues> {
        let mut map: HashMap<DateTime<Local>, (f64, f64)> = HashMap::new();

        for d in data {
            let _ = map
                .entry(d.valid_time.duration_trunc(TimeDelta::minutes(5)).unwrap())
                .and_modify(|v|{v.0 += d.power; v.1 += 1.0;})
                .or_insert((d.power, 1.0));
        }

        let mut result = map
            .into_iter()
            .map(|(d, v)| ProductionValues{ valid_time: d, power: v.0 / v.1 })
            .collect::<Vec<ProductionValues>>();
        result.sort_by(|a, b| a.valid_time.cmp(&b.valid_time));
        
        result
    }
    
    /// Returns visibility factor when sun is behind neighbour houses and also considers
    /// an approximately 10 minutes for sun to go from obscured to visible
    /// 
    /// Also, the visibility takes into account that far off sun azimuth in relation to PV head on does 
    /// have a negative impact on power generation. This is mostly a factor in the morning and afternoon.
    /// 
    /// # Arguments
    /// 
    /// * 'idx' - the current index in the pv_diagram
    /// * 'factor' - factor between diagram and full day
    /// * 'sunrise' - sunrise in minutes since midnight
    /// * 'date' - date to calculate for (only date part is used from the DateTime object)
    fn visibility(&self, idx: usize, factor: f64, sunrise: f64, date: DateTime<Local>) -> (f64, DateTime<Local>) {
        let vis_start = self.visibility_alt;
        let vis_done = self.visibility_alt + 2.0;
        
        let second_of_day = ((idx as f64 / factor + sunrise) * 60.0).round() as u32;
        let date_time = date.with_time(NaiveTime::from_num_seconds_from_midnight_opt(second_of_day, 0).unwrap()).unwrap();
        let (alt, azi) = manager_sun::get_elevation_and_azimuth(date_time, self.lat, self.long);
        
        let v_factor = if azi < 180.0 {
            let azf = (azi * self.azimuth_factor.am_m + self.azimuth_factor.am_b).min(1.0);
            let obf = if alt < vis_start {
                // when obscured by surroundings
                0.15
            } else if alt >= vis_start && alt <= vis_done {
                // approximately 10 minutes in azimuth for sun to be non-obscured by surroundings
                1.0 - (vis_done - alt) * 0.425
            } else {
                1.0
            };

            azf * obf
        } else {
            (azi * self.azimuth_factor.pm_m + self.azimuth_factor.pm_b).min(1.0)
        };

        (v_factor, date_time)
    }
    
    /// Calculates the top sun power production given the sun top elevation for the day
    ///
    /// # Arguments
    ///
    /// * 'day_south_elev' - max elevation for the day we are calculating for
    /// * 'min_south_elev' - min sun south elevation of the year (at winter solstice)
    /// * 'max_south_elev' - max sun south elevation of the year (at summer solstice)
    fn get_max_day_power(&self, day_south_elev: f64, min_south_elev: f64, max_south_elev: f64) -> f64 {
        let sun_top_factor = (day_south_elev - min_south_elev).max(0.0) / (max_south_elev - min_south_elev);
        let sun_top_power = (self.max_pv_power - self.min_pv_power) * sun_top_factor + self.min_pv_power;

        sun_top_power
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
            let (elevation, _) = manager_sun::get_elevation_and_azimuth(date_time, self.lat, self.long);
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
                let (elevation, azimuth) = manager_sun::get_elevation_and_azimuth(date_time, self.lat, self.long);
                if elevation > max_elevation {
                    max_elevation = elevation;
                }

                if elevation > self.sunrise_angle && azimuth < min_azimuth {
                    min_azimuth = azimuth;
                    sunrise = (date_time.hour() * 60 + date_time.minute()) as f64;
                }
                if elevation > self.sunset_angle && azimuth > max_azimuth {
                    max_azimuth = azimuth;
                    sunset = (date_time.hour() * 60 + date_time.minute()) as f64;
                }
            }
        }

        (max_elevation, sunrise, sunset)
    }
}