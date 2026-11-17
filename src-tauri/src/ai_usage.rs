//! AI usage widget: spend, balance or plan limits from each provider.
//!
//! Providers
//! - `anthropic`: Claude API spend this month, from the Usage and Cost Analytics
//!   API (beta) with an Admin API key. Data lands 4–24 h late, so the reading
//!   says how fresh it is.
//! - `openai`: OpenAI API spend this month, from the organization costs API with
//!   an admin key.
//! - `deepseek`: account balance, with a normal API key.
//! - `claude_code` (experimental, off unless chosen): Claude plan limits read with
//!   Claude Code's own sign-in on this PC, from an internal endpoint that can
//!   change without notice. The token is read from Claude Code's file, used for
//!   one request to api.anthropic.com, and never stored or shown.
//!
//! Every request is made here in Rust; keys never reach the UI. The last good
//! reading is kept on disk, so a failure shows "last reading at …" instead of
//! nothing.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{secrets, taskbar};

#[derive(Clone, Serialize, Deserialize)]
pub struct Meter {
    pub label: String,
    /// 0–100
    pub percent: f64,
    /// RFC 3339, when the window resets.
    pub resets_at: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Reading {
    pub provider: String,
    /// Short text for the dock tile, e.g. "$12.40" or "42%".
    pub headline: String,
    /// What the headline means, e.g. "Claude API this month".
    pub caption: String,
    /// Extra label/value lines for the expanded view.
    pub lines: Vec<(String, String)>,
    pub meters: Vec<Meter>,
    /// Where the provider's own data ends (RFC 3339), if it says.
    pub data_as_of: Option<String>,
    /// When this reading was taken (ms since epoch).
    pub fetched_at: u64,
}

#[derive(Serialize)]
pub struct UsageResult {
    /// The new reading, or the last good one if this attempt failed.
    pub reading: Option<Reading>,
    /// Why this attempt failed, if it did.
    pub error: Option<String>,
    /// True when `reading` is an older one kept from before the failure.
    pub stale: bool,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn cache_path(id: &str) -> PathBuf {
    let safe: String = id.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    taskbar::data_dir().join("ai").join(format!("{safe}.json"))
}

fn load_cached(id: &str) -> Option<Reading> {
    serde_json::from_str(&fs::read_to_string(cache_path(id)).ok()?).ok()
}

fn save_cached(id: &str, r: &Reading) {
    let p = cache_path(id);
    if let Some(d) = p.parent() {
        let _ = fs::create_dir_all(d);
    }
    let _ = fs::write(p, serde_json::to_string(r).unwrap_or_default());
}

pub fn forget(id: &str) {
    secrets::remove(id);
    let _ = fs::remove_file(cache_path(id));
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .user_agent("luma-ai-bar")
        .build()
}

/// Turn an HTTP failure into a sentence a person can act on.
fn http_error(provider: &str, e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(401, _) | ureq::Error::Status(403, _) => {
            format!("{provider} refused the key. Check it's the right kind of key and paste it again.")
        }
        ureq::Error::Status(404, _) => format!("{provider} doesn't offer this report to this account."),
        ureq::Error::Status(429, _) => format!("{provider} asked us to slow down; will try again later."),
        ureq::Error::Status(code, _) => format!("{provider} returned an error ({code})."),
        ureq::Error::Transport(_) => format!("Can't reach {provider}. Check the internet connection."),
    }
}

/// First day of this month, 00:00 UTC, as RFC 3339 and as a Unix time.
fn month_start() -> (String, i64) {
    // Days since epoch -> civil date (Howard Hinnant's algorithm), no chrono needed.
    let secs = (now_ms() / 1000) as i64;
    let days = secs.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    let day_of_month = doy - (153 * mp + 2) / 5 + 1;
    let start = secs - (day_of_month - 1) * 86_400 - secs.rem_euclid(86_400);
    (format!("{year:04}-{month:02}-01T00:00:00Z"), start)
}

fn money(dollars: f64) -> String {
    if dollars >= 1000.0 {
        format!("${:.0}", dollars)
    } else {
        format!("${dollars:.2}")
    }
}

/// Sum every `field` (a number or a numeric string) found anywhere inside `v`.
fn sum_field(v: &Value, field: &str) -> f64 {
    match v {
        Value::Object(m) => m
            .iter()
            .map(|(k, x)| {
                if k == field {
                    match x {
                        Value::String(s) => s.parse::<f64>().unwrap_or(0.0),
                        Value::Number(n) => n.as_f64().unwrap_or(0.0),
                        other => sum_field(other, field),
                    }
                } else {
                    sum_field(x, field)
                }
            })
            .sum(),
        Value::Array(a) => a.iter().map(|x| sum_field(x, field)).sum(),
        _ => 0.0,
    }
}

// ---------------------------------------------------------------------------
// Providers
// ---------------------------------------------------------------------------

fn anthropic(key: &str) -> Result<Reading, String> {
    let (start, _) = month_start();
    let body: Value = agent()
        .get("https://api.anthropic.com/v1/organizations/analytics/cost")
        .set("x-api-key", key)
        .set("anthropic-version", "2023-06-01")
        .set("anthropic-beta", "analytics-usage-cost-2026-08-24")
        .query("starting_at", &start)
        .query("limit", "1000")
        .call()
        .map_err(|e| http_error("Anthropic", e))?
        .into_json()
        .map_err(|e| format!("Anthropic sent data this build can't read ({e})."))?;
    let data_as_of = body.get("data_refreshed_at").and_then(|v| v.as_str()).map(String::from);
    // `amount` is a decimal string in cents, after discounts and before credits.
    let cents = sum_field(body.get("data").unwrap_or(&Value::Null), "amount");
    let more = body.get("has_more").and_then(|v| v.as_bool()).unwrap_or(false);
    let mut lines = vec![("Spend this month".to_string(), money(cents / 100.0))];
    if more {
        lines.push(("Note".into(), "Partial: more rows than one page".into()));
    }
    if data_as_of.is_none() {
        lines.push(("Note".into(), "No data has landed for this month yet".into()));
    }
    Ok(Reading {
        provider: "anthropic".into(),
        headline: money(cents / 100.0),
        caption: "Claude API spend this month".into(),
        lines,
        meters: vec![],
        data_as_of,
        fetched_at: now_ms(),
    })
}

fn openai(key: &str) -> Result<Reading, String> {
    let (_, start) = month_start();
    let body: Value = agent()
        .get("https://api.openai.com/v1/organization/costs")
        .set("Authorization", &format!("Bearer {key}"))
        .query("start_time", &start.to_string())
        .query("bucket_width", "1d")
        .query("limit", "31")
        .call()
        .map_err(|e| http_error("OpenAI", e))?
        .into_json()
        .map_err(|e| format!("OpenAI sent data this build can't read ({e})."))?;
    // Each result carries amount: { value (USD), currency }.
    let dollars = sum_field(body.get("data").unwrap_or(&Value::Null), "value");
    Ok(Reading {
        provider: "openai".into(),
        headline: money(dollars),
        caption: "OpenAI API spend this month".into(),
        lines: vec![("Spend this month".into(), money(dollars))],
        meters: vec![],
        data_as_of: None,
        fetched_at: now_ms(),
    })
}

fn deepseek(key: &str) -> Result<Reading, String> {
    let body: Value = agent()
        .get("https://api.deepseek.com/user/balance")
        .set("Authorization", &format!("Bearer {key}"))
        .set("Accept", "application/json")
        .call()
        .map_err(|e| http_error("DeepSeek", e))?
        .into_json()
        .map_err(|e| format!("DeepSeek sent data this build can't read ({e})."))?;
    let infos = body.get("balance_infos").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let first = infos.first().ok_or("DeepSeek sent no balance.")?;
    let currency = first.get("currency").and_then(|v| v.as_str()).unwrap_or("");
    let total = first.get("total_balance").and_then(|v| v.as_str()).unwrap_or("0");
    let symbol = match currency {
        "USD" => "$",
        "CNY" => "¥",
        _ => "",
    };
    let headline = format!("{symbol}{total}");
    let mut lines = vec![("Balance".to_string(), format!("{total} {currency}"))];
    for (label, field) in [("Granted", "granted_balance"), ("Topped up", "topped_up_balance")] {
        if let Some(v) = first.get(field).and_then(|v| v.as_str()) {
            lines.push((label.into(), format!("{v} {currency}")));
        }
    }
    if body.get("is_available").and_then(|v| v.as_bool()) == Some(false) {
        lines.push(("Status".into(), "Balance too low for API calls".into()));
    }
    Ok(Reading {
        provider: "deepseek".into(),
        headline,
        caption: "DeepSeek balance".into(),
        lines,
        meters: vec![],
        data_as_of: None,
        fetched_at: now_ms(),
    })
}

/// Claude Code keeps its sign-in in `%CLAUDE_CONFIG_DIR%` or `~\.claude`.
fn claude_code_file() -> Option<PathBuf> {
    let dir = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join(".claude")))?;
    let f = dir.join(".credentials.json");
    f.exists().then_some(f)
}

