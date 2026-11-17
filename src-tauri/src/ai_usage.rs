//! AI usage widget: spend, balance or plan limits from each provider.
//!
//! Providers
//! - `anthropic`: Claude API spend this month, from the Usage and Cost Analytics
//!   API (beta) with an Admin API key. Data lands 4–24 h late, so the reading
//!   says how fresh it is.
//! - `openai`: OpenAI API spend this month, from the organization costs API with
//!   an admin key.
//! - `deepseek`: account balance, with a normal API key.
//! - `openrouter`: credits used and left on an OpenRouter API key.
//! - `kimi`: Moonshot AI (Kimi) account balance, with a normal API key.
//! - `codex`: ChatGPT plan limits, read from the rate-limit snapshots Codex CLI
//!   writes into its own session logs on this PC. No request is made; the
//!   figures are as fresh as your last Codex turn.
//! - `claude_code`: Claude plan limits (Session, Weekly) read with
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
        ureq::Error::Status(429, resp) => {
            // Tagged so the caller can back off for as long as the service asks.
            let secs = resp.header("retry-after").and_then(|v| v.trim().parse::<u64>().ok()).unwrap_or(0);
            format!("{RATE_LIMITED}{secs}|{provider} asked us to slow down for a while.")
        }
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

fn num(v: &Value, field: &str) -> Option<f64> {
    match v.get(field)? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn openrouter(key: &str) -> Result<Reading, String> {
    let body: Value = agent()
        .get("https://openrouter.ai/api/v1/key")
        .set("Authorization", &format!("Bearer {key}"))
        .call()
        .map_err(|e| http_error("OpenRouter", e))?
        .into_json()
        .map_err(|e| format!("OpenRouter sent data this build can't read ({e})."))?;
    let d = body.get("data").ok_or("OpenRouter sent no key details.")?;
    let usage = num(d, "usage").unwrap_or(0.0);
    let mut lines = vec![];
    for (label, field) in [("Today", "usage_daily"), ("This week", "usage_weekly"), ("This month", "usage_monthly")] {
        if let Some(v) = num(d, field) {
            lines.push((label.to_string(), money(v)));
        }
    }
    lines.push(("All time".into(), money(usage)));
    let mut meters = vec![];
    let (headline, caption) = match (num(d, "limit"), num(d, "limit_remaining")) {
        (Some(limit), Some(left)) if limit > 0.0 => {
            lines.push(("Key limit".into(), money(limit)));
            meters.push(Meter {
                label: "Key limit used".into(),
                percent: ((limit - left) / limit * 100.0).clamp(0.0, 100.0),
                resets_at: None,
            });
            (money(left), format!("OpenRouter, left of {}", money(limit)))
        }
        _ => (money(num(d, "usage_monthly").unwrap_or(usage)), "OpenRouter spend this month".to_string()),
    };
    if d.get("is_free_tier").and_then(|v| v.as_bool()) == Some(true) {
        lines.push(("Tier".into(), "Free".into()));
    }
    Ok(Reading { provider: "openrouter".into(), headline, caption, lines, meters, data_as_of: None, fetched_at: now_ms() })
}

fn kimi(key: &str) -> Result<Reading, String> {
    // Keys from the international and China platforms only work on their own host.
    let ask = |host: &str| -> Result<Value, ureq::Error> {
        agent()
            .get(&format!("https://{host}/v1/users/me/balance"))
            .set("Authorization", &format!("Bearer {key}"))
            .call()?
            .into_json()
            .map_err(ureq::Error::from)
    };
    let (body, symbol) = match ask("api.moonshot.ai") {
        Ok(b) => (b, "$"),
        Err(ureq::Error::Status(401, _)) => (ask("api.moonshot.cn").map_err(|e| http_error("Kimi", e))?, "¥"),
        Err(e) => return Err(http_error("Kimi", e)),
    };
    let d = body.get("data").ok_or("Kimi sent no balance.")?;
    let fmt = |v: f64| format!("{symbol}{v:.2}");
    let available = num(d, "available_balance").unwrap_or(0.0);
    let mut lines = vec![("Available".to_string(), fmt(available))];
    if let Some(v) = num(d, "cash_balance") {
        lines.push(("Cash".into(), fmt(v)));
    }
    if let Some(v) = num(d, "voucher_balance") {
        lines.push(("Vouchers".into(), fmt(v)));
    }
    if available <= 0.0 {
        lines.push(("Status".into(), "Balance too low for API calls".into()));
    }
    Ok(Reading {
        provider: "kimi".into(),
        headline: fmt(available),
        caption: "Kimi balance".into(),
        lines,
        meters: vec![],
        data_as_of: None,
        fetched_at: now_ms(),
    })
}

/// Seconds since the epoch as RFC 3339 (UTC).
fn rfc3339(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Codex CLI keeps its sessions in `%CODEX_HOME%\sessions` or `~\.codex\sessions`.
fn codex_sessions_dir() -> Option<PathBuf> {
    let home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join(".codex")))?;
    let dir = home.join("sessions");
    dir.is_dir().then_some(dir)
}

