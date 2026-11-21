//! Online widgets: revenue, stocks, analytics, developer tools, mail, social
//! and more. Each provider turns one service's API into a `Reading` (the same
//! shape the AI usage widgets use), so they share the key storage, the
//! once-a-minute pacing and the "last reading" cache in `ai_usage`.
//!
//! Keys are pasted by the person into a widget, encrypted with DPAPI, and only
//! ever sent to that one service. Services that need no key (stocks, crypto,
//! exchange rates, LeetCode) are asked with only the symbol or name chosen.

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};

use crate::ai_usage::{agent, http_error, now_ms, num, rfc3339, Item, Meter, Reading};

/// Providers this module answers for, and whether each needs a key.
const PROVIDERS: &[(&str, bool)] = &[
    ("stripe", true),
    ("polar", true),
    ("dodo", true),
    ("shopify", true),
    ("lemonsqueezy", true),
    ("paddle", true),
    ("gumroad", true),
    ("gemini", false),
    ("glm", true),
    ("opencode", false),
    ("opencode_week", false),
    ("stock", false),
    ("watchlist", false),
    ("crypto", false),
    ("currency", false),
    ("plausible", true),
    ("dub", true),
    ("resend", true),
    ("linear", true),
    ("supabase", true),
    ("github_activity", true),
    ("github_notifications", true),
    ("github_overview", true),
    ("leetcode", false),
    ("mail", true),
    ("x", true),
    ("youtube", true),
    ("instagram", true),
    ("calendar", true),
];

pub fn handles(provider: &str) -> bool {
    PROVIDERS.iter().any(|(p, _)| *p == provider)
}

pub fn fetch(provider: &str, key: Option<&str>, cfg: &Value) -> Result<Reading, String> {
    let needs_key = PROVIDERS.iter().any(|(p, k)| *p == provider && *k);
    let key = match key {
        Some(k) => k,
        None if needs_key => return Err("Add a key to start.".into()),
        None => "",
    };
    let mut r = match provider {
        "stripe" => stripe(key),
        "polar" => polar(key),
        "dodo" => dodo(key),
        "shopify" => shopify(key, cfg),
        "lemonsqueezy" => lemonsqueezy(key),
        "paddle" => paddle(key),
        "gumroad" => gumroad(key),
        "gemini" => gemini(),
        "glm" => glm(key),
        "opencode" => opencode(false),
        "opencode_week" => opencode(true),
        "stock" => stock(cfg),
        "watchlist" => watchlist(cfg),
        "crypto" => crypto(cfg),
        "currency" => currency(cfg),
        "plausible" => plausible(key, cfg),
        "dub" => dub(key),
        "resend" => resend(key),
        "linear" => linear(key),
        "supabase" => supabase(key, cfg),
        "github_activity" => github_activity(key),
        "github_notifications" => github_notifications(key),
        "github_overview" => github_overview(key),
        "leetcode" => leetcode(cfg),
        "mail" => mail(key, cfg),
        "x" => x_followers(key, cfg),
        "youtube" => youtube(key, cfg),
        "instagram" => instagram(key),
        "calendar" => calendar(key),
        other => Err(format!("Unknown service {other}.")),
    }?;
    r.provider = provider.to_string();
    r.fetched_at = now_ms();
    Ok(r)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn s<'a>(cfg: &'a Value, field: &str) -> &'a str {
    cfg.get(field).and_then(|v| v.as_str()).map(str::trim).unwrap_or("")
}

fn json_of(resp: Result<ureq::Response, ureq::Error>, who: &str) -> Result<Value, String> {
    resp.map_err(|e| http_error(who, e))?
        .into_json::<Value>()
        .map_err(|e| format!("{who} sent data this build can't read ({e})."))
}

fn now_secs() -> i64 {
    (now_ms() / 1000) as i64
}

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + if m <= 2 { 1 } else { 0 }, m, d)
}

/// This PC's offset from UTC in seconds (local minus UTC), daylight saving included.
fn local_offset() -> i64 {
    use windows::Win32::System::SystemInformation::{GetLocalTime, GetSystemTime};
    let secs = |t: windows::Win32::Foundation::SYSTEMTIME| {
        days_from_civil(t.wYear as i64, t.wMonth as i64, t.wDay as i64) * 86_400
            + t.wHour as i64 * 3600
            + t.wMinute as i64 * 60
    };
    let (l, u) = unsafe { (GetLocalTime(), GetSystemTime()) };
    let diff = secs(l) - secs(u);
    (diff as f64 / 900.0).round() as i64 * 900
}

/// Local day number (days since epoch in this PC's time zone) of a Unix time.
fn local_day(secs: i64) -> i64 {
    (secs + local_offset()).div_euclid(86_400)
}

/// "YYYY-MM-DD" for a day number.
fn ymd(day: i64) -> String {
    let (y, m, d) = civil_from_days(day);
    format!("{y:04}-{m:02}-{d:02}")
}

fn short_date(day: i64) -> String {
    const M: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let (_, m, d) = civil_from_days(day);
    format!("{} {d}", M[(m - 1) as usize])
}

/// Parse ISO 8601 / RFC 3339 times ("2026-11-03T10:20:30.5+02:00", "...Z",
/// "2026-11-03 10:20:30", "2026-11-03") into a Unix time.
pub(crate) fn parse_time(t: &str) -> Option<i64> {
    let t = t.trim();
    let b = t.as_bytes();
    if b.len() < 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let n = |r: std::ops::Range<usize>| t.get(r)?.parse::<i64>().ok();
    let day = days_from_civil(n(0..4)?, n(5..7)?, n(8..10)?);
    if b.len() < 16 {
        return Some(day * 86_400);
    }
    let mut secs = day * 86_400 + n(11..13)? * 3600 + n(14..16)? * 60;
    let mut i = 16;
    if b.len() >= 19 && b[16] == b':' {
        secs += n(17..19)?;
        i = 19;
        while i < b.len() && (b[i] == b'.' || b[i].is_ascii_digit()) {
            i += 1;
        }
    }
    match b.get(i) {
        Some(b'+') | Some(b'-') if b.len() >= i + 6 => {
            let off = n(i + 1..i + 3)? * 3600 + n(i + 4..i + 6)? * 60;
            secs -= if b[i] == b'+' { off } else { -off };
        }
        _ => {}
    }
    Some(secs)
}

fn amount(v: f64, currency: &str) -> String {
    let sym = match currency.to_ascii_uppercase().as_str() {
        "USD" => "$",
        "EUR" => "€",
        "GBP" => "£",
        "INR" => "₹",
        "JPY" => "¥",
        "CNY" => "¥",
        "AUD" => "A$",
        "CAD" => "C$",
        _ => "",
    };
    let digits = if v.abs() >= 10_000.0 { 0 } else { 2 };
    let body = group(v.abs(), digits);
    let sign = if v < 0.0 { "-" } else { "" };
    if sym.is_empty() {
        format!("{sign}{body} {}", currency.to_ascii_uppercase())
    } else {
        format!("{sign}{sym}{body}")
    }
}

/// 12345.6 -> "12,345.60"
fn group(v: f64, digits: usize) -> String {
    let s = format!("{v:.digits$}");
    let (int, frac) = s.split_once('.').map(|(a, b)| (a.to_string(), Some(b.to_string()))).unwrap_or((s.clone(), None));
    let mut out = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    match frac {
        Some(f) => format!("{out}.{f}"),
        None => out,
    }
}

/// 1234567 -> "1.2M"
fn compact(v: f64) -> String {
    let a = v.abs();
    let (d, suffix) = if a >= 1e9 {
        (v / 1e9, "B")
    } else if a >= 1e6 {
        (v / 1e6, "M")
    } else if a >= 1e4 {
        (v / 1e3, "K")
    } else {
        return group(v, 0);
    };
    format!("{}{suffix}", if d.abs() >= 100.0 { format!("{d:.0}") } else { format!("{d:.1}") })
}

fn change(now: f64, before: f64) -> String {
    if before <= 0.0 {
        return if now > 0.0 { "new".into() } else { "–".into() };
    }
    let p = (now - before) / before * 100.0;
    format!("{}{:.0}%", if p >= 0.0 { "+" } else { "" }, p)
}

/// Money received over the last 30 days, per local day.
struct Revenue {
    today: i64,
    by_currency: HashMap<String, HashMap<i64, f64>>,
    orders: u32,
}

