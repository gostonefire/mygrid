use chrono::{DateTime, Datelike, Local, Timelike};
use serde::Serialize;
use crate::config::ConsumptionParameters;
use crate::models::forecast::ForecastValues;
use crate::spline::MonotonicCubicSpline;

#[derive(Clone, Serialize)]
pub struct ConsumptionValues {
    pub valid_time: DateTime<Local>,
    pub power: f64
}

/// Struct for calculating and holding Consumption load per hour given a weather forecast
///
/// The business logic is implemented in the calculate_consumption function. The current version is
/// just an inverse linear proportion between temperature and estimated load.
pub struct Consumption {
    consumption: Vec<ConsumptionValues>,
    min_avg_load: f64,
    max_avg_load: f64,
    diagram: [[f64;24];7],
    curve_x_min: f64,
    curve_x_max: f64,
    curve: MonotonicCubicSpline,
}

impl Consumption {
    /// Returns a new Consumption struct
    ///
    /// # Arguments
    ///
    /// * 'config' - configuration struct
    pub fn new(config: &ConsumptionParameters) -> Consumption {
        let (curve_x, curve_y): (Vec<f64>, Vec<f64>) = config.curve
            .iter()
            .map(|c| (c.0, c.1))
            .unzip();

        Consumption { 
            consumption: Vec::new(),
            min_avg_load: config.min_avg_load,
            max_avg_load: config.max_avg_load,
            diagram: config.diagram.unwrap(),
            curve_x_min: curve_x[0],
            curve_x_max: curve_x[curve_x.len() - 1],
            curve: MonotonicCubicSpline::new(&curve_x, &curve_y)
                .expect("Failed to create consumption curve"),
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

    /// Calculates consumption based on temperature over an estimated curve.
    /// The curve is formed such that it gives an approximation for house consumption within
    /// an outdoor temperature range. It is assumed that temperatures outside that range
    /// don't change much on the consumption in the climate of southern Sweden.
    ///
    /// Output varies between MAX_AVG_LOAD and MIN_AVG_LOAD
    ///
    /// # Arguments
    ///
    /// * 'temp' - outside temperature
    fn consumption_curve(&self, temp: f64) -> f64 {
        let capped_temp = temp.max(self.curve_x_min).min(self.curve_x_max);
        let curve = self.curve.interpolate(capped_temp).clamp(0.0, 1.0);

        curve * (self.max_avg_load - self.min_avg_load) + self.min_avg_load
    }
}