fn label_for(window: &str) -> String {
    match window {
        "five_hour" => "Session".into(),
        "seven_day" => "Weekly".into(),
        "seven_day_opus" => "Weekly (Opus)".into(),
        "seven_day_sonnet" => "Weekly (Sonnet)".into(),
        other => other.replace('_', " "),
    }
}

fn claude_code() -> Result<Reading, String> {
    let file = claude_code_file()
        .ok_or("Claude Code's sign-in wasn't found on this PC. Sign in to Claude Code first.")?;
    let creds: Value = serde_json::from_str(&fs::read_to_string(&file).map_err(|e| e.to_string())?)
        .map_err(|_| "Claude Code's sign-in file isn't in a format this build understands.")?;
    let token = creds
        .pointer("/claudeAiOauth/accessToken")
        .and_then(|v| v.as_str())
        .ok_or("Claude Code's sign-in file has no access token.")?;
    if let Some(exp) = creds.pointer("/claudeAiOauth/expiresAt").and_then(|v| v.as_u64()) {
        if exp < now_ms() {
            return Err("Claude Code's sign-in has expired. Open Claude Code once to renew it.".into());
        }
    }
    let body: Value = agent()
        .get("https://api.anthropic.com/api/oauth/usage")
        .set("Authorization", &format!("Bearer {token}"))
        .set("anthropic-beta", "oauth-2025-04-20")
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(401, _) => {
                "Claude Code's sign-in has expired. Open Claude Code once to renew it.".to_string()
            }
            other => http_error("Anthropic", other),
        })?
        .into_json()
        .map_err(|e| format!("The plan-limits endpoint sent data this build can't read ({e})."))?;

    // Each limit window looks like { "utilization": <0–100>, "resets_at": "..." }.
    let mut meters: Vec<Meter> = body
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(k, v)| {
            let u = v.get("utilization")?.as_f64()?;
            Some(Meter {
                label: label_for(k),
                percent: if u <= 1.0 { u * 100.0 } else { u }.clamp(0.0, 100.0),
                resets_at: v.get("resets_at").and_then(|r| r.as_str()).map(String::from),
            })
        })
        .collect();
    if meters.is_empty() {
        return Err("The plan-limits endpoint changed shape; this experimental reader no longer understands it.".into());
    }
    // Shortest window first: it is the one that runs out soonest.
    meters.sort_by_key(|m| if m.label == "Session" { 0 } else { 1 });
    Ok(Reading {
        provider: "claude_code".into(),
        headline: format!("{:.0}%", meters[0].percent),
        caption: format!("Claude plan, {}", meters[0].label.to_lowercase()),
        lines: vec![],
        meters,
        data_as_of: None,
        fetched_at: now_ms(),
    })
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn ai_key_status(id: String) -> bool {
    secrets::exists(&id)
}