impl Revenue {
    fn new() -> Self {
        Revenue { today: local_day(now_secs()), by_currency: HashMap::new(), orders: 0 }
    }
    fn since(&self) -> i64 {
        // Midnight 29 days ago, local time, as a Unix time.
        (self.today - 29) * 86_400 - local_offset()
    }
    fn add(&mut self, when: i64, value: f64, currency: &str) {
        let day = local_day(when);
        if day < self.today - 29 || day > self.today {
            return;
        }
        self.orders += 1;
        *self
            .by_currency
            .entry(currency.to_ascii_uppercase())
            .or_default()
            .entry(day)
            .or_default() += value;
    }
    fn reading(self, service: &str, what: &str) -> Reading {
        // Report the currency with the most money in it; mention the others.
        let mut cur: Vec<(String, f64)> =
            self.by_currency.iter().map(|(c, d)| (c.clone(), d.values().map(|v| v.abs()).sum())).collect();
        cur.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let currency = cur.first().map(|c| c.0.clone()).unwrap_or_else(|| "USD".into());
        let days = self.by_currency.get(&currency).cloned().unwrap_or_default();
        let at = |d: i64| days.get(&d).copied().unwrap_or(0.0);
        let total: f64 = (0..30).map(|i| at(self.today - i)).sum();
        let week: f64 = (0..7).map(|i| at(self.today - i)).sum();
        let prev: f64 = (7..14).map(|i| at(self.today - i)).sum();
        let series: Vec<f64> = (0..14).rev().map(|i| at(self.today - i)).collect();
        let labels: Vec<String> = (0..14).rev().map(|i| short_date(self.today - i)).collect();
        let mut lines = vec![
            ("Today".to_string(), amount(at(self.today), &currency)),
            ("Last 7 days".to_string(), amount(week, &currency)),
            ("Change on the week before".to_string(), change(week, prev)),
            ("Payments, 30 days".to_string(), self.orders.to_string()),
        ];
        if cur.len() > 1 {
            lines.push((
                "Not counted".to_string(),
                cur[1..].iter().map(|c| c.0.clone()).collect::<Vec<_>>().join(", "),
            ));
        }
        Reading {
            headline: amount(total, &currency),
            caption: format!("{service} {what}, last 30 days"),
            lines,
            series,
            series_labels: labels,
            value: Some(total),
            currency: Some(currency),
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------------------
// Revenue
// ---------------------------------------------------------------------------

/// Stripe: the balance history, which already has fees and refunds taken off.
fn stripe(key: &str) -> Result<Reading, String> {
    let mut rev = Revenue::new();
    let since = rev.since().to_string();
    let mut after: Option<String> = None;
    for _ in 0..30 {
        let mut req = agent()
            .get("https://api.stripe.com/v1/balance_transactions")
            .set("Authorization", &format!("Bearer {key}"))
            .query("limit", "100")
            .query("created[gte]", &since);
        if let Some(a) = &after {
            req = req.query("starting_after", a);
        }
        let body = json_of(req.call(), "Stripe")?;
        let data = body["data"].as_array().cloned().unwrap_or_default();
        for tx in &data {
            let kind = tx["type"].as_str().unwrap_or("");
            if kind.starts_with("payout") || kind.starts_with("transfer") || kind.starts_with("topup") {
                continue;
            }
            let net = tx["net"].as_f64().unwrap_or(0.0) / 100.0;
            rev.add(tx["created"].as_i64().unwrap_or(0), net, tx["currency"].as_str().unwrap_or("usd"));
        }
        if body["has_more"].as_bool() != Some(true) || data.is_empty() {
            break;
        }
        after = data.last().and_then(|t| t["id"].as_str()).map(String::from);
    }
    Ok(rev.reading("Stripe", "net revenue"))
}

/// Polar: its own revenue metrics, per day.
fn polar(key: &str) -> Result<Reading, String> {
    let rev_tmp = Revenue::new();
    let start = ymd(rev_tmp.today - 29);
    let end = ymd(rev_tmp.today);
    let body = json_of(
        agent()
            .get("https://api.polar.sh/v1/metrics/")
            .set("Authorization", &format!("Bearer {key}"))
            .query("start_date", &start)
            .query("end_date", &end)
            .query("interval", "day")
            .call(),
        "Polar",
    )?;
    let mut rev = Revenue::new();
    for p in body["periods"].as_array().cloned().unwrap_or_default() {
        let when = p["timestamp"].as_str().and_then(parse_time).unwrap_or(0) + 12 * 3600;
        let cents = num(&p, "net_revenue").or_else(|| num(&p, "revenue")).unwrap_or(0.0);
        if cents != 0.0 {
            rev.add(when, cents / 100.0, "usd");
        }
    }
    let orders = body["totals"]["orders"].as_u64();
    let mut r = rev.reading("Polar", "net revenue");
    if let Some(o) = orders {
        if let Some(line) = r.lines.iter_mut().find(|l| l.0.starts_with("Payments")) {
            line.0 = "Orders, 30 days".into();
            line.1 = o.to_string();
        }
    }
    Ok(r)
}

/// Dodo Payments: succeeded payments.
fn dodo(key: &str) -> Result<Reading, String> {
    let mut rev = Revenue::new();
    let since = rfc3339(rev.since());
    for base in ["https://live.dodopayments.com", "https://test.dodopayments.com"] {
        let mut ok = false;
        for page in 0..20 {
            let resp = agent()
                .get(&format!("{base}/payments"))
                .set("Authorization", &format!("Bearer {key}"))
                .query("page_size", "100")
                .query("page_number", &page.to_string())
                .query("created_at_gte", &since)
                .call();
            if base.contains("live") && matches!(&resp, Err(ureq::Error::Status(401, _))) {
                break; // maybe a test-mode key
            }
            let body = json_of(resp, "Dodo Payments")?;
            ok = true;
            let items = body["items"].as_array().cloned().unwrap_or_default();
            for p in &items {
                if p["status"].as_str() != Some("succeeded") {
                    continue;
                }
                let when = p["created_at"].as_str().and_then(parse_time).unwrap_or(0);
                rev.add(when, p["total_amount"].as_f64().unwrap_or(0.0) / 100.0, p["currency"].as_str().unwrap_or("USD"));
            }
            if items.len() < 100 {
                break;
            }
        }
        if ok {
            return Ok(rev.reading("Dodo", "payments"));
        }
    }
    Err("Dodo Payments refused the key. Check it and paste it again.".into())
}

/// Shopify: orders from the Admin GraphQL API (a custom app's access token).
fn shopify(key: &str, cfg: &Value) -> Result<Reading, String> {
    let mut shop = s(cfg, "shop").trim_start_matches("https://").trim_end_matches('/').to_string();
    if shop.is_empty() {
        return Err("Add your store's address (yourstore.myshopify.com) in the widget.".into());
    }
    if !shop.contains('.') {
        shop.push_str(".myshopify.com");
    }
    let mut rev = Revenue::new();
    let since = ymd(rev.today - 29);
    let mut cursor: Option<String> = None;
    for _ in 0..20 {
        let q = "query($q:String!,$c:String){orders(first:250,after:$c,query:$q){pageInfo{hasNextPage endCursor} nodes{createdAt currentTotalPriceSet{shopMoney{amount currencyCode}}}}}";
        let body = json_of(
            agent()
                .post(&format!("https://{shop}/admin/api/2025-07/graphql.json"))
                .set("X-Shopify-Access-Token", key)
                .send_json(json!({"query": q, "variables": {"q": format!("created_at:>={since}"), "c": cursor}})),
            "Shopify",
        )?;
        if let Some(e) = body["errors"].as_array().and_then(|a| a.first()) {
            return Err(format!("Shopify: {}", e["message"].as_str().unwrap_or("the request was refused")));
        }
        let orders = &body["data"]["orders"];
        for o in orders["nodes"].as_array().cloned().unwrap_or_default() {
            let m = &o["currentTotalPriceSet"]["shopMoney"];
            let v = m["amount"].as_str().and_then(|a| a.parse::<f64>().ok()).unwrap_or(0.0);
            let when = o["createdAt"].as_str().and_then(parse_time).unwrap_or(0);
            rev.add(when, v, m["currencyCode"].as_str().unwrap_or("USD"));
        }
        if orders["pageInfo"]["hasNextPage"].as_bool() != Some(true) {
            break;
        }
        cursor = orders["pageInfo"]["endCursor"].as_str().map(String::from);
    }
    Ok(rev.reading("Shopify", "sales"))
}

/// Lemon Squeezy: orders after tax and refunds.
fn lemonsqueezy(key: &str) -> Result<Reading, String> {
    let mut rev = Revenue::new();
    let since = rev.since();
    for page in 1..=30 {
        let body = json_of(
            agent()
                .get("https://api.lemonsqueezy.com/v1/orders")
                .set("Authorization", &format!("Bearer {key}"))
                .set("Accept", "application/vnd.api+json")
                .query("page[size]", "100")
                .query("page[number]", &page.to_string())
                .query("sort", "-created_at")
                .call(),
            "Lemon Squeezy",
        )?;
        let data = body["data"].as_array().cloned().unwrap_or_default();
        let mut older = 0;
        for o in &data {
            let a = &o["attributes"];
            let when = a["created_at"].as_str().and_then(parse_time).unwrap_or(0);
            if when < since {
                older += 1;
                continue;
            }
            if a["status"].as_str() == Some("pending") || a["status"].as_str() == Some("failed") {
                continue;
            }
            let cents = a["total"].as_f64().unwrap_or(0.0) - a["tax"].as_f64().unwrap_or(0.0)
                - a["refunded_amount"].as_f64().unwrap_or(0.0);
            rev.add(when, cents / 100.0, a["currency"].as_str().unwrap_or("USD"));
        }
        let last = body["meta"]["page"]["lastPage"].as_u64().unwrap_or(1);
        if data.is_empty() || older == data.len() || page as u64 >= last {
            break;
        }
    }
    Ok(rev.reading("Lemon Squeezy", "revenue"))
}

/// Paddle Billing: completed transactions, your earnings after Paddle's fee.
fn paddle(key: &str) -> Result<Reading, String> {
    let mut rev = Revenue::new();
    let since = rfc3339(rev.since());
    let mut url = Some(format!(
        "https://api.paddle.com/transactions?status=completed&per_page=200&created_at[GTE]={since}"
    ));
    let mut pages = 0;
    while let Some(u) = url.take() {
        pages += 1;
        let body = json_of(agent().get(&u).set("Authorization", &format!("Bearer {key}")).call(), "Paddle")?;
        for t in body["data"].as_array().cloned().unwrap_or_default() {
            let totals = &t["details"]["totals"];
            let minor = totals["earnings"].as_str().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
            let cur = t["currency_code"].as_str().unwrap_or("USD");
            let div = if matches!(cur, "JPY" | "KRW") { 1.0 } else { 100.0 };
            let when = t["created_at"].as_str().and_then(parse_time).unwrap_or(0);
            rev.add(when, minor / div, cur);
        }
        let p = &body["meta"]["pagination"];
        if p["has_more"].as_bool() == Some(true) && pages < 20 {
            url = p["next"].as_str().map(String::from);
        }
    }
    Ok(rev.reading("Paddle", "earnings"))
}

/// Gumroad: sales minus Gumroad's fee, refunds left out.
fn gumroad(key: &str) -> Result<Reading, String> {
    let mut rev = Revenue::new();
    let after = ymd(rev.today - 30);
    let mut page_key: Option<String> = None;
    for _ in 0..30 {
        let mut req = agent()
            .get("https://api.gumroad.com/v2/sales")
            .set("Authorization", &format!("Bearer {key}"))
            .query("after", &after);
        if let Some(k) = &page_key {
            req = req.query("page_key", k);
        }
        let body = json_of(req.call(), "Gumroad")?;
        for sale in body["sales"].as_array().cloned().unwrap_or_default() {
            if sale["refunded"].as_bool() == Some(true) || sale["chargedback"].as_bool() == Some(true) {
                continue;
            }
            let cents = sale["price"].as_f64().unwrap_or(0.0) - sale["gumroad_fee"].as_f64().unwrap_or(0.0);
            let when = sale["created_at"].as_str().and_then(parse_time).unwrap_or(0);
            let cur = sale["currency"].as_str().unwrap_or("usd");
            rev.add(when, cents / 100.0, cur);
        }
        page_key = body["next_page_key"].as_str().map(String::from);
        if page_key.is_none() {
            break;
        }
    }
    Ok(rev.reading("Gumroad", "earnings"))
}

// ---------------------------------------------------------------------------
// AI
// ---------------------------------------------------------------------------

fn home() -> PathBuf {
    std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default()
}

/// Gemini CLI quota, read with Gemini CLI's own sign-in on this PC (like the
/// Claude widget). The token is used for two requests to Google and never
/// stored or shown. These are the endpoints Gemini CLI itself uses for /stats,
/// so they can change without notice.
fn gemini() -> Result<Reading, String> {
    let file = home().join(".gemini").join("oauth_creds.json");
    let creds: Value = serde_json::from_str(
        &fs::read_to_string(&file).map_err(|_| "No Gemini CLI sign-in was found. Sign in to Gemini CLI with Google first.")?,
    )
    .map_err(|_| "Gemini CLI's sign-in file couldn't be read.")?;
    let token = creds["access_token"].as_str().ok_or("Gemini CLI's sign-in has no token. Run gemini once.")?;
    if let Some(exp) = creds["expiry_date"].as_f64() {
        if (exp as u64) < now_ms() {
            return Err("Gemini CLI's sign-in has expired. Run gemini once to renew it.".into());
        }
    }
    let base = "https://cloudcode-pa.googleapis.com/v1internal";
    let load = json_of(
        agent()
            .post(&format!("{base}:loadCodeAssist"))
            .set("Authorization", &format!("Bearer {token}"))
            .send_json(json!({"metadata": {"ideType": "IDE_UNSPECIFIED", "platform": "PLATFORM_UNSPECIFIED", "pluginType": "GEMINI"}})),
        "Gemini",
    )?;
    let project = load["cloudaicompanionProject"]
        .as_str()
        .map(String::from)
        .or_else(|| load["cloudaicompanionProject"]["id"].as_str().map(String::from));
    let mut body = json!({});
    if let Some(p) = &project {
        body["project"] = json!(p);
    }
    let quota = json_of(
        agent()
            .post(&format!("{base}:retrieveUserQuota"))
            .set("Authorization", &format!("Bearer {token}"))
            .send_json(body),
        "Gemini",
    )?;
    let mut best: HashMap<String, Meter> = HashMap::new();
    for b in quota["buckets"].as_array().cloned().unwrap_or_default() {
        let model = b["modelId"].as_str().unwrap_or("");
        let label = if model.contains("pro") {
            "Pro"
        } else if model.contains("flash-lite") {
            "Flash Lite"
        } else if model.contains("flash") {
            "Flash"
        } else {
            continue;
        };
        let used = (1.0 - b["remainingFraction"].as_f64().unwrap_or(1.0)) * 100.0;
        let m = Meter { label: label.into(), percent: used.clamp(0.0, 100.0), resets_at: b["resetTime"].as_str().map(String::from) };
        let keep = best.get(label).map(|o| o.percent < m.percent).unwrap_or(true);
        if keep {
            best.insert(label.into(), m);
        }
    }
    if best.is_empty() {
        return Err("Gemini didn't report any quota for this account.".into());
    }
    let mut meters: Vec<Meter> = best.into_values().collect();
    let rank = |l: &str| match l {
        "Pro" => 0,
        "Flash" => 1,
        _ => 2,
    };
    meters.sort_by_key(|m| rank(&m.label));
    let tier = load["currentTier"]["name"].as_str().unwrap_or("").to_string();
    let mut lines = vec![];
    if !tier.is_empty() {
        lines.push(("Plan".into(), tier));
    }
    Ok(Reading {
        headline: format!("{}%", meters[0].percent.round()),
        caption: format!("Gemini CLI, {} used", meters[0].label),
        meters,
        lines,
        ..Default::default()
    })
}

/// Z.ai (GLM) Coding Plan limits, with the plan's API key.
fn glm(key: &str) -> Result<Reading, String> {
    let mut last = String::new();
    for host in ["https://api.z.ai", "https://open.bigmodel.cn"] {
        let resp = agent()
            .get(&format!("{host}/api/monitor/usage/quota/limit"))
            .set("Authorization", &format!("Bearer {key}"))
            .set("Accept-Language", "en-US,en")
            .call();
        let body = match json_of(resp, "Z.ai") {
            Ok(b) => b,
            Err(e) => {
                last = e;
                continue;
            }
        };
        let data = if body["data"].is_object() { &body["data"] } else { &body };
        let mut meters = vec![];
        for l in data["limits"].as_array().cloned().unwrap_or_default() {
            let label = match l["type"].as_str().unwrap_or("") {
                "TOKENS_LIMIT" => "Session",
                "TIME_LIMIT" => "Tools",
                other => other,
            };
            let pct = num(&l, "percentage").unwrap_or_else(|| {
                let cur = num(&l, "currentValue").unwrap_or(0.0);
                let total = num(&l, "usage").or_else(|| num(&l, "total")).unwrap_or(0.0);
                if total > 0.0 { cur / total * 100.0 } else { 0.0 }
            });
            let reset = l["nextResetTime"].as_i64().map(|ms| rfc3339(ms / 1000));
            meters.push(Meter { label: label.into(), percent: pct.clamp(0.0, 100.0), resets_at: reset });
        }
        if meters.is_empty() {
            last = body["msg"].as_str().unwrap_or("Z.ai didn't report any Coding Plan limits for this key.").to_string();
            continue;
        }
        meters.sort_by_key(|m| if m.label == "Session" { 0 } else { 1 });
        let mut lines = vec![];
        if let Some(level) = data["level"].as_str() {
            lines.push(("Plan".into(), level.to_string()));
        }
        return Ok(Reading {
            headline: format!("{}%", meters[0].percent.round()),
            caption: "GLM Coding Plan, session".into(),
            meters,
            lines,
            ..Default::default()
        });
    }
    Err(last)
}

fn opencode_dirs() -> Vec<PathBuf> {
    let mut v = vec![];
    if let Some(x) = std::env::var_os("XDG_DATA_HOME") {
        v.push(PathBuf::from(x).join("opencode").join("storage").join("message"));
    }
    v.push(home().join(".local").join("share").join("opencode").join("storage").join("message"));
    if let Some(a) = std::env::var_os("LOCALAPPDATA") {
        v.push(PathBuf::from(a).join("opencode").join("storage").join("message"));
    }
    v.into_iter().filter(|p| p.is_dir()).collect()
}

pub fn opencode_found() -> bool {
    !opencode_dirs().is_empty()
}

/// OpenCode token use, from the messages OpenCode keeps on this PC.
fn opencode(week_view: bool) -> Result<Reading, String> {
    let dirs = opencode_dirs();
    if dirs.is_empty() {
        return Err("No OpenCode history was found on this PC.".into());
    }
    let now = now_secs();
    let week_ago = std::time::SystemTime::now() - Duration::from_secs(7 * 86_400);
    let today = local_day(now);
    let (mut day_tokens, mut week_tokens, mut week_cost) = (0f64, 0f64, 0f64);
    let mut by_model: HashMap<String, f64> = HashMap::new();
    let mut sessions: HashMap<String, (i64, f64)> = HashMap::new();
    let mut daily = vec![0f64; 7];
    for dir in dirs {
        for session in fs::read_dir(dir).into_iter().flatten().flatten() {
            let fresh = session.metadata().and_then(|m| m.modified()).map(|t| t > week_ago).unwrap_or(false);
            if !fresh {
                continue;
            }
            for f in fs::read_dir(session.path()).into_iter().flatten().flatten() {
                let Ok(text) = fs::read_to_string(f.path()) else { continue };
                let Ok(m) = serde_json::from_str::<Value>(&text) else { continue };
                if m["role"].as_str() != Some("assistant") {
                    continue;
                }
                let created = m["time"]["created"].as_i64().unwrap_or(0) / 1000;
                if created < now - 7 * 86_400 {
                    continue;
                }
                let t = &m["tokens"];
                let tokens = num(t, "input").unwrap_or(0.0)
                    + num(t, "output").unwrap_or(0.0)
                    + num(t, "reasoning").unwrap_or(0.0)
                    + num(&t["cache"], "read").unwrap_or(0.0)
                    + num(&t["cache"], "write").unwrap_or(0.0);
                week_tokens += tokens;
                week_cost += num(&m, "cost").unwrap_or(0.0);
                let day = local_day(created);
                if day == today {
                    day_tokens += tokens;
                }
                if let Some(slot) = daily.get_mut((6 - (today - day)).clamp(0, 6) as usize) {
                    *slot += tokens;
                }
                let model = m["modelID"].as_str().unwrap_or("unknown").to_string();
                *by_model.entry(model).or_default() += tokens;
                let sid = m["sessionID"].as_str().unwrap_or("").to_string();
                let e = sessions.entry(sid).or_insert((0, 0.0));
                e.0 = e.0.max(created);
                e.1 += tokens;
            }
        }
    }
    let latest = sessions.values().max_by_key(|s| s.0).map(|s| s.1).unwrap_or(0.0);
    let top = by_model.iter().max_by(|a, b| a.1.partial_cmp(b.1).unwrap()).map(|m| m.0.clone()).unwrap_or_else(|| "–".into());
    let mut lines = vec![
        ("Latest session".to_string(), format!("{} tokens", compact(latest))),
        ("Today".to_string(), format!("{} tokens", compact(day_tokens))),
        ("Last 7 days".to_string(), format!("{} tokens", compact(week_tokens))),
        ("Top model".to_string(), top),
    ];
    if week_cost > 0.0 {
        lines.push(("Cost, 7 days".into(), format!("${week_cost:.2}")));
    }
    let (headline, caption) = if week_view {
        (compact(week_tokens), "OpenCode tokens, last 7 days")
    } else {
        (compact(latest), "OpenCode tokens, latest session")
    };
    Ok(Reading {
        headline,
        caption: caption.into(),
        lines,
        series: daily,
        value: Some(if week_view { week_tokens } else { latest }),
        ..Default::default()
    })
}

// ---------------------------------------------------------------------------
// Stocks, crypto, currency
// ---------------------------------------------------------------------------

fn browser_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36")
        .build()
}

struct Quote {
    symbol: String,
    price: f64,
    prev: f64,
    currency: String,
    closes: Vec<f64>,
    name: String,
}

/// Yahoo Finance's public chart data for one symbol, today in 5-minute steps.
fn quote(symbol: &str) -> Result<Quote, String> {
    let sym: String = symbol.trim().to_ascii_uppercase().chars().filter(|c| c.is_ascii_alphanumeric() || ".-^=".contains(*c)).collect();
    if sym.is_empty() {
        return Err("Add a ticker symbol in the widget.".into());
    }
    let body = json_of(
        browser_agent()
            .get(&format!("https://query1.finance.yahoo.com/v8/finance/chart/{sym}"))
            .query("range", "1d")
            .query("interval", "5m")
            .call(),
        "Yahoo Finance",
    )?;
    let r = &body["chart"]["result"][0];
    if r.is_null() {
        return Err(format!("No prices found for {sym}."));
    }
    let meta = &r["meta"];
    let closes: Vec<f64> = r["indicators"]["quote"][0]["close"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_f64()).collect())
        .unwrap_or_default();
    let price = num(meta, "regularMarketPrice").or(closes.last().copied()).unwrap_or(0.0);
    let prev = num(meta, "chartPreviousClose").or_else(|| num(meta, "previousClose")).unwrap_or(price);
    Ok(Quote {
        symbol: sym,
        price,
        prev,
        currency: meta["currency"].as_str().unwrap_or("USD").to_string(),
        closes,
        name: meta["shortName"].as_str().or(meta["longName"].as_str()).unwrap_or("").to_string(),
    })
}

fn pct_change(price: f64, prev: f64) -> f64 {
    if prev == 0.0 { 0.0 } else { (price - prev) / prev * 100.0 }
}

fn signed(p: f64) -> String {
    format!("{}{:.2}%", if p >= 0.0 { "+" } else { "" }, p)
}

fn stock(cfg: &Value) -> Result<Reading, String> {
    let sym = if s(cfg, "symbol").is_empty() { "AAPL" } else { s(cfg, "symbol") };
    let q = quote(sym)?;
    let p = pct_change(q.price, q.prev);
    Ok(Reading {
        headline: amount(q.price, &q.currency),
        caption: format!("{} {}", q.symbol, signed(p)),
        lines: vec![
            ("Name".into(), if q.name.is_empty() { q.symbol.clone() } else { q.name.clone() }),
            ("Change today".into(), format!("{} ({})", amount(q.price - q.prev, &q.currency), signed(p))),
            ("Previous close".into(), amount(q.prev, &q.currency)),
        ],
        series: q.closes,
        value: Some(p),
        currency: Some(q.currency),
        ..Default::default()
    })
}

fn watchlist(cfg: &Value) -> Result<Reading, String> {
    let list = if s(cfg, "symbols").is_empty() { "AAPL, MSFT, NVDA" } else { s(cfg, "symbols") };
    let mut items = vec![];
    let mut errors = vec![];
    for sym in list.split([',', ' ']).filter(|x| !x.trim().is_empty()).take(6) {
        match quote(sym) {
            Ok(q) => items.push(Item {
                title: q.symbol.clone(),
                detail: signed(pct_change(q.price, q.prev)),
                value: amount(q.price, &q.currency),
                url: None,
            }),
            Err(e) => errors.push(e),
        }
    }
    if items.is_empty() {
        return Err(errors.into_iter().next().unwrap_or_else(|| "Add ticker symbols in the widget.".into()));
    }
    Ok(Reading {
        headline: items[0].value.clone(),
        caption: items.iter().map(|i| i.title.as_str()).collect::<Vec<_>>().join(" · "),
        items,
        ..Default::default()
    })
}

/// Binance public market data: 24-hour ticker and hourly closes.
fn crypto(cfg: &Value) -> Result<Reading, String> {
    let mut sym: String = s(cfg, "symbol").to_ascii_uppercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    if sym.is_empty() {
        sym = "BTC".into();
    }
    if !["USDT", "USDC", "FDUSD", "EUR", "BTC", "ETH"].iter().any(|q| sym.len() > q.len() && sym.ends_with(q)) {
        sym.push_str("USDT");
    }
    let mut last = String::new();
    for host in ["https://data-api.binance.vision", "https://api.binance.com"] {
        let t = match json_of(agent().get(&format!("{host}/api/v3/ticker/24hr")).query("symbol", &sym).call(), "Binance") {
            Ok(t) => t,
            Err(e) => {
                last = e;
                continue;
            }
        };
        let f = |k: &str| t[k].as_str().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
        let k = json_of(
            agent().get(&format!("{host}/api/v3/klines")).query("symbol", &sym).query("interval", "1h").query("limit", "24").call(),
            "Binance",
        )
        .unwrap_or(Value::Null);
        let closes: Vec<f64> = k
            .as_array()
            .map(|a| a.iter().filter_map(|c| c[4].as_str()?.parse::<f64>().ok()).collect())
            .unwrap_or_default();
        let price = f("lastPrice");
        let quote_ccy = if sym.ends_with("EUR") { "EUR" } else { "USD" };
        let digits = if price >= 100.0 { 2 } else if price >= 1.0 { 3 } else { 6 };
        let shown = if quote_ccy == "EUR" { format!("€{}", group(price, digits)) } else { format!("${}", group(price, digits)) };
        return Ok(Reading {
            headline: shown,
            caption: format!("{sym} {}", signed(f("priceChangePercent"))),
            lines: vec![
                ("24h change".into(), signed(f("priceChangePercent"))),
                ("24h high".into(), group(f("highPrice"), digits)),
                ("24h low".into(), group(f("lowPrice"), digits)),
                ("24h volume".into(), compact(f("quoteVolume"))),
            ],
            series: closes,
            value: Some(f("priceChangePercent")),
            ..Default::default()
        });
    }
    Err(last)
}

/// Exchange rates from the European Central Bank, via Frankfurter (no key).
fn currency(cfg: &Value) -> Result<Reading, String> {
    let from = if s(cfg, "from").is_empty() { "USD".to_string() } else { s(cfg, "from").to_ascii_uppercase() };
    let to = if s(cfg, "to").is_empty() { "EUR".to_string() } else { s(cfg, "to").to_ascii_uppercase() };
    let amt = cfg.get("amount").and_then(|v| v.as_f64()).filter(|v| *v > 0.0).unwrap_or(1.0);
    let today = local_day(now_secs());
    let start = ymd(today - 30);
    let mut last = String::new();
    for base in ["https://api.frankfurter.dev/v1", "https://api.frankfurter.app"] {
        let body = match json_of(agent().get(&format!("{base}/{start}..")).query("base", &from).query("symbols", &to).call(), "Frankfurter") {
            Ok(b) => b,
            Err(e) => {
                last = e;
                continue;
            }
        };
        let mut days: Vec<(String, f64)> = body["rates"]
            .as_object()
            .map(|m| m.iter().filter_map(|(d, r)| Some((d.clone(), r[&to].as_f64()?))).collect())
            .unwrap_or_default();
        days.sort_by(|a, b| a.0.cmp(&b.0));
        let Some((date, rate)) = days.last().cloned() else {
            return Err(format!("No rate for {from} to {to}."));
        };
        let first = days.first().map(|d| d.1).unwrap_or(rate);
        return Ok(Reading {
            headline: format!("{} {to}", group(amt * rate, if amt * rate >= 1000.0 { 0 } else { 2 })),
            caption: format!("{} {from} in {to}", group(amt, if amt.fract() == 0.0 { 0 } else { 2 })),
            lines: vec![
                (format!("1 {from}"), format!("{rate:.4} {to}")),
                (format!("1 {to}"), format!("{:.4} {from}", 1.0 / rate)),
                ("30-day change".into(), signed(pct_change(rate, first))),
                ("Rate date".into(), date),
            ],
            series: days.iter().map(|d| d.1).collect(),
            series_labels: days.iter().map(|d| d.0.clone()).collect(),
            value: Some(amt * rate),
            ..Default::default()
        });
    }
    Err(last)
}

// ---------------------------------------------------------------------------
// Analytics
// ---------------------------------------------------------------------------

fn plausible(key: &str, cfg: &Value) -> Result<Reading, String> {
    let site = s(cfg, "site").trim_start_matches("https://").trim_end_matches('/');
    if site.is_empty() {
        return Err("Add your site's domain in the widget.".into());
    }
    let base = if s(cfg, "base").is_empty() { "https://plausible.io" } else { s(cfg, "base").trim_end_matches('/') };
    let today = local_day(now_secs());
    let body = json_of(
        agent()
            .post(&format!("{base}/api/v2/query"))
            .set("Authorization", &format!("Bearer {key}"))
            .send_json(json!({
                "site_id": site,
                "metrics": ["visitors", "pageviews"],
                "date_range": [ymd(today - 13), ymd(today)],
                "dimensions": ["time:day"],
            })),
        "Plausible",
    )?;
    let mut by_day: HashMap<String, (f64, f64)> = HashMap::new();
    for row in body["results"].as_array().cloned().unwrap_or_default() {
        let d = row["dimensions"][0].as_str().unwrap_or("").get(0..10).unwrap_or("").to_string();
        let m = &row["metrics"];
        by_day.insert(d, (m[0].as_f64().unwrap_or(0.0), m[1].as_f64().unwrap_or(0.0)));
    }
    let at = |i: i64| by_day.get(&ymd(today - i)).copied().unwrap_or((0.0, 0.0));
    let series: Vec<f64> = (0..14).rev().map(|i| at(i).0).collect();
    let week: f64 = (0..7).map(|i| at(i).0).sum();
    let prev: f64 = (7..14).map(|i| at(i).0).sum();
    let views: f64 = (0..7).map(|i| at(i).1).sum();
    Ok(Reading {
        headline: compact(week),
        caption: format!("{site}, visitors this week"),
        lines: vec![
            ("Today".into(), compact(at(0).0)),
            ("Pageviews, 7 days".into(), compact(views)),
            ("Change on the week before".into(), change(week, prev)),
        ],
        series,
        series_labels: (0..14).rev().map(|i| short_date(today - i)).collect(),
        value: Some(week),
        ..Default::default()
    })
}

fn dub(key: &str) -> Result<Reading, String> {
    let body = json_of(
        agent()
            .get("https://api.dub.co/analytics")
            .set("Authorization", &format!("Bearer {key}"))
            .query("event", "clicks")
            .query("groupBy", "timeseries")
            .query("interval", "30d")
            .call(),
        "Dub",
    )?;
    let rows = body.as_array().cloned().unwrap_or_default();
    let clicks: Vec<f64> = rows.iter().map(|r| r["clicks"].as_f64().unwrap_or(0.0)).collect();
    let n = clicks.len();
    let tail = |a: usize, b: usize| clicks[n.saturating_sub(b)..n.saturating_sub(a)].iter().sum::<f64>();
    let week = tail(0, 7);
    let prev = tail(7, 14);
    Ok(Reading {
        headline: compact(week),
        caption: "Dub link clicks this week".into(),
        lines: vec![
            ("Last 30 days".into(), compact(clicks.iter().sum())),
            ("Change on the week before".into(), change(week, prev)),
        ],
        series: clicks[n.saturating_sub(14)..].to_vec(),
        value: Some(week),
        ..Default::default()
    })
}

// ---------------------------------------------------------------------------
// Developer
// ---------------------------------------------------------------------------

fn resend(key: &str) -> Result<Reading, String> {
    let since = now_secs() - 7 * 86_400;
    let (mut sent, mut delivered, mut bounced) = (0u32, 0u32, 0u32);
    let mut after: Option<String> = None;
    let mut items = vec![];
    'pages: for _ in 0..10 {
        let mut req = agent().get("https://api.resend.com/emails").set("Authorization", &format!("Bearer {key}")).query("limit", "100");
        if let Some(a) = &after {
            req = req.query("after", a);
        }
        let body = json_of(req.call(), "Resend")?;
        let data = body["data"].as_array().cloned().unwrap_or_default();
        for e in &data {
            let when = e["created_at"].as_str().and_then(parse_time).unwrap_or(0);
            if when < since {
                break 'pages;
            }
            sent += 1;
            match e["last_event"].as_str().unwrap_or("") {
                "delivered" | "opened" | "clicked" => delivered += 1,
                "bounced" | "complained" | "failed" => bounced += 1,
                _ => {}
            }
            if items.len() < 6 {
                items.push(Item {
                    title: e["subject"].as_str().unwrap_or("(no subject)").to_string(),
                    detail: e["to"][0].as_str().unwrap_or("").to_string(),
                    value: e["last_event"].as_str().unwrap_or("").replace('_', " "),
                    url: None,
                });
            }
        }
        if body["has_more"].as_bool() != Some(true) || data.is_empty() {
            break;
        }
        after = data.last().and_then(|e| e["id"].as_str()).map(String::from);
    }
    let rate = if sent > 0 { delivered as f64 / sent as f64 * 100.0 } else { 0.0 };
    Ok(Reading {
        headline: compact(sent as f64),
        caption: "Resend emails sent this week".into(),
        lines: vec![
            ("Delivered".into(), format!("{delivered} ({rate:.0}%)")),
            ("Bounced or failed".into(), bounced.to_string()),
        ],
        items,
        value: Some(sent as f64),
        ..Default::default()
    })
}

