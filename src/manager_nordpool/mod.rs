pub mod errors;

use std::ops::Add;
use std::time::Duration;
use chrono::{DateTime, Local, TimeDelta, Timelike};
use ureq::Agent;
use crate::manager_nordpool::errors::NordPoolError;
use crate::models::nordpool_tariffs::Tariffs;


pub struct NordPool {
    agent: Agent,
}

impl NordPool {
    pub fn new() -> NordPool {
        let config = Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .build();

        let agent = config.into();

        Self { agent }
    }

    /// Retrieves day ahead prices from NordPool
    /// It gets the tariffs for the day indicated by date_time (if it can't an error will be returned),
    /// then it tries to get also next days tariffs and if successful those 24 tariffs are added
    /// also added to the result.
    ///
    /// # Arguments
    ///
    /// * 'date_time' - the date to retrieve prices for
    pub fn get_tariffs(&self, date_time: DateTime<Local>) -> Result<Vec<f64>, NordPoolError> {
        let mut result = self.get_day_tariffs(date_time)?;
        if let Ok(next_day) = self.get_day_tariffs(date_time.add(TimeDelta::days(1))) {
            result.extend(next_day);
        }

        Ok(result)
    }

    /// Retrieves day ahead prices from NordPool
    ///
    /// # Arguments
    ///
    /// * 'date_time' - the date to retrieve prices for
    fn get_day_tariffs(&self, date_time: DateTime<Local>) -> Result<Vec<f64>, NordPoolError> {
        let url = "https://dataportal-api.nordpoolgroup.com/api/DayAheadPrices";
        let date = format!("{}", date_time.format("%Y-%m-%d"));
        let query = vec![
            ("date", date.as_str()),
            ("market", "DayAhead"),
            ("deliveryArea", "SE4"),
            ("currency", "SEK"),
        ];

        let mut response = self.agent
            .get(url)
            .query_pairs(query)
            .call()?;

        if response.status() == 204 {
            return Err(NordPoolError::NoContent);
        }

        let json = response
            .body_mut()
            .read_to_string()?;

        let tariffs: Tariffs = serde_json::from_str(&json)?;

        NordPool::tariffs_to_vec(&tariffs)
    }

    /// Transforms the Tariffs struct to a plain vector of prices
    ///
    /// # Arguments
    ///
    /// * 'tariffs' - the struct containing prices
    fn tariffs_to_vec(tariffs: &Tariffs) -> Result<Vec<f64>, NordPoolError> {
        if tariffs.multi_area_entries.len() != 24 {
            return Err(NordPoolError::from("number of day tariffs not equal to 24"))
        }

        let mut result: Vec<f64> = vec![0.0;24];
        tariffs.multi_area_entries.iter().for_each(
            |t| {
                let sek_per_kwh = (t.entry_per_area.se4 / 10f64).round() / 100f64;
                result[t.delivery_start.hour() as usize] = sek_per_kwh;
            });

        Ok(result)
    }
}