use std::fmt;
use std::fmt::Formatter;
use chrono::{DateTime, Local, Timelike};
use reqwest::blocking::{Client};
use reqwest::{StatusCode};
use crate::models::nordpool_tariffs::Tariffs;

pub enum NordPoolError {
    NordPool(String),
    Document(String),
}

impl fmt::Display for NordPoolError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            NordPoolError::NordPool(e) => write!(f, "NordPoolError::NordPool: {}", e),
            NordPoolError::Document(e) => write!(f, "NordPoolError::Document: {}", e),
        }
    }
}
impl From<&str> for NordPoolError {
    fn from(e: &str) -> Self {
        NordPoolError::NordPool(e.to_string())
    }
}
impl From<reqwest::Error> for NordPoolError {
    fn from(e: reqwest::Error) -> Self {
        NordPoolError::NordPool(e.to_string())
    }
}
impl From<serde_json::Error> for NordPoolError {
    fn from(e: serde_json::Error) -> Self {
        NordPoolError::Document(e.to_string())
    }
}

pub struct NordPool {
    client: Client,
}

impl NordPool {
    pub fn new() -> NordPool {
        let client = Client::new();
        NordPool { client }
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

        let res = self.client
            .get(url)
            .query(&query)
            .send()?;

        if res.status() != StatusCode::OK {
            return Err(NordPoolError::NordPool(res.status().to_string()));
        }

        let json = res.text()?;

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