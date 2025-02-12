use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta, TimeZone, Utc};
use std::env;
use std::ops::Add;
use reqwest::header::DATE;
use crate::charge_level::{get_charge_level};

mod manager_nordpool;
mod manager_fox_cloud;
mod manager_sun;
mod models;
mod manager_smhi;
mod charge_level;

const LAT: f64 = 56.22332313734338;
const LONG: f64 = 15.658393416666142;

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

    let charge_level = get_charge_level(Local::now().add(TimeDelta::days(1)), vec![8, 9, 10]).unwrap();
    println!("{}", charge_level);


    /*
    56.22332313734338, 15.658393416666142
    let nordpool = manager_nordpool::NordPool::new();
    let y = nordpool.get_tariffs(Utc::now().add(TimeDelta::days(1)));
    match y {
        Ok(r) => {
            for d in r.multi_area_entries {
                println!("{:?}: {:0.2}kr", d.delivery_start.naive_local().time(), (d.entry_per_area.se4 / 10f64).round() / 100f64);
            }
        },
        Err(e) => { println!("Error: {}", e); }
    }
*/
/*
    let fox = manager_fox_cloud::Fox::new(api_key);

    //let y = fox.get_device_detail(SN);
    //let y = fox.get_min_soc_on_grid(&inverter_sn);
    //let y = fox.get_device_time(&inverter_sn);
    //let y = fox.get_battery_charging_time_schedule(&inverter_sn);
    //let y = fox.get_current_soc(&inverter_sn);
    let y = fox.get_max_soc(&inverter_sn);
    //let y = fox.set_max_soc(&inverter_sn, 100);
    //let y = fox.set_device_time(&inverter_sn, Local::now());
    //let y = fox.set_min_soc_on_grid(&inverter_sn, 45);
    //let y = fox.set_battery_charging_time_schedule(
    //    &inverter_sn,
    //    false, 1, 30, 3, 59,
    //    false, 13, 0, 15, 59,
    //);

    match y {
        Ok(r) => { println!("{}", r); },
        //Ok(r) => {
        //    println!("{}-{:0>2}-{:0>2} {:0>2}:{:0>2}:{:0>2}", r.year, r.month, r.day, r.hour, r.minute, r.second);
        //},
        //Ok(()) => { println!("Fox was successfully set!");}
        Err(e) => { println!("Error: {}", e); }
    }
    println!("{}", x);
    */
}
