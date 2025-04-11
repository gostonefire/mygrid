use crate::models::smhi_forecast::TimeValues;

/// Min average consumption/load in watts over an hour
const MIN_AVG_LOAD: f64 = 200.0;

/// Max average consumption/load in watts over an hour
const MAX_AVG_LOAD: f64 = 3000.0;

/// Struct for calculating and holding Consumption load per hour given a whether forecast
///
/// The business logic is implemented in the calculate_consumption function. Current version is
/// just an inverse linear proportion between temperature and estimated load.
pub struct Consumption {
    hours: [f64;24],
}
impl Consumption {
    /// Returns a new Consumption struct with calculated/estimated load levels per hour.
    ///
    /// # Arguments
    ///
    /// * 'forecast' - whether forecast including temperatures per hour
    /// * 'consumption_diagram' - daily household consumption not considering house heating
    /// * 'week_day' - day of week to create estimated consumption for
    pub fn new(forecast: &[TimeValues;24], consumption_diagram: [[f64;24];7], week_day: u32) -> Consumption {
        let mut consumption = Consumption { hours: [0.0;24] };
        consumption.calculate_consumption(forecast, consumption_diagram[week_day as usize]);

        consumption
    }

    /// Return the hourly calculated consumption estimates
    pub fn get_consumption(&self) -> [f64;24] {
        self.hours
    }

    /// Calculates hourly household consumption based on temperature forecast
    ///
    /// The consumption goes down quite drastically with warmer whether so some inverse exponential
    /// or inverse power of X is probably going to be pretty close
    ///
    /// # Arguments
    ///
    /// * 'forecast' - the temperature forecast
    /// * 'day_consumption' - estimated household consumption for the day to calculate
    fn calculate_consumption(&mut self, forecast: &[TimeValues;24], day_consumption: [f64;24]) {
        let mut hour_load: [f64;24] = [0.0;24];

        for (h, v) in forecast.iter().enumerate() {
            let temp = v.temp.max(0.0).min(20.0);
            let load_factor = (20.0 - temp).powi(5) / 20.0_f64.powi(5);
            let load = load_factor * (MAX_AVG_LOAD - MIN_AVG_LOAD) + MIN_AVG_LOAD;
            hour_load[h] = load + day_consumption[h];
        }

        self.hours = hour_load;
    }
}