fn dirs_newest_first(p: &std::path::Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(p).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    v.sort();
    v.reverse();
    v
}

/// Session logs, newest first, walking year/month/day folders from the latest.
fn codex_recent_logs(dir: &std::path::Path, max: usize) -> Vec<(std::time::SystemTime, PathBuf)> {
    let mut out = vec![];
    for year in dirs_newest_first(dir) {
        for month in dirs_newest_first(&year) {
            for day in dirs_newest_first(&month) {
                out.extend(
                    fs::read_dir(&day)
                        .into_iter()
                        .flatten()
                        .flatten()
                        .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
                        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path()))),
                );
                if out.len() >= max {
                    out.sort_by(|a, b| b.0.cmp(&a.0));
                    out.truncate(max);
                    return out;
                }
            }
        }
    }
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out
}

/// The last rate-limit snapshot in one session log, read from its end.
fn codex_snapshot(path: &std::path::Path) -> Option<Value> {
    use std::io::{Read, Seek, SeekFrom};
    const TAIL: u64 = 4 << 20;
    let mut f = fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(TAIL))).ok()?;
    let mut bytes = vec![];
    f.take(TAIL).read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    text.lines().rev().filter(|l| l.contains("\"rate_limits\"")).find_map(|line| {
        let v: Value = serde_json::from_str(line).ok()?;
        let rl = v.get("payload").unwrap_or(&v).get("rate_limits")?;
        let has = |k: &str| rl.get(k).is_some_and(|w| !w.is_null());
        (has("primary") || has("secondary")).then(|| rl.clone())
    })
}

