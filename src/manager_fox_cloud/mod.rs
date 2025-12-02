pub mod errors;

use std::ops::Add;
use std::str::FromStr;
use std::time::Duration;
use chrono::{DateTime, Local, NaiveTime, TimeDelta, Timelike, Utc};
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use ureq::Agent;
use ureq::http::{HeaderMap, HeaderName, HeaderValue};
use anyhow::Result;
use crate::config::FoxESS;
use crate::manager_fox_cloud::errors::FoxError;
use crate::models::fox_charge_time_schedule::{ChargingTime, ChargingTimeSchedule};
use crate::models::fox_soc_settings::{SocCurrentResult, RequestCurrentSoc, SetSoc};

const REQUEST_DOMAIN: &str = "https://www.foxesscloud.com";

pub struct Fox {
    api_key: String,
    sn: String,
    agent: Agent,
}

impl Fox {
    /// Returns a new instance of the Fox struct
    ///
    /// # Arguments
    ///
    /// * 'api_key' - API key for communication with Fox Cloud
    /// * 'sn' - the serial number of the inverter to manage
    pub fn new(config: &FoxESS) -> Self {
        let agent_config = Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .build();

        let agent = agent_config.into();

        Self { api_key: config.api_key.to_string(), sn: config.inverter_sn.to_string(), agent }
    }

    /// Get the battery current soc (state of charge)
    ///
    /// See https://www.foxesscloud.com/public/i18n/en/OpenApiDocument.html#get20device20real-time20data0a3ca20id3dget20device20real-time20data4303e203ca3e
    ///
    /// # Arguments
    ///
    pub fn get_current_soc(&self) -> Result<u8, FoxError> {
        let path = "/op/v0/device/real/query";

        let req = RequestCurrentSoc { sn: self.sn.clone(), variables: vec!["SoC".to_string()] };
        let req_json = serde_json::to_string(&req)?;

        let json = self.post_request(&path, req_json)?;

        let fox_data: SocCurrentResult = serde_json::from_str(&json)?;

        Ok(fox_data.result[0].datas[0].value.round() as u8)
    }

    /// Set the inverter battery min soc on grid setting
    ///
    /// See https://www.foxesscloud.com/public/i18n/en/OpenApiDocument.html#set20the20device20settings20item0a3ca20id3dset20the20device20settings20item4303e203ca3e
    ///
    /// # Arguments
    ///
    /// * 'soc' - the new min soc on grid value (10-100)
    pub fn set_min_soc_on_grid(&self, soc: u8) -> Result<(), FoxError> {
        let path = "/op/v0/device/setting/set";

        let req = SetSoc { sn: self.sn.clone(), key: "MinSocOnGrid".to_string(), value: soc.to_string() };
        let req_json = serde_json::to_string(&req)?;

        let _ = self.post_request(&path, req_json)?;

        Ok(())
    }

    /// Set the inverter battery max soc setting
    ///
    /// See https://www.foxesscloud.com/public/i18n/en/OpenApiDocument.html#set20the20device20settings20item0a3ca20id3dset20the20device20settings20item4303e203ca3e
    ///
    /// # Arguments
    ///
    /// * 'soc' - the new min soc on grid value (10-100)
    pub fn set_max_soc(&self, soc: u8) -> Result<(), FoxError> {
        let path = "/op/v0/device/setting/set";

        let req = SetSoc { sn: self.sn.clone(), key: "MaxSoc".to_string(), value: soc.to_string() };
        let req_json = serde_json::to_string(&req)?;

        let _ = self.post_request(&path, req_json)?;

        Ok(())
    }

