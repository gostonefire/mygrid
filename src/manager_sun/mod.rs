use std::f64::consts::PI;
use chrono::{DateTime, Datelike, Local};

pub fn get_max_elevation(date: DateTime<Local>, lat: f64) -> f64 {
    let day = date.ordinal0() as f64;

    // First calculate the declination of the day of the year
    // Formula below taken from https://www.reuk.co.uk/wordpress/solar/solar-declination/ and
    // modified for radians
    let earth_tilt = -23.44f64.to_radians();
    let p1 = earth_tilt.sin();
    let p2 = 2f64 * PI / 365.24f64 * (day + 10f64);
    let p3 = 2f64 * 0.0167f64 * (2f64 * PI / 365.24f64 * (day - 2f64)).sin();
    let declination = (p1 * (p2 + p3).cos()).asin();

    let elevation = 90f64 - lat + declination.to_degrees();

    elevation
}