fn linear(key: &str) -> Result<Reading, String> {
    let q = r#"{ viewer { name assignedIssues(first: 50, orderBy: updatedAt, filter: { state: { type: { nin: ["completed", "canceled"] } } }) { nodes { identifier title url priority state { name type } } } } }"#;
    let body = json_of(
        agent().post("https://api.linear.app/graphql").set("Authorization", key).send_json(json!({ "query": q })),
        "Linear",
    )?;
    if let Some(e) = body["errors"].as_array().and_then(|a| a.first()) {
        return Err(format!("Linear: {}", e["message"].as_str().unwrap_or("the request was refused")));
    }
    let nodes = body["data"]["viewer"]["assignedIssues"]["nodes"].as_array().cloned().unwrap_or_default();
    let started = nodes.iter().filter(|n| n["state"]["type"].as_str() == Some("started")).count();
    let mut sorted = nodes.clone();
    sorted.sort_by_key(|n| if n["state"]["type"].as_str() == Some("started") { 0 } else { 1 });
    let items = sorted
        .iter()
        .take(8)
        .map(|n| Item {
            title: n["title"].as_str().unwrap_or("").to_string(),
            detail: n["identifier"].as_str().unwrap_or("").to_string(),
            value: n["state"]["name"].as_str().unwrap_or("").to_string(),
            url: n["url"].as_str().map(String::from),
        })
        .collect();
    Ok(Reading {
        headline: nodes.len().to_string(),
        caption: "Linear issues assigned to you".into(),
        lines: vec![("In progress".into(), started.to_string()), ("Not started".into(), (nodes.len() - started).to_string())],
        items,
        value: Some(nodes.len() as f64),
        ..Default::default()
    })
}

