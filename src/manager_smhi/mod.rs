use chrono::{DateTime, Local};
use reqwest::blocking::Client;
use reqwest::StatusCode;
use crate::models::smhi_forecast::{Forecast, FullForecast, FullTimeSeries, Parameter, TimeSeries};

pub struct SMHI {
    client: Client,
    lat: f64,
    long: f64,
}

impl SMHI {
    pub fn new(lat: f64, long: f64) -> SMHI {
        let client = Client::new();
        Self { client, lat, long }
    }

    pub fn get_cloud_forecast(&self, date_time: DateTime<Local>) -> Result<Forecast, String> {
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

        let mut forecast = Forecast {
            approved_time: tmp_forecast.approved_time,
            reference_time: tmp_forecast.reference_time,
            time_series: vec![],
        };

        for ts in tmp_forecast.time_series {
            if ts.valid_time.date_naive() == date {
                for params in ts.parameters {
                    if params.name.eq("tcc_mean") {
                        forecast.time_series.push(
                            TimeSeries {
                                valid_time: ts.valid_time.clone(),
                                parameters: Parameter {
                                    name: params.name,
                                    value: params.values[0],
                                }
                            }
                        )
                    }
                }
            }
        }

        Ok(forecast)
    }

}
