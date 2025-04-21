pub mod errors;

use std::ops::Add;
use std::time::Duration;
use chrono::{DateTime, DurationRound, Local, TimeDelta};
use ureq::Agent;
use crate::config::GeoRef;
use crate::manager_smhi::errors::SMHIError;
use crate::models::smhi_forecast::{FullForecast, ForecastValues};


/// Struct for managing whether forecasts produced by SMHI
pub struct SMHI {
    agent: Agent,
    lat: f64,
    long: f64,
    forecast: Vec<ForecastValues>,
}

impl SMHI {
    /// Returns a SMHI struct ready for fetching and processing whether forecasts from SMHI
    ///
    /// The given lat/long values will be truncated to 4 decimals since that is the max
    /// precision that SMHI allows in their forecast API
    ///
    /// # Arguments
    ///
    /// * 'lat' - latitude for the point to get forecasts for
    /// * 'long' - longitude for the point to get forecasts for
    pub fn new(config: &GeoRef) -> SMHI {
        let agent_config = Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .build();

        let agent = agent_config.into();

        Self { agent, lat: config.lat, long: config.long, forecast: Vec::new() }
    }

    /// Retrieves a whether forecast from SMHI for the given date.
    /// The raw forecast consists of several days worth of data and many whether parameters
    /// but the returned forecast will only include the specified date and data
    /// representing cloud index (0-8) and forecasted temperatures.
    ///
    /// SMHI does not always return a forecast for every hour of a date, for instance if
    /// the date is far ahead or if some hours has already passed. This function fills those gaps
    /// so that the returned data always include 24 hours of data.
    ///
    /// # Arguments
    ///
    /// * 'date_time' - the date to get a forecast for
    pub fn new_forecast(&mut self, date_time: DateTime<Local>) -> Result<Vec<ForecastValues>, SMHIError> {
        let smhi_domain = "https://opendata-download-metfcst.smhi.se";
        let base_url = "/api/category/pmp3g/version/2/geotype/point";
        let url = format!("{}{}/lon/{:0.4}/lat/{:0.4}/data.json",
                          smhi_domain, base_url, self.long, self.lat);

        let date = date_time.date_naive();
        let next_date = date.add(TimeDelta::days(1));

        let json = self.agent
            .get(url)
            .call()?
            .body_mut()
            .read_to_string()?;

        let tmp_forecast: FullForecast = serde_json::from_str(&json)?;

        let mut forecast: Vec<ForecastValues> = Vec::new();

        for ts in tmp_forecast.time_series {
            let forecast_date = ts.valid_time.date_naive();
            if forecast_date == date || forecast_date == next_date {
                let mut time_values = ForecastValues {
                    valid_time: ts.valid_time.duration_trunc(TimeDelta::hours(1)).unwrap(),
                    temp: 0.0,
                    cloud: 0.0,
                };

                let mut lcc_mean: f64 = 0.0;
                let mut mcc_mean: f64 = 0.0;
                let mut hcc_mean: f64 = 0.0;
                for params in ts.parameters {
                    match params.name.as_str() {
                        "lcc_mean" => lcc_mean = params.values[0],
                        "mcc_mean" => mcc_mean = params.values[0],
                        "hcc_mean" => hcc_mean = params.values[0],
                        "t" => time_values.temp = params.values[0],
                        _ => (),
                    }
                }
                time_values.cloud = ((lcc_mean + mcc_mean + hcc_mean) / 3.0).round();
                forecast.push(time_values);
            }
        }

        if forecast.len() == 0 {
            Err(SMHIError::SMHI(format!("No forecast found for {}", date_time.date_naive())))
        } else {
            self.top_up_forecast(date_time, forecast);
            Ok(self.forecast.clone())
        }
    }

    /// Makes sure that the forecast to store and use is relevant up to the current hour of
    /// the day. If not we copy back from the given forecast.
    ///
    /// # Arguments
    ///
    /// * 'date_time' - date time to produce forecast for
    /// * 'forecast' - whether forecast to enrich if necessary
    fn top_up_forecast(&mut self, date_time: DateTime<Local>, forecast: Vec<ForecastValues>) {
        let mut new_forecast: Vec<ForecastValues> = Vec::new();
        let date_hour = date_time.duration_trunc(TimeDelta::hours(1)).unwrap();
        let diff = (forecast[0].valid_time.duration_trunc(TimeDelta::hours(1)).unwrap() - date_hour).num_hours();
        if diff > 0 {
            for h in 0..diff {
                new_forecast.push(ForecastValues {
                    valid_time: date_hour.add(TimeDelta::hours(h)),
                    temp: forecast[0].temp,
                    cloud: forecast[0].cloud,
                });
            }
        }

        forecast.into_iter().for_each(|t| new_forecast.push(t));

        self.forecast = new_forecast;
    }
}

/*
/// Translates whether symbols to values between 0 and 5 from SMHI Wsymb2 values
/// Wsymb2 values are from 1-27 (see https://opendata.smhi.se/metfcst/pmp/parameters#cloud-cover-parameters)
/// SMHI values 1-6 represent various levels of cloudy, the rest is for instance fog, rain, snow, etc.
///
/// This function uses 1-6 as-is and estimates all others as 6, and the scale is transformed to 0-5
///
/// # Arguments
///
/// * 'value' - the Wsymb2 value
fn translate_wsymb2(value: f64) -> f64 {
    let symbol = value.floor() as u32;
    if symbol > 6 {
        5.0
    } else {
        (symbol - 1) as f64
    }
}
*/