fn supabase(key: &str, cfg: &Value) -> Result<Reading, String> {
    let r: String = s(cfg, "project").chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    if r.is_empty() {
        return Err("Add your project's reference ID in the widget.".into());
    }
    let base = format!("https://api.supabase.com/v1/projects/{r}");
    let auth = format!("Bearer {key}");
    let project = json_of(agent().get(&base).set("Authorization", &auth).call(), "Supabase")?;
    let health = json_of(
        agent()
            .get(&format!("{base}/health?services=db&services=auth&services=rest&services=storage&services=realtime"))
            .set("Authorization", &auth)
            .call(),
        "Supabase",
    )
    .unwrap_or(Value::Null);
    let usage = json_of(
        agent().get(&format!("{base}/analytics/endpoints/usage.api-counts")).query("interval", "1day").set("Authorization", &auth).call(),
        "Supabase",
    )
    .unwrap_or(Value::Null);
    let rows = usage["result"].as_array().cloned().unwrap_or_default();
    let series: Vec<f64> = rows
        .iter()
        .map(|r| {
            ["total_rest_requests", "total_auth_requests", "total_storage_requests", "total_realtime_requests"]
                .iter()
                .map(|k| num(r, k).unwrap_or(0.0))
                .sum()
        })
        .collect();
    let total: f64 = series.iter().sum();
    let services = health.as_array().cloned().unwrap_or_default();
    let unhealthy: Vec<String> = services
        .iter()
        .filter(|s| s["healthy"].as_bool() == Some(false))
        .map(|s| s["name"].as_str().unwrap_or("").to_string())
        .collect();
    let status = project["status"].as_str().unwrap_or("");
    let state = if !unhealthy.is_empty() {
        format!("Issues: {}", unhealthy.join(", "))
    } else if status == "ACTIVE_HEALTHY" || (!services.is_empty() && status.is_empty()) {
        "Healthy".into()
    } else {
        status.replace('_', " ").to_lowercase()
    };
    let items = services
        .iter()
        .map(|s| Item {
            title: s["name"].as_str().unwrap_or("").to_string(),
            value: if s["healthy"].as_bool() == Some(true) { "healthy".into() } else { s["status"].as_str().unwrap_or("down").to_lowercase() },
            ..Default::default()
        })
        .collect();
    Ok(Reading {
        headline: compact(total),
        caption: format!("{}: API requests, {state}", project["name"].as_str().unwrap_or("Supabase")),
        lines: vec![("Health".into(), state), ("Region".into(), project["region"].as_str().unwrap_or("–").to_string())],
        items,
        series,
        value: Some(total),
        ..Default::default()
    })
}

