pub mod errors;

use std::time::Duration;
use chrono::{DateTime, Local, Timelike};
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
    ///
    /// # Arguments
    ///
    /// * 'date_time' - the date to retrieve prices for
    pub fn get_tariffs(&self, date_time: DateTime<Local>) -> Result<Vec<f64>, NordPoolError> {
        let url = "https://dataportal-api.nordpoolgroup.com/api/DayAheadPrices";
        let date = format!("{}", date_time.format("%Y-%m-%d"));
        let query = vec![
            ("date", date.as_str()),
            ("market", "DayAhead"),
            ("deliveryArea", "SE4"),
            ("currency", "SEK"),
        ];

        let json = self.agent
            .get(url)
            .query_pairs(query)
            .call()?
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