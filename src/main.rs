use chrono::{Local, TimeDelta, Utc};
use std::env;
use std::ops::Add;
use crate::charge_level::{get_charge_level};
use crate::consumption::Consumption;
use crate::manager_nordpool::NordPool;
use crate::manager_smhi::SMHI;
use crate::production::PVProduction;
use crate::time_blocks::create_schedule;

mod manager_nordpool;
mod manager_fox_cloud;
mod manager_sun;
mod models;
mod manager_smhi;
mod charge_level;
mod time_blocks;
mod production;
mod consumption;

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

    let nordpool = NordPool::new();
    let tariffs = nordpool.get_tariffs(Utc::now().add(TimeDelta::days(1))).unwrap();

    let mut schedule = create_schedule(&tariffs);
    //for s in &schedule.blocks {
    //    println!("{}", s);
    //}

    let smhi = SMHI::new(LAT, LONG);
    let forecast = smhi.get_cloud_forecast(Local::now().add(TimeDelta::days(1))).unwrap();

    let production = PVProduction::new(&forecast, LAT, LONG);
    let consumption = Consumption::new(&forecast);

    for b in 0..schedule.blocks.len() - 1 {
        if schedule.blocks[b].block_type.eq("C") {
            let block = schedule.blocks.get_mut(b + 1).unwrap();
            let selected_hours = (block.start_hour..=block.end_hour).map(|b| b).collect::<Vec<usize>>();
            let charge_level = get_charge_level(selected_hours, &production, &consumption);
            block.min_soc_on_grid = Some(charge_level);
            schedule.blocks[b].max_soc = charge_level;
        }
    }

    /*
    for block in schedule.blocks.iter_mut() {
        if block.block_type.eq("C") {
            let selected_hours = (block.start_hour..=block.end_hour).map(|b| b as u32).collect::<Vec<u32>>();
            let charge_level = get_charge_level(selected_hours, &forecast).unwrap();
            block.max_soc = charge_level as f64;
        }
    }

     */

    for s in &schedule.blocks {
        println!("{}", s);
    }

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