// ---------------------------------------------------------------------------
// GitHub and LeetCode
// ---------------------------------------------------------------------------

fn github(key: &str, path: &str) -> Result<ureq::Response, ureq::Error> {
    agent()
        .get(&format!("https://api.github.com{path}"))
        .set("Authorization", &format!("Bearer {key}"))
        .set("Accept", "application/vnd.github+json")
        .set("X-GitHub-Api-Version", "2022-11-28")
        .call()
}

fn github_graphql(key: &str, query: &str) -> Result<Value, String> {
    let body = json_of(
        agent()
            .post("https://api.github.com/graphql")
            .set("Authorization", &format!("Bearer {key}"))
            .send_json(json!({ "query": query })),
        "GitHub",
    )?;
    if let Some(e) = body["errors"].as_array().and_then(|a| a.first()) {
        return Err(format!("GitHub: {}", e["message"].as_str().unwrap_or("the request was refused")));
    }
    Ok(body["data"].clone())
}

fn github_activity(key: &str) -> Result<Reading, String> {
    let d = github_graphql(
        key,
        "{ viewer { login contributionsCollection { contributionCalendar { totalContributions weeks { contributionDays { date contributionCount } } } } } }",
    )?;
    let cal = &d["viewer"]["contributionsCollection"]["contributionCalendar"];
    let mut days: Vec<(String, f64)> = vec![];
    for w in cal["weeks"].as_array().cloned().unwrap_or_default() {
        for day in w["contributionDays"].as_array().cloned().unwrap_or_default() {
            days.push((day["date"].as_str().unwrap_or("").to_string(), day["contributionCount"].as_f64().unwrap_or(0.0)));
        }
    }
    // Streak: consecutive days with contributions, ending today (or yesterday,
    // if nothing has been pushed yet today).
    let mut streak = 0;
    let mut i = days.len();
    if i > 0 && days[i - 1].1 == 0.0 {
        i -= 1;
    }
    while i > 0 && days[i - 1].1 > 0.0 {
        streak += 1;
        i -= 1;
    }
    let mut best = 0;
    let mut run = 0;
    for d in &days {
        run = if d.1 > 0.0 { run + 1 } else { 0 };
        best = best.max(run);
    }
    let week: f64 = days.iter().rev().take(7).map(|d| d.1).sum();
    let total = cal["totalContributions"].as_f64().unwrap_or(0.0);
    Ok(Reading {
        headline: format!("{streak}d"),
        caption: format!("{} on GitHub, current streak", d["viewer"]["login"].as_str().unwrap_or("You")),
        lines: vec![
            ("Contributions, last year".into(), group(total, 0)),
            ("This week".into(), group(week, 0)),
            ("Longest streak".into(), format!("{best} days")),
        ],
        series: days.iter().map(|d| d.1).collect(),
        series_labels: days.iter().map(|d| d.0.clone()).collect(),
        value: Some(streak as f64),
        ..Default::default()
    })
}

