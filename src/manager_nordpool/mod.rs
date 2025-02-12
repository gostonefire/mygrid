use chrono::{DateTime, Utc};
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

    pub fn get_tariffs(&self, date_time: DateTime<Utc>) -> Result<Tariffs, String> {
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
        Ok(tariffs)
    }
}