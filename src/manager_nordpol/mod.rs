use reqwest::blocking::get;

pub fn tariffs() -> String {
    let body = get("https://dataportal-api.nordpoolgroup.com/api/DayAheadPrices?date=2024-11-14&market=DayAhead&deliveryArea=SE4&currency=SEK").unwrap().text().unwrap();

    body
}