fn github_notification_items(key: &str) -> Result<(usize, Vec<Item>), String> {
    let list = json_of(github(key, "/notifications?per_page=50"), "GitHub")?;
    let all = list.as_array().cloned().unwrap_or_default();
    let items = all
        .iter()
        .take(8)
        .map(|n| Item {
            title: n["subject"]["title"].as_str().unwrap_or("").to_string(),
            detail: n["repository"]["full_name"].as_str().unwrap_or("").to_string(),
            value: n["reason"].as_str().unwrap_or("").replace('_', " "),
            url: Some("https://github.com/notifications".into()),
        })
        .collect();
    Ok((all.len(), items))
}

fn github_notifications(key: &str) -> Result<Reading, String> {
    let (n, items) = github_notification_items(key).map_err(|e| {
        if e.contains("refused") {
            "GitHub refused the token. Notifications need a classic token with the notifications scope.".into()
        } else {
            e
        }
    })?;
    Ok(Reading {
        headline: if n >= 50 { "50+".into() } else { n.to_string() },
        caption: "Unread GitHub notifications".into(),
        items,
        value: Some(n as f64),
        ..Default::default()
    })
}

fn github_overview(key: &str) -> Result<Reading, String> {
    let d = github_graphql(
        key,
        "{ viewer { login pullRequests(states: OPEN) { totalCount } issues(states: OPEN) { totalCount } followers { totalCount } repositories(ownerAffiliations: OWNER, first: 100, orderBy: {field: STARGAZERS, direction: DESC}) { totalCount nodes { name stargazerCount } } } }",
    )?;
    let v = &d["viewer"];
    let repos = v["repositories"]["nodes"].as_array().cloned().unwrap_or_default();
    let stars: f64 = repos.iter().map(|r| r["stargazerCount"].as_f64().unwrap_or(0.0)).sum();
    let notes = github_notification_items(key).map(|x| x.0).ok();
    let items = repos
        .iter()
        .take(5)
        .map(|r| Item {
            title: r["name"].as_str().unwrap_or("").to_string(),
            value: format!("★ {}", compact(r["stargazerCount"].as_f64().unwrap_or(0.0))),
            ..Default::default()
        })
        .collect();
    let mut lines = vec![
        ("Open pull requests".into(), v["pullRequests"]["totalCount"].to_string()),
        ("Open issues".into(), v["issues"]["totalCount"].to_string()),
        ("Stars".into(), compact(stars)),
        ("Followers".into(), v["followers"]["totalCount"].to_string()),
    ];
    if let Some(n) = notes {
        lines.insert(0, ("Notifications".into(), if n >= 50 { "50+".into() } else { n.to_string() }));
    }
    Ok(Reading {
        headline: format!("★ {}", compact(stars)),
        caption: format!("{} on GitHub", v["login"].as_str().unwrap_or("You")),
        lines,
        items,
        value: Some(stars),
        ..Default::default()
    })
}

fn leetcode(cfg: &Value) -> Result<Reading, String> {
    let user: String = s(cfg, "user").chars().filter(|c| c.is_ascii_alphanumeric() || "-_".contains(*c)).collect();
    if user.is_empty() {
        return Err("Add your LeetCode username in the widget.".into());
    }
    let q = "query($u: String!) { matchedUser(username: $u) { submitStatsGlobal { acSubmissionNum { difficulty count } } userCalendar { streak totalActiveDays } } }";
    let body = json_of(
        browser_agent()
            .post("https://leetcode.com/graphql")
            .set("Referer", "https://leetcode.com/")
            .send_json(json!({ "query": q, "variables": { "u": user } })),
        "LeetCode",
    )?;
    let u = &body["data"]["matchedUser"];
    if u.is_null() {
        return Err(format!("LeetCode has no user called {user}."));
    }
    let mut counts: HashMap<String, f64> = HashMap::new();
    for row in u["submitStatsGlobal"]["acSubmissionNum"].as_array().cloned().unwrap_or_default() {
        counts.insert(row["difficulty"].as_str().unwrap_or("").to_string(), row["count"].as_f64().unwrap_or(0.0));
    }
    let c = |k: &str| counts.get(k).copied().unwrap_or(0.0);
    let streak = u["userCalendar"]["streak"].as_i64().unwrap_or(0);
    Ok(Reading {
        headline: group(c("All"), 0),
        caption: format!("{user}: problems solved"),
        lines: vec![
            ("Easy".into(), group(c("Easy"), 0)),
            ("Medium".into(), group(c("Medium"), 0)),
            ("Hard".into(), group(c("Hard"), 0)),
            ("Streak".into(), format!("{streak} days")),
            ("Active days this year".into(), u["userCalendar"]["totalActiveDays"].to_string()),
        ],
        value: Some(c("All")),
        ..Default::default()
    })
}

// ---------------------------------------------------------------------------
// Mail (IMAP with an app password)
// ---------------------------------------------------------------------------

fn imap_server(email: &str) -> Option<&'static str> {
    let domain = email.rsplit('@').next()?.to_ascii_lowercase();
    Some(match domain.as_str() {
        "gmail.com" | "googlemail.com" => "imap.gmail.com",
        "icloud.com" | "me.com" | "mac.com" => "imap.mail.me.com",
        "yahoo.com" | "ymail.com" => "imap.mail.yahoo.com",
        "fastmail.com" | "fastmail.fm" => "imap.fastmail.com",
        "aol.com" => "imap.aol.com",
        "zoho.com" => "imap.zoho.com",
        "proton.me" | "protonmail.com" => "127.0.0.1",
        _ => return None,
    })
}

