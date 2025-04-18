use chrono::{DateTime, Datelike, Local, Timelike};
use serde::Serialize;
use crate::config::ConsumptionParameters;
use crate::models::smhi_forecast::ForecastValues;

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
    min_avg_load: f64,
    max_avg_load: f64,
    diagram: [[f64;24];7],
}

impl Consumption {
    /// Returns a new Consumption struct
    ///
    /// # Arguments
    ///
    /// * 'config' - configuration struct
    pub fn new(config: &ConsumptionParameters) -> Consumption {
        Consumption { 
            consumption: Vec::new(),
            min_avg_load: config.min_avg_load,
            max_avg_load: config.max_avg_load,
            diagram: config.diagram.unwrap(),
        }
    }
    
    /// Calculate and return new hourly consumption estimates
    /// 
    /// # Arguments
    /// 
    /// * 'forecast' - whether forecast from SMHI
    pub fn new_estimates(&mut self, forecast: &Vec<ForecastValues>) -> &Vec<ConsumptionValues> {
        self.calculate_consumption(forecast);
        
        &self.consumption
    }

    /// Calculates hourly household consumption based on temperature forecast
    ///
    /// # Arguments
    ///
    /// * 'forecast' - the temperature forecast
    fn calculate_consumption(&mut self, forecast: &Vec<ForecastValues>) {
        let mut hour_load: Vec<ConsumptionValues> = Vec::new();

        for v in forecast.iter() {
            let week_day = v.valid_time.weekday().num_days_from_monday() as usize;
            let hour = v.valid_time.hour() as usize;
            let power = self.consumption_curve(v.temp) + self.diagram[week_day][hour];
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
    fn consumption_curve(&self, temp: f64) -> f64 {
        let capped_temp = temp.max(-4.0).min(20.0);
        let factor = 8.0 * 3.0f64.sqrt() - 8.0;
        let curve = 2.0 / (capped_temp + factor) - 2.0 / ( 20.0 + factor);

        curve * (self.max_avg_load - self.min_avg_load) + self.min_avg_load
    }
}