#[tauri::command]
pub fn ai_set_key(id: String, key: String) -> Result<(), String> {
    let key = key.trim();
    if key.len() < 8 {
        return Err("That doesn't look like a complete key.".into());
    }
    secrets::store(&id, key)
}

#[tauri::command]
pub fn ai_clear_key(id: String) {
    forget(&id);
}

#[tauri::command]
pub fn ai_claude_code_found() -> bool {
    claude_code_file().is_some()
}

/// Fetch a fresh reading; on failure return the last good one marked stale.
#[tauri::command]
pub async fn ai_usage_fetch(id: String, provider: String) -> UsageResult {
    let id2 = id.clone();
    let attempt = tauri::async_runtime::spawn_blocking(move || -> Result<Reading, String> {
        if provider == "claude_code" {
            return claude_code();
        }
        let key = secrets::load(&id2).ok_or("Add a key to start.")?;
        match provider.as_str() {
            "anthropic" => anthropic(&key),
            "openai" => openai(&key),
            "deepseek" => deepseek(&key),
            other => Err(format!("Unknown provider {other}.")),
        }
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()));

    match attempt {
        Ok(r) => {
            save_cached(&id, &r);
            UsageResult { reading: Some(r), error: None, stale: false }
        }
        Err(error) => {
            let last = load_cached(&id);
            UsageResult { stale: last.is_some(), reading: last, error: Some(error) }
        }
    }
}