fn imap_quote(v: &str) -> String {
    format!("\"{}\"", v.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Read until the tagged reply to `tag`; return everything read.
fn imap_until<S: Read>(stream: &mut S, tag: &str) -> Result<String, String> {
    let mut out = Vec::new();
    let mut buf = [0u8; 4096];
    let needle = format!("{tag} ");
    loop {
        let n = stream.read(&mut buf).map_err(|e| format!("Mail server stopped answering ({e})."))?;
        if n == 0 {
            return Err("The mail server closed the connection.".into());
        }
        out.extend_from_slice(&buf[..n]);
        let text = String::from_utf8_lossy(&out);
        if let Some(line) = text.lines().rev().find(|l| l.starts_with(&needle)) {
            if text.ends_with("\r\n") {
                if line[needle.len()..].starts_with("OK") {
                    return Ok(text.into_owned());
                }
                let why = line[needle.len()..].trim();
                return Err(if tag == "a1" {
                    format!("The mail server refused the sign-in. Use an app password, not your normal password. ({why})")
                } else {
                    format!("The mail server said: {why}")
                });
            }
        }
        if out.len() > 2_000_000 {
            return Err("The mail server sent too much.".into());
        }
    }
}

/// Decode RFC 2047 words like "=?UTF-8?B?...?=" in a header.
fn decode_header(v: &str) -> String {
    use base64::Engine;
    let mut out = String::new();
    let mut rest = v;
    while let Some(start) = rest.find("=?") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let parts: Vec<&str> = after.splitn(3, '?').collect();
        if parts.len() < 3 {
            out.push_str(&rest[start..]);
            return out;
        }
        let Some(end) = parts[2].find("?=") else {
            out.push_str(&rest[start..]);
            return out;
        };
        let data = &parts[2][..end];
        let bytes = if parts[1].eq_ignore_ascii_case("B") {
            base64::engine::general_purpose::STANDARD.decode(data).unwrap_or_default()
        } else {
            let mut b = vec![];
            let q = data.replace('_', " ");
            let q = q.as_bytes();
            let mut i = 0;
            while i < q.len() {
                if q[i] == b'=' && i + 2 < q.len() {
                    if let Ok(x) = u8::from_str_radix(std::str::from_utf8(&q[i + 1..i + 3]).unwrap_or(""), 16) {
                        b.push(x);
                        i += 3;
                        continue;
                    }
                }
                b.push(q[i]);
                i += 1;
            }
            b
        };
        out.push_str(&String::from_utf8_lossy(&bytes));
        rest = &parts[2][end + 2..];
        // Spaces between two encoded words are not part of the text.
        let trimmed = rest.trim_start_matches([' ', '\t']);
        if trimmed.starts_with("=?") {
            rest = trimmed;
        }
    }
    out.push_str(rest);
    out.trim().to_string()
}

fn mail(password: &str, cfg: &Value) -> Result<Reading, String> {
    let email = s(cfg, "email");
    if email.is_empty() {
        return Err("Add your email address in the widget.".into());
    }
    let server = if s(cfg, "server").is_empty() {
        imap_server(email).ok_or("Add your mail server's IMAP address in the widget.")?.to_string()
    } else {
        s(cfg, "server").to_string()
    };
    let addr = std::net::ToSocketAddrs::to_socket_addrs(&(server.as_str(), 993))
        .map_err(|_| format!("Can't find {server}."))?
        .next()
        .ok_or(format!("Can't find {server}."))?;
    let tcp = std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(10)).map_err(|_| format!("Can't reach {server}."))?;
    tcp.set_read_timeout(Some(Duration::from_secs(15))).ok();
    let connector = native_tls::TlsConnector::new().map_err(|e| e.to_string())?;
    let mut tls = connector.connect(&server, tcp).map_err(|e| format!("Secure connection to {server} failed ({e})."))?;
    // Greeting.
    let mut buf = [0u8; 1024];
    let _ = tls.read(&mut buf);
    let send = |tls: &mut native_tls::TlsStream<std::net::TcpStream>, tag: &str, cmd: &str| -> Result<String, String> {
        tls.write_all(format!("{tag} {cmd}\r\n").as_bytes()).map_err(|e| e.to_string())?;
        imap_until(tls, tag)
    };
    send(&mut tls, "a1", &format!("LOGIN {} {}", imap_quote(email), imap_quote(password)))?;
    send(&mut tls, "a2", "EXAMINE INBOX")?;
    let search = send(&mut tls, "a3", "SEARCH UNSEEN")?;
    let ids: Vec<u64> = search
        .lines()
        .filter(|l| l.starts_with("* SEARCH"))
        .flat_map(|l| l[8..].split_whitespace().filter_map(|x| x.parse().ok()).collect::<Vec<_>>())
        .collect();
    let mut items = vec![];
    if !ids.is_empty() {
        let last: Vec<String> = ids.iter().rev().take(6).map(|i| i.to_string()).collect();
        let fetched = send(&mut tls, "a4", &format!("FETCH {} (BODY.PEEK[HEADER.FIELDS (FROM SUBJECT)])", last.join(",")))?;
        // Each message's headers come as a block; unfold and pick From/Subject.
        for block in fetched.split("\r\n* ").skip(0) {
            let unfolded = block.replace("\r\n ", " ").replace("\r\n\t", " ");
            let mut from = String::new();
            let mut subject = String::new();
            for line in unfolded.lines() {
                let lower = line.to_ascii_lowercase();
                if lower.starts_with("from:") {
                    from = decode_header(&line[5..]);
                    if let Some(i) = from.find('<') {
                        let name = from[..i].trim().trim_matches('"').to_string();
                        if !name.is_empty() {
                            from = name;
                        }
                    }
                } else if lower.starts_with("subject:") {
                    subject = decode_header(&line[8..]);
                }
            }
            if !from.is_empty() || !subject.is_empty() {
                items.push(Item { title: if subject.is_empty() { "(no subject)".into() } else { subject }, detail: from, ..Default::default() });
            }
        }
        items.reverse();
    }
    let _ = send(&mut tls, "a5", "LOGOUT");
    let n = ids.len();
    let gmail = server.contains("gmail");
    let latest = items.first().map(|i| i.detail.clone()).unwrap_or_default();
    Ok(Reading {
        headline: n.to_string(),
        caption: if n == 0 { "No unread mail".into() } else { format!("Unread mail, latest from {latest}") },
        lines: vec![("Account".into(), email.to_string())],
        items: items
            .into_iter()
            .map(|mut i| {
                if gmail {
                    i.url = Some("https://mail.google.com/mail/u/0/#inbox".into());
                }
                i
            })
            .collect(),
        value: Some(n as f64),
        ..Default::default()
    })
}

// ---------------------------------------------------------------------------
// Social
// ---------------------------------------------------------------------------

fn handle(cfg: &Value, field: &str) -> String {
    s(cfg, field).trim_start_matches('@').chars().filter(|c| c.is_ascii_alphanumeric() || "._-".contains(*c)).collect()
}

fn x_followers(key: &str, cfg: &Value) -> Result<Reading, String> {
    let user = handle(cfg, "user");
    if user.is_empty() {
        return Err("Add your X username in the widget.".into());
    }
    let body = json_of(
        agent()
            .get(&format!("https://api.x.com/2/users/by/username/{user}"))
            .query("user.fields", "public_metrics")
            .set("Authorization", &format!("Bearer {key}"))
            .call(),
        "X",
    )?;
    let m = &body["data"]["public_metrics"];
    if m.is_null() {
        return Err(format!("X has no account called @{user}."));
    }
    let followers = num(m, "followers_count").unwrap_or(0.0);
    Ok(Reading {
        headline: compact(followers),
        caption: format!("@{user} followers on X"),
        lines: vec![
            ("Following".into(), compact(num(m, "following_count").unwrap_or(0.0))),
            ("Posts".into(), compact(num(m, "tweet_count").unwrap_or(0.0))),
        ],
        value: Some(followers),
        ..Default::default()
    })
}

fn youtube(key: &str, cfg: &Value) -> Result<Reading, String> {
    let h = handle(cfg, "channel");
    if h.is_empty() {
        return Err("Add your channel's handle (@name) in the widget.".into());
    }
    let mut req = agent()
        .get("https://www.googleapis.com/youtube/v3/channels")
        .query("part", "statistics,snippet")
        .set("X-goog-api-key", key);
    req = if h.starts_with("UC") && h.len() == 24 { req.query("id", &h) } else { req.query("forHandle", &format!("@{h}")) };
    let body = json_of(req.call(), "YouTube")?;
    let c = &body["items"][0];
    if c.is_null() {
        return Err(format!("YouTube has no channel @{h}."));
    }
    let st = &c["statistics"];
    let f = |k: &str| st[k].as_str().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
    Ok(Reading {
        headline: compact(f("subscriberCount")),
        caption: format!("{} subscribers", c["snippet"]["title"].as_str().unwrap_or("YouTube")),
        lines: vec![("Views".into(), compact(f("viewCount"))), ("Videos".into(), compact(f("videoCount")))],
        value: Some(f("subscriberCount")),
        ..Default::default()
    })
}

fn instagram(key: &str) -> Result<Reading, String> {
    let body = json_of(
        agent()
            .get("https://graph.instagram.com/me")
            .query("fields", "username,followers_count,follows_count,media_count")
            .set("Authorization", &format!("Bearer {key}"))
            .call(),
        "Instagram",
    )?;
    let followers = num(&body, "followers_count").unwrap_or(0.0);
    Ok(Reading {
        headline: compact(followers),
        caption: format!("@{} followers on Instagram", body["username"].as_str().unwrap_or("you")),
        lines: vec![
            ("Following".into(), compact(num(&body, "follows_count").unwrap_or(0.0))),
            ("Posts".into(), compact(num(&body, "media_count").unwrap_or(0.0))),
        ],
        value: Some(followers),
        ..Default::default()
    })
}

// ---------------------------------------------------------------------------
// Calendar (a private iCalendar address from Google, Outlook or iCloud)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Event {
    /// Local wall-clock start, in seconds since 1970 (local, not UTC).
    start: i64,
    all_day: bool,
    title: String,
    location: String,
    uid: String,
    /// A single changed occurrence of a repeating event.
    is_override: bool,
}

/// Parse "20261118T103000Z", "20261118T103000" or "20261118" into local seconds.
fn ics_time(value: &str, params: &str) -> Option<(i64, bool)> {
    let v = value.trim();
    let n = |r: std::ops::Range<usize>| v.get(r)?.parse::<i64>().ok();
    let day = days_from_civil(n(0..4)?, n(4..6)?, n(6..8)?);
    if v.len() == 8 || params.contains("VALUE=DATE") && !v.contains('T') {
        return Some((day * 86_400, true));
    }
    let secs = day * 86_400 + n(9..11)? * 3600 + n(11..13)? * 60 + n(13..15).unwrap_or(0);
    // UTC times move to this PC's zone; times with a TZID are taken as written
    // (right whenever the calendar's zone is this PC's zone).
    Some((if v.ends_with('Z') { secs + local_offset() } else { secs }, false))
}

fn weekday(local_secs: i64) -> i64 {
    // 1970-01-01 was a Thursday; 0 = Monday.
    (local_secs.div_euclid(86_400) + 3).rem_euclid(7)
}

fn expand(rrule: &str, first: i64, exdates: &[i64], from: i64, to: i64) -> Vec<i64> {
    let mut freq = "";
    let mut interval = 1i64;
    let mut count: Option<i64> = None;
    let mut until: Option<i64> = None;
    let mut byday: Vec<i64> = vec![];
    for part in rrule.split(';') {
        let (k, v) = part.split_once('=').unwrap_or((part, ""));
        match k {
            "FREQ" => freq = v,
            "INTERVAL" => interval = v.parse().unwrap_or(1).max(1),
            "COUNT" => count = v.parse().ok(),
            "UNTIL" => until = ics_time(v, "").map(|t| t.0),
            "BYDAY" => {
                byday = v
                    .split(',')
                    .filter_map(|d| {
                        let d = d.trim_start_matches(|c: char| c == '-' || c == '+' || c.is_ascii_digit());
                        ["MO", "TU", "WE", "TH", "FR", "SA", "SU"].iter().position(|x| *x == d).map(|i| i as i64)
                    })
                    .collect()
            }
            _ => {}
        }
    }
    let mut out = vec![];
    let mut n = 0i64;
    let tod = first.rem_euclid(86_400);
    let first_day = first.div_euclid(86_400);
    let (fy, fm, fd) = civil_from_days(first_day);
    for step in 0..3000i64 {
        let t = match freq {
            "DAILY" => first + step * interval * 86_400,
            "WEEKLY" if !byday.is_empty() => {
                // Walk day by day; keep days on the listed weekdays in active weeks.
                let day = first_day - weekday(first) + step; // from that week's Monday
                if day < first_day {
                    continue;
                }
                let week = (day - (first_day - weekday(first))) / 7;
                if week % interval != 0 || !byday.contains(&((day + 3).rem_euclid(7))) {
                    continue;
                }
                day * 86_400 + tod
            }
            "WEEKLY" => first + step * interval * 7 * 86_400,
            "MONTHLY" => {
                let months = fm - 1 + step * interval;
                let (y, m) = (fy + months.div_euclid(12), months.rem_euclid(12) + 1);
                days_from_civil(y, m, fd) * 86_400 + tod
            }
            "YEARLY" => days_from_civil(fy + step * interval, fm, fd) * 86_400 + tod,
            _ => return if first >= from && first <= to { vec![first] } else { vec![] },
        };
        if until.is_some_and(|u| t > u) || t > to {
            break;
        }
        n += 1;
        if count.is_some_and(|c| n > c) {
            break;
        }
        if t >= from && !exdates.contains(&t) {
            out.push(t);
        }
    }
    out
}

