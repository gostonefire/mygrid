use chrono::{DateTime, Local, Timelike};
use reqwest::blocking::Client;
use reqwest::StatusCode;
use crate::models::smhi_forecast::{FullForecast, TimeValues};

/// Struct for managing whether forecasts produced by SMHI
pub struct SMHI {
    client: Client,
    lat: f64,
    long: f64,
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
        let client = Client::new();
        Self { client, lat, long }
    }

    /// Retrieves a whether forecast from SMHI for the given date.
    /// The raw forecast consists of several days worth of data and many whether parameters
    /// but the returned forecast will only include the specified data and data
    /// representing cloud index (0-8) and forecasted temperatures.
    ///
    /// SMHI does not always return a forecast for every hour of a date, for instance if
    /// the date is far ahead or if some hours has already passed. This function fills those gaps
    /// so that the returned data always include 24 hours of data.
    ///
    /// # Arguments
    ///
    /// * 'date_time' - the date to get a forecast for
    pub fn get_cloud_forecast(&self, date_time: DateTime<Local>) -> Result<[TimeValues;24], String> {
        let smhi_domain = "https://opendata-download-metfcst.smhi.se";
        let base_url = "/api/category/pmp3g/version/2/geotype/point";
        let url = format!("{}{}/lon/{:0.4}/lat/{:0.4}/data.json",
                          smhi_domain, base_url, self.long, self.lat);

        let date = date_time.date_naive();

        let res = self.client
            .get(url)
            .send()
            .map_err(|e| format!("Get request error: {}", e.to_string()))?;

        if res.status() != StatusCode::OK {
            return Err(format!("Http error: {}", res.status().to_string()))
        }

        let json = res.text().map_err(|e| e.to_string())?;
        let tmp_forecast: FullForecast = serde_json::from_str(&json).map_err(|e| e.to_string())?;

        let mut forecast: Vec<TimeValues> = Vec::with_capacity(24);

        for ts in tmp_forecast.time_series {
            if ts.valid_time.date_naive() == date {
                let mut time_values = TimeValues {
                    valid_time: ts.valid_time.clone(),
                    temp: 0.0,
                    cloud: 0.0,
                };

                for params in ts.parameters {
                    if params.name.eq("tcc_mean") {
                        time_values.cloud = params.values[0];
                    } else if params.name.eq("t") {
                        time_values.temp = params.values[0];
                    }
                }
                forecast.push(time_values);
            }
        }

        if forecast.len() == 0 {
            Err(format!("No forecast found for {}", date_time.date_naive()))
        } else {
            Ok(SMHI::fill_in_gaps(&forecast))
        }
    }

    /// Takes a forecast that may have time slots where SMHI hasn't reported any data for.
    /// That could be that the forecast is for the current day in which only hours to come
    /// have data, or if the forecast is several days in the future in which SMHI only
    /// reports some few hours of the day.
    ///
    /// This function fills in those gaps given data available using a very simple
    /// interpolate/extrapolate algorithm (i.e. split the half).
    ///
    /// # Arguments
    ///
    /// * 'forecast' - whether forecast to enrich (if needed)
    fn fill_in_gaps(forecast: &Vec<TimeValues>) -> [TimeValues;24] {
        let mut new_forecast = [TimeValues { valid_time: Default::default(), temp: 0.0, cloud: 0.0 }; 24];

        let available = forecast
            .iter()
            .map(|t| t.valid_time.hour() as usize)
            .collect::<Vec<usize>>();

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

        new_forecast
    }
}


