use std::fmt;
use std::time::Duration;
use chrono::{DateTime, Datelike, Local, Timelike};
use ureq::{Agent, Error};
use crate::models::smhi_forecast::{FullForecast, TimeValues};

pub enum SMHIError {
    SMHI(String),
    Document(String),
}

impl fmt::Display for SMHIError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SMHIError::SMHI(e) => write!(f, "SMHIError::SMHI: {}", e),
            SMHIError::Document(e) => write!(f, "SMHIError::Document: {}", e),
        }
    }
}
impl From<Error> for SMHIError {
    fn from(e: Error) -> Self {
        SMHIError::SMHI(e.to_string())
    }
}
impl From<serde_json::Error> for SMHIError {
    fn from(e: serde_json::Error) -> Self {
        SMHIError::Document(e.to_string())
    }
}

/// Struct for managing whether forecasts produced by SMHI
pub struct SMHI {
    agent: Agent,
    lat: f64,
    long: f64,
    forecast: [TimeValues;24],
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
    pub fn new(lat: f64, long: f64) -> SMHI {
        let config = Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .build();

        let agent = config.into();

        let forecast = [TimeValues { valid_time: Default::default(), temp: 0.0, cloud: 0.0 }; 24];

        Self { agent, lat, long, forecast }
    }

    /// Sets an existing forecast
    ///
    /// # Arguments
    ///
    /// * 'forecast' - the forecast to set
    pub fn set_forecast(&mut self, forecast: [TimeValues; 24]) {
        self.forecast = forecast;
    }

    /// Gets the current forecast
    ///
    pub fn get_forecast(&self) -> &[TimeValues; 24] {
        &self.forecast
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
    pub fn new_forecast(&mut self, date_time: DateTime<Local>) -> Result<[TimeValues;24], SMHIError> {
        let smhi_domain = "https://opendata-download-metfcst.smhi.se";
        let base_url = "/api/category/pmp3g/version/2/geotype/point";
        let url = format!("{}{}/lon/{:0.4}/lat/{:0.4}/data.json",
                          smhi_domain, base_url, self.long, self.lat);

        let date = date_time.date_naive();

        let json = self.agent
            .get(url)
            .call()?
            .body_mut()
            .read_to_string()?;

        let tmp_forecast: FullForecast = serde_json::from_str(&json)?;

        let mut forecast: Vec<TimeValues> = Vec::with_capacity(24);

        for ts in tmp_forecast.time_series {
            if ts.valid_time.date_naive() == date {
                let mut time_values = TimeValues {
                    valid_time: ts.valid_time.clone(),
                    temp: 0.0,
                    cloud: 0.0,
                };

                for params in ts.parameters {
                    if params.name.eq("Wsymb2") {
                        time_values.cloud = translate_wsymb2(params.values[0]);
                    } else if params.name.eq("t") {
                        time_values.temp = params.values[0];
                    }
                }
                forecast.push(time_values);
            }
        }

        if forecast.len() == 0 {
            Err(SMHIError::SMHI(format!("No forecast found for {}", date_time.date_naive())))
        } else {
            self.fill_in_gaps(forecast);
            Ok(self.forecast)
        }
    }

    /// Takes a forecast that may have time slots where SMHI hasn't reported any data for.
    /// That could be that the forecast is for the current day in which only hours to come
    /// have data, or if the forecast is several days in the future in which SMHI only
    /// reports some few hours of the day.
    ///
    /// This function fills in those gaps given data available using a very simple
    /// interpolate/extrapolate algorithm (i.e. split the half). If however any current forecast
    /// in the struct contains data for those gaps, such data will be copied to the new forecast
    /// before calculating data.
    ///
    /// # Arguments
    ///
    /// * 'forecast' - whether forecast to enrich (if needed)
    fn fill_in_gaps(&mut self, mut forecast: Vec<TimeValues>) {
        let mut new_forecast = [TimeValues { valid_time: Default::default(), temp: 0.0, cloud: 0.0 }; 24];

        let mut available = forecast
            .iter()
            .map(|t| t.valid_time.hour() as usize)
            .collect::<Vec<usize>>();

        let year = forecast[0].valid_time.year();
        let ordinal = forecast[0].valid_time.ordinal0();
        for h in self.forecast {
            if h.valid_time.year() == year && h.valid_time.ordinal0() == ordinal {
                if !available.contains(&(h.valid_time.hour() as usize)) {
                    forecast.push(h.clone());
                    available.push(h.valid_time.hour() as usize);
                }
            }
        }

        available.sort();
        forecast.sort_by(|a, b| a.valid_time.cmp(&b.valid_time));

        let mut next_to_set: usize = 0;
        for (i, h) in forecast.iter().enumerate() {
            let this_hour = h.valid_time.hour() as usize;
            let mut next_hour = available.get(i + 1).map_or(24, |v| *v);
            next_hour = this_hour + (next_hour - this_hour) / 2 + 1;
            for j in next_to_set..next_hour {
                new_forecast[j].valid_time = h.valid_time.with_hour(j as u32).unwrap();
                new_forecast[j].temp = h.temp;
                new_forecast[j].cloud = h.cloud;
            }
            next_to_set = next_hour;
        }

        self.forecast = new_forecast;
    }
}

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