fn calendar(url: &str) -> Result<Reading, String> {
    let url = url.trim().replacen("webcal://", "https://", 1);
    if !url.starts_with("https://") {
        return Err("Paste the calendar's secret address in iCal format (it starts with https:// or webcal://).".into());
    }
    let text = agent()
        .get(&url)
        .call()
        .map_err(|e| http_error("The calendar", e))?
        .into_string()
        .map_err(|e| e.to_string())?;
    let text = text.replace("\r\n ", "").replace("\r\n\t", "").replace("\n ", "");
    let now = now_secs() + local_offset();
    let horizon = now + 60 * 86_400;
    let mut events: Vec<Event> = vec![];
    let mut cur: Option<(Option<(i64, bool)>, String, String, String, Vec<i64>, Option<i64>)> = None;
    let mut overridden: Vec<(String, i64)> = vec![];
    let mut uid = String::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line == "BEGIN:VEVENT" {
            cur = Some((None, String::new(), String::new(), String::new(), vec![], None));
            uid.clear();
            continue;
        }
        let Some(e) = cur.as_mut() else { continue };
        if line == "END:VEVENT" {
            let (start, title, location, rrule, exdates, recurrence_id) = cur.take().unwrap();
            let Some((start, all_day)) = start else { continue };
            let is_override = recurrence_id.is_some();
            if let Some(rid) = recurrence_id {
                overridden.push((uid.clone(), rid));
            }
            let title = title.replace("\\,", ",").replace("\\;", ";").replace("\\n", " ");
            let location = location.replace("\\,", ",").replace("\\;", ";").replace("\\n", " ");
            let from = now - if all_day { 86_400 } else { 3 * 3600 };
            let times = if rrule.is_empty() {
                if start >= from && start <= horizon { vec![start] } else { vec![] }
            } else {
                expand(&rrule, start, &exdates, from, horizon)
            };
            for t in times {
                if all_day && t + 86_400 <= now {
                    continue;
                }
                events.push(Event { start: t, all_day, title: title.clone(), location: location.clone(), uid: uid.clone(), is_override });
            }
            continue;
        }
        let (head, value) = line.split_once(':').unwrap_or((line, ""));
        let (name, params) = head.split_once(';').unwrap_or((head, ""));
        match name {
            "DTSTART" => e.0 = ics_time(value, params),
            "SUMMARY" => e.1 = value.to_string(),
            "LOCATION" => e.2 = value.to_string(),
            "RRULE" => e.3 = value.to_string(),
            "EXDATE" => e.4.extend(value.split(',').filter_map(|v| ics_time(v, params).map(|t| t.0))),
            "RECURRENCE-ID" => e.5 = ics_time(value, params).map(|t| t.0),
            "UID" => uid = value.to_string(),
            _ => {}
        }
    }
    // A changed single occurrence replaces the series' copy of it.
    events.retain(|ev| ev.is_override || !overridden.iter().any(|(u, t)| *u == ev.uid && *t == ev.start));
    events.sort_by_key(|e| (e.start, !e.all_day));
    events.dedup_by(|a, b| a.start == b.start && a.title == b.title);
    let fmt = |e: &Event| {
        let day = e.start.div_euclid(86_400);
        let t = e.start.rem_euclid(86_400);
        if e.all_day { ymd(day) } else { format!("{}T{:02}:{:02}", ymd(day), t / 3600, t % 3600 / 60) }
    };
    let items: Vec<Item> = events
        .iter()
        .take(12)
        .map(|e| Item {
            title: e.title.clone(),
            detail: e.location.clone(),
            value: fmt(e),
            url: None,
        })
        .collect();
    let next = events.iter().find(|e| !e.all_day && e.start + 1800 > now).or(events.first());
    let (headline, caption) = match next {
        Some(e) => {
            (fmt(e), e.title.clone())
        }
        None => (String::new(), "Nothing in the next 60 days".into()),
    };
    Ok(Reading { headline, caption, items, ..Default::default() })
}

// ---------------------------------------------------------------------------
// Wallpaper gallery (Bing's picture of the day)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct Picture {
    id: String,
    title: String,
    credit: String,
    thumb: String,
}

#[tauri::command]
pub async fn wallpaper_gallery() -> Result<Vec<Picture>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        use base64::Engine;
        let body = json_of(
            agent().get("https://www.bing.com/HPImageArchive.aspx").query("format", "js").query("idx", "0").query("n", "8").call(),
            "Bing",
        )?;
        let mut out = vec![];
        for img in body["images"].as_array().cloned().unwrap_or_default() {
            let Some(base) = img["urlbase"].as_str() else { continue };
            let id = base.trim_start_matches("/th?id=").to_string();
            if !id.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) {
                continue;
            }
            let Ok(resp) = agent().get(&format!("https://www.bing.com{base}_400x240.jpg")).call() else { continue };
            let mut bytes = vec![];
            if resp.into_reader().take(400_000).read_to_end(&mut bytes).is_err() {
                continue;
            }
            out.push(Picture {
                id,
                title: img["title"].as_str().unwrap_or("").to_string(),
                credit: img["copyright"].as_str().unwrap_or("").to_string(),
                thumb: format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)),
            });
        }
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Download one gallery picture in full size and make it the desktop background.
#[tauri::command]
pub async fn wallpaper_set_gallery(app: tauri::AppHandle, id: String) -> Result<(), String> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) {
        return Err("bad picture".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Emitter;
        use windows::core::HSTRING;
        use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED};
        use windows::Win32::UI::Shell::{DesktopWallpaper, IDesktopWallpaper};
        let dir = crate::taskbar::data_dir().join("wallpapers");
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let file = dir.join(format!("{id}.jpg"));
        if !file.exists() {
            let mut bytes = vec![];
            let resp = agent()
                .get(&format!("https://www.bing.com/th?id={id}_UHD.jpg"))
                .call()
                .or_else(|_| agent().get(&format!("https://www.bing.com/th?id={id}_1920x1080.jpg")).call())
                .map_err(|e| http_error("Bing", e))?;
            resp.into_reader().take(30_000_000).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            fs::write(&file, bytes).map_err(|e| e.to_string())?;
        }
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let wp: IDesktopWallpaper = CoCreateInstance(&DesktopWallpaper, None, CLSCTX_ALL).map_err(|e| e.to_string())?;
            wp.SetWallpaper(None, &HSTRING::from(file.as_os_str())).map_err(|e| e.to_string())?;
        }
        let _ = app.emit("wallpaper-changed", true);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn online_local_found(provider: String) -> bool {
    match provider.as_str() {
        "gemini" => home().join(".gemini").join("oauth_creds.json").exists(),
        "opencode" | "opencode_week" => opencode_found(),
        _ => true,
    }
}

/// Open an https link from a widget (an issue, a notification) in the browser.
#[tauri::command]
pub fn open_link(url: String) -> Result<(), String> {
    // https links, plus the one app link the WhatsApp widget uses.
    if url != "whatsapp://" && (!url.starts_with("https://") || url.chars().any(|c| c.is_whitespace() || c == '"')) {
        return Err("only https links can be opened".into());
    }
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    unsafe {
        ShellExecuteW(None, w!("open"), &HSTRING::from(url), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_times() {
        assert_eq!(parse_time("1970-01-02T00:00:00Z"), Some(86_400));
        assert_eq!(parse_time("1970-01-01T02:00:00+02:00"), Some(0));
        assert_eq!(parse_time("1970-01-01T00:00:00.123Z"), Some(0));
        assert_eq!(parse_time("1970-01-03"), Some(2 * 86_400));
    }

    #[test]
    fn money_and_counts() {
        assert_eq!(amount(1234.5, "usd"), "$1,234.50");
        assert_eq!(amount(12345.0, "EUR"), "€12,345");
        assert_eq!(compact(15300.0), "15.3K");
        assert_eq!(compact(999.0), "999");
    }

    #[test]
    fn weekly_rule() {
        // 1970-01-05 was a Monday; every Mon and Wed at 09:00 for 2 weeks.
        let first = 4 * 86_400 + 9 * 3600;
        let got = expand("FREQ=WEEKLY;BYDAY=MO,WE;COUNT=4", first, &[], 0, 30 * 86_400);
        let days: Vec<i64> = got.iter().map(|t| t / 86_400).collect();
        assert_eq!(days, vec![4, 6, 11, 13]);
    }

    /// Live check of the services that need no key: cargo test -- --ignored live
    #[test]
    #[ignore]
    fn live_no_key_services() {
        for (p, cfg) in [
            ("stock", json!({"symbol": "MSFT"})),
            ("watchlist", json!({"symbols": "AAPL, NVDA"})),
            ("crypto", json!({"symbol": "ETH"})),
            ("currency", json!({"from": "USD", "to": "INR", "amount": 10})),
            ("leetcode", json!({"user": "lee215"})),
        ] {
            match fetch(p, None, &cfg) {
                Ok(r) => println!("{p}: {} | {} | {} points | {} items", r.headline, r.caption, r.series.len(), r.items.len()),
                Err(e) => println!("{p}: ERROR {e}"),
            }
        }
    }

    #[test]
    fn header_words() {
        assert_eq!(decode_header("=?UTF-8?B?SGVsbG8=?= world"), "Hello world");
        assert_eq!(decode_header("=?utf-8?Q?caf=C3=A9?="), "café");
    }
}
