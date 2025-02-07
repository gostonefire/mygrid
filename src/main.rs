use chrono::{Local};
use crate::manager_nordpol::tariffs;
use std::env;

mod manager_nordpol;
mod manager_fox_cloud;
mod manager_sun;
mod models;


fn main() {
    let api_key: String;
    let inverter_sn: String;
    match env::var("FOX_ESS_API_KEY") {
        Ok(v) => api_key = v,
        Err(e) => {println!("Error getting API key: {}", e); return;}
    }
    match env::var("FOX_ESS_INVERTER_SN") {
        Ok(v) => inverter_sn = v,
        Err(e) => {println!("Error getting inverter SN: {}", e); return;}
    }

    // https://dataportal-api.nordpoolgroup.com/api/DayAheadPrices?date=2024-11-14&market=DayAhead&deliveryArea=SE4&currency=SEK
    let _ = tariffs();
    let x = manager_sun::get_max_elevation(Local::now(), 56.2f64);

    let fox = manager_fox_cloud::Fox::new(api_key);

    //let y = fox.get_device_detail(SN);
    //let y = fox.get_min_soc_on_grid(SN);
    //let y = fox.get_device_time(SN);
    let y = fox.get_battery_charging_time_schedule(&inverter_sn);

    match y {
        Ok(r) => { println!("{} {} {} {} {}",
            r.enable_1,
            r.start_time_1.hour,
            r.start_time_1.minute,
            r.end_time_1.hour,
            r.end_time_1.minute
        ); },
        Err(e) => { println!("Error: {}", e); }
    }
    println!("{}", x);
}
