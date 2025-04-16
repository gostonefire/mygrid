use chrono::{DateTime, Datelike, Local, Timelike};
use serde::Serialize;
use crate::models::smhi_forecast::ForecastValues;

/// Min average consumption/load in watts over an hour
const MIN_AVG_LOAD: f64 = 200.0;

/// Max average consumption/load in watts over an hour
const MAX_AVG_LOAD: f64 = 2500.0;

#[derive(Clone, Serialize)]
pub struct ConsumptionValues {
    pub valid_time: DateTime<Local>,
    pub power: f64
}

/// Struct for calculating and holding Consumption load per hour given a whether forecast
///
/// The business logic is implemented in the calculate_consumption function. Current version is
/// just an inverse linear proportion between temperature and estimated load.
pub struct Consumption {
    consumption: Vec<ConsumptionValues>,
}
impl Consumption {
    /// Returns a new Consumption struct with calculated/estimated load levels per hour.
    ///
    /// # Arguments
    ///
    /// * 'forecast' - whether forecast including temperatures per hour
    /// * 'consumption_diagram' - daily household consumption not considering house heating
    /// * 'week_day' - day of week to create estimated consumption for
    pub fn new(forecast: &Vec<ForecastValues>, consumption_diagram: [[f64;24];7]) -> Consumption {
        let mut consumption = Consumption { consumption: Vec::new() };
        consumption.calculate_consumption(forecast, consumption_diagram);

        consumption
    }

    /// Return the hourly calculated consumption estimates
    pub fn get_consumption(&self) -> &Vec<ConsumptionValues> {
        &self.consumption
    }

    /// Calculates hourly household consumption based on temperature forecast
    ///
    /// The consumption goes down quite drastically with warmer whether so some inverse exponential
    /// or inverse power of X is probably going to be pretty close
    ///
    /// # Arguments
    ///
    /// * 'forecast' - the temperature forecast
    ///  * 'consumption_diagram' - daily household consumption not considering house heating
    fn calculate_consumption(&mut self, forecast: &Vec<ForecastValues>, consumption_diagram: [[f64;24];7]) {
        let mut hour_load: Vec<ConsumptionValues> = Vec::new();

        for v in forecast.iter() {
            let week_day = v.valid_time.weekday().num_days_from_monday() as usize;
            let hour = v.valid_time.hour() as usize;
            let power = Consumption::consumption_curve(v.temp) + consumption_diagram[week_day][hour];
            hour_load.push(ConsumptionValues { valid_time: v.valid_time, power });
        }

        self.consumption = hour_load;
    }

    /// Calculates consumption based on temperature over a multiplicative inverse (1/x) curve.
    /// The curve is formed such that it gives an approximation for a house consumption between
    /// outside temperatures from -4 to 20. It is assumed that temperatures outside that range
    /// doesn't change much on the consumption in the climate of southern Sweden.
    ///
    /// The factor is calculated such that the curve function is equal to 1 at X = -4 and 0 (zero)
    /// at X = 20.
    ///
    /// Output thus varies between MAX_AVG_LOAD and MIN_AVG_LOAD
    ///
    /// # Arguments
    ///
    /// * 'temp' - outside temperature
    fn consumption_curve(temp: f64) -> f64 {
        let capped_temp = temp.max(-4.0).min(20.0);
        let factor = 8.0 * 3.0f64.sqrt() - 8.0;
        let curve = 2.0 / (capped_temp + factor) - 2.0 / ( 20.0 + factor);

        curve * (MAX_AVG_LOAD - MIN_AVG_LOAD) + MIN_AVG_LOAD
    }
}


