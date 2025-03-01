use chrono::{DateTime, Local, Timelike};
use reqwest::blocking::{Client};
use reqwest::StatusCode;
use crate::models::nordpool_tariffs::Tariffs;

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
    pub fn get_tariffs(&self, date_time: DateTime<Local>) -> Result<Vec<f64>, String> {
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
            .send()
            .map_err(|e| format!("Get request error: {}", e.to_string()))?;

        if res.status() != StatusCode::OK {
            return Err(format!("Http error: {}", res.status().to_string()))
        }

        let json = res.text().map_err(|e| e.to_string())?;

        let tariffs: Tariffs = serde_json::from_str(&json).map_err(|e| e.to_string())?;

        NordPool::tariffs_to_vec(&tariffs)
    }

    /// Transforms the Tariffs struct to a plain vector of prices
    ///
    /// # Arguments
    ///
    /// * 'tariffs' - the struct containing prices
    fn tariffs_to_vec(tariffs: &Tariffs) -> Result<Vec<f64>, String> {
        if tariffs.multi_area_entries.len() != 24 {
            return Err("number of day tariffs not equal to 24".to_string());
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