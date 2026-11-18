//! Weather from Open-Meteo (free, no key). Requests go out from Rust only; the
//! UI never talks to the network. Only the chosen place's coordinates are sent.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

const CACHE_FOR: Duration = Duration::from_secs(15 * 60);

#[derive(Clone, Serialize)]
pub struct Day {
    pub date: String,
    pub code: i32,
    pub max: f64,
    pub min: f64,
    /// Local times, "2026-11-18T06:52".
    pub sunrise: String,
    pub sunset: String,
    /// Chance of rain, 0–100.
    pub rain_chance: f64,
}

#[derive(Clone, Serialize)]
pub struct Hour {
    /// Local time, "2026-11-18T14:00".
    pub time: String,
    pub temperature: f64,
    pub code: i32,
    pub rain_chance: f64,
}

#[derive(Clone, Serialize)]
pub struct Weather {
    pub temperature: f64,
    pub code: i32,
    pub is_day: bool,
    pub feels_like: f64,
    pub humidity: f64,
    pub wind: f64,
    /// "km/h" or "mph"
    pub wind_unit: String,
    /// The current local time at the place, "2026-11-18T14:15".
    pub local_time: String,
    pub days: Vec<Day>,
    /// The next 12 hours, starting with this one.
    pub hours: Vec<Hour>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Place {
    pub name: String,
    pub region: String,
    pub country: String,
    pub latitude: f64,
    pub longitude: f64,
}

static CACHE: Mutex<Option<HashMap<String, (Instant, Weather)>>> = Mutex::new(None);

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .user_agent("luma-ai-bar")
        .build()
}

#[derive(Deserialize)]
struct ForecastResponse {
    current: Current,
    daily: Daily,
    hourly: Hourly,
}

#[derive(Deserialize)]
struct Hourly {
    time: Vec<String>,
    temperature_2m: Vec<f64>,
    weather_code: Vec<i32>,
    #[serde(default)]
    precipitation_probability: Vec<Option<f64>>,
}

#[derive(Deserialize)]
struct Current {
    temperature_2m: f64,
    weather_code: i32,
    is_day: i32,
    time: String,
    #[serde(default)]
    apparent_temperature: f64,
    #[serde(default)]
    relative_humidity_2m: f64,
    #[serde(default)]
    wind_speed_10m: f64,
}

#[derive(Deserialize)]
struct Daily {
    time: Vec<String>,
    weather_code: Vec<i32>,
    temperature_2m_max: Vec<f64>,
    temperature_2m_min: Vec<f64>,
    #[serde(default)]
    sunrise: Vec<String>,
    #[serde(default)]
    sunset: Vec<String>,
    #[serde(default)]
    precipitation_probability_max: Vec<Option<f64>>,
}

fn fetch(lat: f64, lon: f64, fahrenheit: bool) -> Result<Weather, String> {
    let r: ForecastResponse = agent()
        .get("https://api.open-meteo.com/v1/forecast")
        .query("latitude", &format!("{lat:.3}"))
        .query("longitude", &format!("{lon:.3}"))
        .query("current", "temperature_2m,weather_code,is_day,apparent_temperature,relative_humidity_2m,wind_speed_10m")
        .query("daily", "weather_code,temperature_2m_max,temperature_2m_min,sunrise,sunset,precipitation_probability_max")
        .query("hourly", "temperature_2m,weather_code,precipitation_probability")
        .query("wind_speed_unit", if fahrenheit { "mph" } else { "kmh" })
        .query("timezone", "auto")
        .query("forecast_days", "7")
        .query("temperature_unit", if fahrenheit { "fahrenheit" } else { "celsius" })
        .call()
        .map_err(|e| format!("weather service unreachable: {e}"))?
        .into_json()
        .map_err(|e| format!("unexpected weather data: {e}"))?;
    let d = r.daily;
    let days = (0..d.time.len())
        .map(|i| Day {
            date: d.time[i].clone(),
            code: *d.weather_code.get(i).unwrap_or(&0),
            max: *d.temperature_2m_max.get(i).unwrap_or(&0.0),
            min: *d.temperature_2m_min.get(i).unwrap_or(&0.0),
            sunrise: d.sunrise.get(i).cloned().unwrap_or_default(),
            sunset: d.sunset.get(i).cloned().unwrap_or_default(),
            rain_chance: d.precipitation_probability_max.get(i).copied().flatten().unwrap_or(0.0),
        })
        .collect();
    // Hours from the current one on; times are the place's local time.
    let h = r.hourly;
    let this_hour = r.current.time.get(0..13).unwrap_or("").to_string();
    let start = h.time.iter().position(|t| t.starts_with(&this_hour)).unwrap_or(0);
    let hours = (start..(start + 12).min(h.time.len()))
        .map(|i| Hour {
            time: h.time[i].clone(),
            temperature: *h.temperature_2m.get(i).unwrap_or(&0.0),
            code: *h.weather_code.get(i).unwrap_or(&0),
            rain_chance: h.precipitation_probability.get(i).copied().flatten().unwrap_or(0.0),
        })
        .collect();
    Ok(Weather {
        temperature: r.current.temperature_2m,
        code: r.current.weather_code,
        is_day: r.current.is_day == 1,
        feels_like: r.current.apparent_temperature,
        humidity: r.current.relative_humidity_2m,
        wind: r.current.wind_speed_10m,
        wind_unit: if fahrenheit { "mph" } else { "km/h" }.into(),
        local_time: r.current.time.clone(),
        days,
        hours,
    })
}

#[tauri::command]
pub async fn weather_get(latitude: f64, longitude: f64, fahrenheit: bool) -> Result<Weather, String> {
    let key = format!("{latitude:.3},{longitude:.3},{fahrenheit}");
    if let Some((at, w)) = CACHE
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .get(&key)
        .cloned()
    {
        if at.elapsed() < CACHE_FOR {
            return Ok(w);
        }
    }
    let w = tauri::async_runtime::spawn_blocking(move || fetch(latitude, longitude, fahrenheit))
        .await
        .map_err(|e| e.to_string())??;
    CACHE
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .insert(key, (Instant::now(), w.clone()));
    Ok(w)
}

#[derive(Deserialize)]
struct GeoResponse {
    #[serde(default)]
    results: Vec<GeoResult>,
}

#[derive(Deserialize)]
struct GeoResult {
    name: String,
    #[serde(default)]
    admin1: String,
    #[serde(default)]
    country: String,
    latitude: f64,
    longitude: f64,
}

/// Look up places by name for the weather widget's location picker.
#[tauri::command]
pub async fn place_search(query: String) -> Result<Vec<Place>, String> {
    let q = query.trim().to_string();
    if q.chars().count() < 2 {
        return Ok(Vec::new());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let r: GeoResponse = agent()
            .get("https://geocoding-api.open-meteo.com/v1/search")
            .query("name", &q)
            .query("count", "6")
            .query("language", "en")
            .query("format", "json")
            .call()
            .map_err(|e| format!("place search unreachable: {e}"))?
            .into_json()
            .map_err(|e| format!("unexpected place data: {e}"))?;
        Ok(r.results
            .into_iter()
            .map(|g| Place {
                name: g.name,
                region: g.admin1,
                country: g.country,
                latitude: g.latitude,
                longitude: g.longitude,
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}
