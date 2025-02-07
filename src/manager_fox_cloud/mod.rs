use chrono::Utc;
use reqwest::blocking::{Client, Response};
use reqwest::header::{HeaderMap, HeaderValue};
use md5::{Digest, Md5};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use crate::models::charging_time_schedule::{ChargingTimeResult, ChargingTimeSchedule};
use crate::models::soc_settings::{SocSettingsResult};
use crate::models::device_details::{DeviceDetailsResult, DeviceDetails};
use crate::models::device_time::{DeviceTime, DeviceTimeResult};

const REQUEST_DOMAIN: &str = "https://www.foxesscloud.com";

pub struct Fox {
    api_key: String,
    client: Client,
}

impl Fox {
    pub fn new(api_key: String) -> Self {
        let client = Client::new();
        Self {
            api_key,
            client,
        }
    }

    /// Obtains inverter details.
    ///
    /// See https://www.foxesscloud.com/public/i18n/en/OpenApiDocument.html#get20device20detail0a3ca20id3dget20device20detail4303e203ca3e
    ///
    /// # Arguments
    ///
    /// * 'sn' - the serial number of the inverter
    pub fn get_device_detail(&self, sn: &str) -> Result<DeviceDetails, String> {
        let path = "/op/v0/device/detail";
        let json = self.get_request(&path, vec![("sn", sn)])?;

        let fox_data: DeviceDetailsResult = serde_json::from_str(&json).map_err(|e| e.to_string())?;

        Ok(fox_data.result)
    }

    /// Obtain the inverter battery min soc on grid setting
    ///
    /// See http://foxesscloud.com/public/i18n/en/OpenApiDocument.html#get20the20minimum20soc20settings20for20the20battery20of20device200a3ca20id3dget20the20minimum20soc20settings20for20the20battery20of20device204303e203ca3e
    ///
    /// # Arguments
    ///
    /// * 'sn' - the serial number of the inverter
    pub fn get_min_soc_on_grid(&self, sn: &str) -> Result<u8, String> {
        let path = "/op/v0/device/battery/soc/get";
        let json = self.get_request(&path,vec![("sn", sn)])?;

        let fox_data: SocSettingsResult = serde_json::from_str(&json).map_err(|e| e.to_string())?;

        Ok(fox_data.result.min_soc_on_grid)
    }

    /// Obtain the battery charging time schedule.
    /// This is the standard charging scheduler setting.
    ///
    /// See https://www.foxesscloud.com/public/i18n/en/OpenApiDocument.html#get20the20setting20of20battery20charging20time0a3ca20id3dget20the20setting20of20battery20charging20time4303e203ca3e
    ///
    /// # Arguments
    ///
    /// * 'sn' - the serial number of the inverter
    pub fn get_battery_charging_time_schedule(&self, sn: &str) -> Result<ChargingTimeSchedule, String> {
        let path = "/op/v0/device/battery/forceChargeTime/get";
        let json = self.get_request(&path,vec![("sn", sn)])?;

        let fox_data: ChargingTimeResult = serde_json::from_str(&json).map_err(|e| e.to_string())?;

        Ok(fox_data.result)
    }

    /// Obtain the inverter local time
    ///
    /// See https://www.foxesscloud.com/public/i18n/en/OpenApiDocument.html#get20the20device20time0a3ca20id3dget20the20device20time4303e203ca3e
    ///
    /// # Arguments
    ///
    /// * 'sn' - the serial number of the inverter
    pub fn get_device_time(&self, sn: &str) -> Result<DeviceTime, String> {
        let path = "/op/v0/device/time/get";
        let json = self.post_request(&path, format!("{{ \"sn\": \"{}\" }}", sn))?;

        let fox_data: DeviceTimeResult = serde_json::from_str(&json).map_err(|e| e.to_string())?;

        Ok(fox_data.result)
    }

    /// Builds a request and sends it as a GET.
    /// The return is the json representation of the result as specified by
    /// respective FoxESS API
    ///
    /// # Arguments
    ///
    /// * path - the API path excluding the domain
    /// * query - a vector of tuples with query parameters
    fn get_request(&self, path: &str, query: Vec<(&str, &str)>) -> Result<String, String> {
        let url = format!("{}{}", REQUEST_DOMAIN, path);
        let header = self.generate_header(&path);

        let res = self.client
            .get(url)
            .headers(header)
            .query(&query)
            .send()
            .map_err(|e| format!("Get request error: {}", e.to_string()))?;

        let json = Fox::get_check_response(res)?;

        Ok(json)
    }

    /// Builds a request and sends it as a POST.
    /// The return is the json representation of the result as specified by
    /// respective FoxESS API
    ///
    /// # Arguments
    ///
    /// * path - the API path excluding the domain
    /// * body - a string containing the payload in json format
    fn post_request(&self, path: &str, body: String) -> Result<String, String> {
        let url = format!("{}{}", REQUEST_DOMAIN, path);
        let mut header = self.generate_header(&path);
        header.append("Content-Type", HeaderValue::from_str("application/json").unwrap());

        let res = self.client
            .post(url)
            .headers(header)
            .body(body)
            .send()
            .map_err(|e| format!("Post request error: {}", e.to_string()))?;

        let json = Fox::get_check_response(res)?;

        Ok(json)
    }

    /// Generates http headers required by Fox Open API, this includes also building a
    /// md5 hashed signature.
    ///
    /// # Arguments
    ///
    /// * 'path' - the path, excluding the domain part, to the FoxESS specific API
    fn generate_header(&self, path: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();

        let timestamp = Utc::now().timestamp() * 1000;
        let signature = format!("{}\\r\\n{}\\r\\n{}", path, self.api_key, timestamp);

        let mut hasher = Md5::new();
        hasher.update(signature.as_bytes());
        let signature_md5 = hasher.finalize().iter().map(|x| format!("{:02x}", x)).collect::<String>();

        headers.append("token", HeaderValue::from_str(&self.api_key).unwrap());
        headers.append("timestamp", HeaderValue::from_str(&timestamp.to_string()).unwrap());
        headers.append("signature", HeaderValue::from_str(&signature_md5).unwrap());
        headers.append("lang", HeaderValue::from_str("en").unwrap());

        headers
    }

    /// Extracts the text body from the response, it also checks for http error and
    /// Fox ESS specific error
    ///
    /// # Arguments
    ///
    /// * 'response' - the response object from a Fox ESS request
    fn get_check_response(response: Response) -> Result<String, String> {
        if response.status() != StatusCode::OK {
            return Err(format!("Http error: {}", response.status().to_string()))
        }

        let json = response.text().map_err(|e| e.to_string())?;
        let fox_res: FoxResponse = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        if fox_res.errno != 0 {
            return Err(format!("Err from Fox: {} - {}", fox_res.errno, fox_res.msg))
        }

        Ok(json)
    }
}

#[derive(Serialize, Deserialize)]
struct FoxResponse {
    errno: u32,
    msg: String,
}


