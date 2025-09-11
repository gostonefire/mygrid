pub mod errors;

use std::ops::Add;
use std::time::Duration;
use chrono::{DateTime, DurationRound, Local, TimeDelta};
use ureq::Agent;
use crate::config::Config;
use crate::manager_smhi::errors::SMHIError;
use crate::models::smhi_forecast::{FullForecast, ForecastValues};


/// Struct for managing whether forecasts produced by SMHI
pub struct SMHI {
    agent: Agent,
    lat: f64,
    long: f64,
    forecast: Vec<ForecastValues>,
    high_clouds_factor: f64,
    mid_clouds_factor: f64,
    low_clouds_factor: f64,
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
    pub fn new(config: &Config) -> SMHI {
        let agent_config = Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .build();

        let agent = agent_config.into();

        Self { 
            agent, 
            lat: config.geo_ref.lat, 
            long: config.geo_ref.long, 
            forecast: Vec::new(), 
            high_clouds_factor: config.production.high_clouds_factor, 
            mid_clouds_factor: config.production.mid_clouds_factor, 
            low_clouds_factor: config.production.low_clouds_factor, }
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
        let base_url = "/api/category/snow1g/version/1/geotype/point";
        let url = format!("{}{}/lon/{:0.4}/lat/{:0.4}/data.json",
                          smhi_domain, base_url, self.long, self.lat);

        let date = date_time.duration_trunc(TimeDelta::days(1)).unwrap();
        let next_date = date.add(TimeDelta::days(1));

        let json = self.agent
            .get(url)
            .call()?
            .body_mut()
            .read_to_string()?;

        let tmp_forecast: FullForecast = serde_json::from_str(&json)?;

        let mut forecast: Vec<ForecastValues> = Vec::new();

        for ts in tmp_forecast.time_series {
            let forecast_time = ts.time.with_timezone(&Local);
            let forecast_date = forecast_time.duration_trunc(TimeDelta::days(1)).unwrap();
            if forecast_date == date || forecast_date == next_date {
                let time_values = ForecastValues {
                    valid_time: forecast_time,
                    temp: ts.data.air_temperature,
                    lcc_mean: ts.data.low_type_cloud_area_fraction,
                    mcc_mean: ts.data.medium_type_cloud_area_fraction,
                    hcc_mean: ts.data.high_type_cloud_area_fraction,
                    cloud_factor: 0.0,
                };

                forecast.push(time_values);
            }
        }

        forecast.iter_mut().for_each(|f| {
            f.cloud_factor =
                (1.0 - f.hcc_mean/8.0 * self.high_clouds_factor) * 
                (1.0 - f.mcc_mean/8.0 * self.mid_clouds_factor) *
                (1.0 - f.lcc_mean/8.0 * self.low_clouds_factor);    
        });
        
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
                    lcc_mean: forecast[0].lcc_mean,
                    mcc_mean: forecast[0].mcc_mean,
                    hcc_mean: forecast[0].hcc_mean,
                    cloud_factor: forecast[0].cloud_factor,
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