fn codex() -> Result<Reading, String> {
    let dir = codex_sessions_dir()
        .ok_or("No Codex CLI sessions were found on this PC. Use Codex signed in with ChatGPT first.")?;
    let (when, snapshot) = codex_recent_logs(&dir, 40)
        .into_iter()
        .find_map(|(t, p)| Some((t, codex_snapshot(&p)?)))
        .ok_or("Codex hasn't recorded any plan limits yet. They appear after your next Codex turn.")?;
    let as_of = when.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let now = (now_ms() / 1000) as i64;

    let mut meters = vec![];
    for key in ["primary", "secondary"] {
        let Some(w) = snapshot.get(key).filter(|w| !w.is_null()) else { continue };
        let minutes = w.get("window_minutes").and_then(|v| v.as_i64());
        let resets = w
            .get("resets_at")
            .and_then(|v| v.as_i64())
            .or_else(|| w.get("resets_in_seconds").and_then(|v| v.as_i64()).map(|s| as_of + s));
        let mut percent = w.get("used_percent").and_then(|v| v.as_f64()).unwrap_or(0.0);
        // The window has rolled over since Codex last looked.
        if resets.is_some_and(|r| r <= now) {
            percent = 0.0;
        }
        let label = match minutes {
            Some(m) if m <= 6 * 60 => "Session".to_string(),
            Some(m) if (9_000..=11_000).contains(&m) => "Weekly".to_string(),
            Some(m) if m % 1440 == 0 => format!("{} days", m / 1440),
            Some(m) => format!("{} hours", (m as f64 / 60.0).round()),
            None if key == "primary" => "Session".to_string(),
            None => "Weekly".to_string(),
        };
        meters.push(Meter { label, percent: percent.clamp(0.0, 100.0), resets_at: resets.map(rfc3339) });
    }
    if meters.is_empty() {
        return Err("Codex's last session has no plan limits in it.".into());
    }
    meters.sort_by_key(|m| meter_rank(&m.label));
    let mut lines = vec![];
    if let Some(plan) = snapshot.get("plan_type").and_then(|v| v.as_str()) {
        let mut c = plan.chars();
        lines.push(("Plan".to_string(), c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()));
    }
    Ok(Reading {
        provider: "codex".into(),
        headline: format!("{}%", meters[0].percent.round()),
        caption: format!("ChatGPT plan, {}", meters[0].label.to_lowercase()),
        lines,
        meters,
        data_as_of: Some(rfc3339(as_of)),
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
        "extra_usage" => "Extra usage".into(),
        other => {
            let t = other.replace('_', " ");
            let mut c = t.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
        }
    }
}

/// Session, then Weekly, then the per-model weekly limits, then anything else
/// (such as extra usage), so the dock's two meters are always Session and Weekly.
fn meter_rank(label: &str) -> u8 {
    match label {
        "Session" => 0,
        "Weekly" => 1,
        l if l.starts_with("Weekly") => 2,
        _ => 3,
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
        return Err("Anthropic changed how plan limits are reported; this version can't read them.".into());
    }
    meters.sort_by_key(|m| meter_rank(&m.label));
    Ok(Reading {
        provider: "claude_code".into(),
        headline: format!("{}%", meters[0].percent.round()),
        caption: format!("Claude plan, {}", meters[0].label.to_lowercase()),
        lines: vec![],
        meters,
        data_as_of: None,
        fetched_at: now_ms(),
    })
}

// ---------------------------------------------------------------------------
// Pacing: at most one request a minute per source, and back off when told to
// ---------------------------------------------------------------------------

const RATE_LIMITED: &str = "~rate-limited~";
/// How often each source is actually asked. Claude's plan-limits endpoint has a
/// tight per-account limit that Claude Code's own background checks also use
/// (it can demand waits of 10+ minutes), so it is asked every 5 minutes; a
/// manual Refresh may ask once a minute. Spend and balance APIs: once a minute.
fn min_gap(source: &str, manual: bool) -> std::time::Duration {
    let secs = if manual {
        55
    } else if source == "claude_code" {
        5 * 60 - 5
    } else {
        55
    };
    std::time::Duration::from_secs(secs)
}

/// First wait after a slow-down reply with no usable Retry-After (Claude: 5 min).
fn first_backoff(source: &str) -> u64 {
    if source == "claude_code" { 5 * 60 } else { 60 }
}
const MAX_BACKOFF: u64 = 30 * 60;

#[derive(Default)]
struct Pace {
    last_ok: Option<(std::time::Instant, Reading)>,
    backoff_until: Option<std::time::Instant>,
    strikes: u32,
}

static PACE: std::sync::Mutex<Option<std::collections::HashMap<String, Pace>>> = std::sync::Mutex::new(None);

fn with_pace<R>(source: &str, f: impl FnOnce(&mut Pace) -> R) -> R {
    let mut guard = PACE.lock().unwrap();
    f(guard.get_or_insert_with(Default::default).entry(source.to_string()).or_default())
}

/// Run `fetch` unless this source was read under a minute ago (reuse that) or
/// is backing off after a slow-down reply (then say when it tries again).
fn throttled(source: &str, manual: bool, fetch: impl FnOnce() -> Result<Reading, String>) -> Result<Reading, String> {
    let now = std::time::Instant::now();
    let early = with_pace(source, |p| {
        if let Some((at, r)) = &p.last_ok {
            if now.duration_since(*at) < min_gap(source, manual) {
                return Some(Ok(r.clone()));
            }
        }
        if let Some(until) = p.backoff_until {
            if until > now {
                let mins = (until - now).as_secs().div_ceil(60);
                return Some(Err(format!(
                    "Paused: the service asked us to slow down. Trying again in {mins} min."
                )));
            }
        }
        None
    });
    if let Some(answer) = early {
        return answer;
    }
    match fetch() {
        Ok(r) => {
            with_pace(source, |p| {
                p.last_ok = Some((std::time::Instant::now(), r.clone()));
                p.backoff_until = None;
                p.strikes = 0;
            });
            Ok(r)
        }
        Err(e) if e.starts_with(RATE_LIMITED) => {
            let rest = &e[RATE_LIMITED.len()..];
            let (secs, message) = rest.split_once('|').unwrap_or(("0", rest));
            let asked = secs.parse::<u64>().unwrap_or(0);
            let wait = with_pace(source, |p| {
                p.strikes += 1;
                // Honour Retry-After; otherwise 1, 2, 4, 8 ... minutes, at most 15.
                // Some accounts get "retry-after: 0" on every refusal; treat it as unknown.
                let wait = if asked > 0 { asked } else { first_backoff(source) << (p.strikes - 1).min(4) };
                eprintln!("[ai] {source}: slow-down reply, retry-after={asked}s, waiting {wait}s");
                let wait = wait.min(MAX_BACKOFF);
                p.backoff_until = Some(std::time::Instant::now() + std::time::Duration::from_secs(wait));
                wait
            });
            Err(format!("{message} Trying again in {} min.", wait.div_ceil(60)))
        }
        Err(e) => Err(e),
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Service icons
// ---------------------------------------------------------------------------

/// Each service's own site icon, fetched once from that service and cached on
/// disk, so no logo files ship with the app.
fn icon_sources(service: &str) -> &'static [&'static str] {
    match service {
        "claude" | "claude_code" | "anthropic" => &[
            "https://claude.ai/apple-touch-icon.png",
            "https://claude.ai/favicon.ico",
        ],
        "openai" | "chatgpt" => &[
            "https://openai.com/favicon.ico",
            "https://cdn.oaistatic.com/assets/favicon-o20kmmos.svg",
            "https://chatgpt.com/apple-touch-icon.png",
        ],
        "deepseek" => &[
            "https://chat.deepseek.com/favicon.svg",
            "https://www.deepseek.com/favicon.ico",
        ],
        "openrouter" => &["https://openrouter.ai/apple-touch-icon.png", "https://openrouter.ai/favicon.ico"],
        "kimi" => &["https://www.kimi.com/favicon.ico", "https://platform.moonshot.ai/favicon.ico"],
        _ => &[],
    }
}

static ICON_CACHE: std::sync::Mutex<Option<std::collections::HashMap<String, Option<String>>>> =
    std::sync::Mutex::new(None);

fn fetch_icon(url: &str) -> Option<String> {
    use base64::Engine;
    use std::io::Read;
    let resp = agent().get(url).call().ok()?;
    let mime = resp.content_type().to_string();
    if !mime.starts_with("image/") {
        return None;
    }
    let mut bytes = Vec::new();
    resp.into_reader().take(512 * 1024).read_to_end(&mut bytes).ok()?;
    if bytes.is_empty() {
        return None;
    }
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[tauri::command]
pub async fn service_icon(service: String) -> Option<String> {
    let key: String = service.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    if let Some(hit) = ICON_CACHE.lock().unwrap().get_or_insert_with(Default::default).get(&key) {
        return hit.clone();
    }
    let file = taskbar::data_dir().join("icons").join(format!("{key}.txt"));
    let found = match fs::read_to_string(&file) {
        Ok(cached) if cached.starts_with("data:image/") => Some(cached),
        _ => {
            let sources = icon_sources(&key);
            let fetched = tauri::async_runtime::spawn_blocking(move || {
                sources.iter().find_map(|u| fetch_icon(u))
            })
            .await
            .ok()
            .flatten();
            if let Some(data) = &fetched {
                let _ = fs::create_dir_all(file.parent().unwrap());
                let _ = fs::write(&file, data);
            }
            fetched
        }
    };
    ICON_CACHE
        .lock()
        .unwrap()
        .get_or_insert_with(Default::default)
        .insert(key, found.clone());
    found
}

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

#[tauri::command]
pub fn ai_codex_found() -> bool {
    codex_sessions_dir().is_some()
}

/// Fetch a fresh reading; on failure return the last good one marked stale.
#[tauri::command]
pub async fn ai_usage_fetch(id: String, provider: String, manual: Option<bool>) -> UsageResult {
    let manual = manual.unwrap_or(false);
    // Claude plan limits belong to the account, not the widget: every Claude
    // widget shares one request. Key-based services are per widget (per key).
    let source = if provider == "claude_code" || provider == "codex" { provider.clone() } else { id.clone() };
    let id2 = id.clone();
    let attempt = tauri::async_runtime::spawn_blocking(move || -> Result<Reading, String> {
        throttled(&source, manual, || {
            if provider == "claude_code" {
                return claude_code();
            }
            if provider == "codex" {
                return codex();
            }
            let key = secrets::load(&id2).ok_or("Add a key to start.")?;
            match provider.as_str() {
                "anthropic" => anthropic(&key),
                "openai" => openai(&key),
                "deepseek" => deepseek(&key),
                "openrouter" => openrouter(&key),
                "kimi" => kimi(&key),
                other => Err(format!("Unknown provider {other}.")),
            }
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339_dates() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(1_794_934_841), "2026-11-17T17:00:41Z");
        assert_eq!(rfc3339(951_782_400), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn codex_reads_latest_snapshot() {
        let home = std::env::temp_dir().join(format!("luma-codex-test-{}", std::process::id()));
        let day = home.join("sessions").join("2026").join("11").join("17");
        fs::create_dir_all(&day).unwrap();
        let soon = now_ms() / 1000 + 3600;
        let past = now_ms() / 1000 - 60;
        let log = format!(
            "{{\"type\":\"session_meta\",\"payload\":{{}}}}\n\
             {{\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"info\":null,\"rate_limits\":{{\"primary\":{{\"used_percent\":10.0,\"window_minutes\":300,\"resets_at\":{soon}}},\"secondary\":null}}}}}}\n\
             {{\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"info\":null,\"rate_limits\":{{\"primary\":{{\"used_percent\":42.5,\"window_minutes\":300,\"resets_at\":{soon}}},\"secondary\":{{\"used_percent\":77.0,\"window_minutes\":10080,\"resets_at\":{past}}},\"plan_type\":\"plus\"}}}}}}\n\
             {{\"type\":\"response_item\",\"payload\":{{}}}}\n"
        );
        fs::write(day.join("rollout-test.jsonl"), log).unwrap();
        unsafe { std::env::set_var("CODEX_HOME", &home) };
        let r = codex().unwrap();
        unsafe { std::env::remove_var("CODEX_HOME") };
        let _ = fs::remove_dir_all(&home);

        assert_eq!(r.headline, "43%");
        assert_eq!(r.meters[0].label, "Session");
        assert_eq!(r.meters[0].percent, 42.5);
        assert_eq!(r.meters[1].label, "Weekly");
        // The weekly window already reset, so it reads 0.
        assert_eq!(r.meters[1].percent, 0.0);
        assert_eq!(r.lines, vec![("Plan".to_string(), "Plus".to_string())]);
    }
}