    /// Set the battery charging time schedule.
    /// This is the standard charging scheduler setting.
    /// No time overlaps are permitted between the two schedules.
    ///
    /// See https://www.foxesscloud.com/public/i18n/en/OpenApiDocument.html#set20the20battery20charging20time0a3ca20id3dset20the20battery20charging20time4303e203ca3e
    ///
    /// # Arguments
    ///
    /// * 'enable' - whether schedule 1 shall be enabled
    /// * 'start' - start time of schedule 1 as a DateTime<Utc>
    /// * 'end' - end time of schedule 1 as a DateTime<Utc> (non-inclusive)
    pub fn set_battery_charging_time_schedule(&self, enable: bool, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<(), FoxError> {
        let path = "/op/v0/device/battery/forceChargeTime/set";

        let mut start_hour: u8 = 0;
        let mut start_minute: u8 = 0;
        let mut end_hour: u8 = 0;
        let mut end_minute: u8 = 0;

        if enable {
            let start_local = start.with_timezone(&Local);
            let end_local = end.with_timezone(&Local).add(TimeDelta::minutes(-1));

            start_hour = start_local.hour() as u8;
            start_minute = start_local.minute() as u8;
            end_hour = end_local.hour() as u8;
            end_minute = end_local.minute() as u8;
        }

        let schedule = self.build_charge_time_schedule(
            enable, start_hour, start_minute, end_hour, end_minute,
            false, 0, 0, 0, 0,
        )?;
        let req_json = serde_json::to_string(&schedule)?;

        let _ = self.post_request(&path, req_json)?;

        Ok(())
    }

    /// Disables any current ongoing charging schedule in the inverter
    ///
    pub fn disable_charge_schedule(&self) -> Result<(), FoxError> {
        self.set_battery_charging_time_schedule(
            false, Default::default(), Default::default()
        )
    }

    /*
    /// Get the inverter local time
    ///
    /// See https://www.foxesscloud.com/public/i18n/en/OpenApiDocument.html#get20the20device20time0a3ca20id3dget20the20device20time4303e203ca3e
    ///
    pub fn get_device_time(&self) -> Result<NaiveDateTime, FoxError> {
        let path = "/op/v0/device/time/get";

        let req = RequestTime { sn: self.sn.clone() };
        let req_json = serde_json::to_string(&req)?;

        let json = self.post_request(&path, req_json)?;

        let fox_data: DeviceTimeResult = serde_json::from_str(&json)?;

        let device_time = Fox::device_time_to_date_time(&fox_data.result)?;

        Ok(device_time)
    }

    /// Set the inverter local time
    ///
    /// See https://www.foxesscloud.com/public/i18n/en/OpenApiDocument.html#set20the20device20time0a3ca20id3dset20the20device20time4303e203ca3e
    ///
    /// # Arguments
    ///
    /// * 'date_time' - date and time as a DateTime<Local>, i.e., OS local time
    pub fn set_device_time(&self, date_time: NaiveDateTime) -> Result<(), FoxError> {
        let path = "/op/v0/device/time/set";

        let req = DeviceTime {
            sn: self.sn.clone(),
            year: date_time.year().to_string(),
            month: date_time.month().to_string(),
            day: date_time.day().to_string(),
            hour:date_time.hour().to_string(),
            minute: date_time.minute().to_string(),
            second: date_time.second().to_string(),
        };
        let req_json = serde_json::to_string(&req)?;

        let _ = self.post_request(&path, req_json)?;

        Ok(())
    }

     */

    /// Builds a request and sends it as a POST.
    /// The return is the JSON representation of the result as specified by
    ///  the respective FoxESS API
    ///
    /// # Arguments
    ///
    /// * path - the API path excluding the domain
    /// * body - a string containing the payload in JSON format
    fn post_request(&self, path: &str, body: String) -> Result<String, FoxError> {
        let url = format!("{}{}", REQUEST_DOMAIN, path);

        let mut req = self.agent.post(url);
        let headers = req.headers_mut().ok_or(FoxError("post RequestBuilder Error".to_string()))?;
        self.generate_headers(headers, &path, Some(vec!(("Content-Type", "application/json"))));

        let json = req
            .send(body)?
            .body_mut()
            .read_to_string()?;

        let fox_res: FoxResponse = serde_json::from_str(&json)?;
        if fox_res.errno != 0 {
            return Err(FoxError(format!("fox cloud errno: {}, msg: {}", fox_res.errno, fox_res.msg)));
        }

        Ok(json)
    }

    /// Generates http headers required by Fox Open API; this includes also building a
    /// md5 hashed signature.
    ///
    /// # Arguments
    ///
    /// * 'headers' - a header map to insert new headers into
    /// * 'path' - the path, excluding the domain part, to the FoxESS specific API
    /// * 'extra' - any extra headers to add besides FoxCloud standards
    fn generate_headers(&self, headers: &mut HeaderMap, path: &str, extra: Option<Vec<(&str, &str)>>) {

        let timestamp = Utc::now().timestamp() * 1000;
        let signature = format!("{}\\r\\n{}\\r\\n{}", path, self.api_key, timestamp);

        let mut hasher = Md5::new();
        hasher.update(signature.as_bytes());
        let signature_md5 = hasher.finalize().iter().map(|x| format!("{:02x}", x)).collect::<String>();

        headers.insert("token", HeaderValue::from_str(&self.api_key).unwrap());
        headers.insert("timestamp", HeaderValue::from_str(&timestamp.to_string()).unwrap());
        headers.insert("signature", HeaderValue::from_str(&signature_md5).unwrap());
        headers.insert("lang", HeaderValue::from_str("en").unwrap());

        if let Some(h) = extra {
            h.iter().for_each(|&(k, v)| {
                headers.insert(HeaderName::from_str(k).unwrap(), HeaderValue::from_str(v).unwrap());
            });
        }
    }

    /// Builds a charge time schedule after first checking for inconsistencies.
    /// Inconsistencies are any of:
    /// * wrong time, e.g., hour outside 0-23 or minute outside 0-59
    /// * start time after end time
    /// * overlapping between schedule 1 and 2 (times are inclusive in both ends)
    ///
    /// It does correct minor errors:
    /// * a schedule not enabled is automatically set to zero start and end time
    /// * a schedule which is enabled but with the same start and end time is disabled and zeroed
    ///
    /// # Arguments
    ///
    /// * 'sn' - the serial number of the inverter
    /// * 'enable_1' - whether schedule 1 shall be enabled
    /// * 'start_hour_1' - start hour of schedule 1
    /// * 'start_minute_1' - start minute of schedule 1
    /// * 'end_hour_1' - end hour of schedule 1
    /// * 'end_minute_1' - end minute of schedule 1
    /// * 'enable_2' - whether schedule 2 shall be enabled
    /// * 'start_hour_2' - start hour of schedule 2
    /// * 'start_minute_2' - start minute of schedule 2
    /// * 'end_hour_2' - end hour of schedule 2
    /// * 'end_minute_2' - end minute of schedule 2
    fn build_charge_time_schedule(
        &self,
        mut enable_1: bool, mut start_hour_1: u8, mut start_minute_1: u8, mut end_hour_1: u8, mut end_minute_1: u8,
        mut enable_2: bool, mut start_hour_2: u8, mut start_minute_2: u8, mut end_hour_2: u8, mut end_minute_2: u8,
    ) -> Result<ChargingTimeSchedule, FoxError> {

        // Check schedule 1 for inconsistencies
        let start_1 = NaiveTime::from_hms_opt(start_hour_1 as u32, start_minute_1 as u32, 0)
            .ok_or(FoxError("charge schedule 1 start time error".to_string()))?;
        let end_1 = NaiveTime::from_hms_opt(end_hour_1 as u32, end_minute_1 as u32, 0)
            .ok_or(FoxError("charge schedule 1 end time error".to_string()))?;
        let dur_1 = end_1 - start_1;

        if dur_1 < TimeDelta::new(0, 0).unwrap() {
            return Err(FoxError("charge schedule 1 start time is after end time".to_string()));
        }

        if !enable_1 || dur_1 == TimeDelta::new(0, 0).unwrap() {
            enable_1 = false;
            start_hour_1 = 0;
            start_minute_1 = 0;
            end_hour_1 = 0;
            end_minute_1 = 0;
        }

        // Check schedule 2 for inconsistencies
        let start_2 = NaiveTime::from_hms_opt(start_hour_2 as u32, start_minute_2 as u32, 0)
            .ok_or(FoxError("charge schedule 2 start time error".to_string()))?;
        let end_2 = NaiveTime::from_hms_opt(end_hour_2 as u32, end_minute_2 as u32, 0)
            .ok_or(FoxError("charge schedule 2 end time error".to_string()))?;
        let dur_2 = end_2 - start_2;

        if dur_2 < TimeDelta::new(0, 0).unwrap() {
            return Err(FoxError("charge schedule 2 start time is after end time".to_string()));
        }

        if !enable_2 || dur_2 <= TimeDelta::new(0, 0).unwrap() {
            enable_2 = false;
            start_hour_2 = 0;
            start_minute_2 = 0;
            end_hour_2 = 0;
            end_minute_2 = 0;
        }


        // Check if schedules are overlapping
        if enable_1 && enable_2 {
            if start_2 >= start_1 && start_2 <= start_1 + dur_1 {
                return Err(FoxError("overlapping charge schedules".to_string()));
            }
            if end_2 >= start_1 && end_2 <= start_1 + dur_1 {
                return Err(FoxError("overlapping charge schedules".to_string()));
            }
        }

        // All checks seem fine, return schedule struct
        Ok(ChargingTimeSchedule {
            sn: self.sn.clone(),
            enable_1,
            start_time_1: ChargingTime { hour: start_hour_1, minute: start_minute_1 },
            end_time_1: ChargingTime { hour: end_hour_1, minute: end_minute_1 },
            enable_2,
            start_time_2: ChargingTime { hour: start_hour_2, minute: start_minute_2 },
            end_time_2: ChargingTime { hour: end_hour_2, minute: end_minute_2 },
        })
    }

    /*
    /// Converts a DeviceTime struct to the NaiveDateTime format.
    /// The reason for going through NaiveDateTime is that the inverter is timezone unaware,
    /// hence when passing between summer and winter time, there may be a gap where an hour
    /// might hit a gap or a fold in time with time zone awareness.
    ///
    /// # Arguments
    ///
    /// * 'device_time' - the DeviceTime struct from the inverter response
    fn device_time_to_date_time(device_time: &DeviceTime) -> Result<NaiveDateTime, FoxError> {
        let dt_string = format!("{}-{}-{} {}:{}:{}",
                                device_time.year,
                                device_time.month,
                                device_time.day,
                                device_time.hour,
                                device_time.minute,
                                device_time.second);

        let naive_device_time = NaiveDateTime::parse_from_str(&dt_string, "%Y-%m-%d %H:%M:%S")
            .map_err(|e| FoxError(format!("illegal date time format from FoxCloud [{}]: {}", dt_string, e.to_string())))?;

        Ok(naive_device_time)
    }

     */
}

#[derive(Serialize, Deserialize)]
struct FoxResponse {
    errno: u32,
    msg: String,
}


