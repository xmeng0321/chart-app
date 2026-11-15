use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub fn favorites_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".config")
        .join("chart-app")
        .join("favorites.json")
}

pub fn load_favorites() -> Vec<String> {
    let path = favorites_path();
    if let Ok(content) = std::fs::read_to_string(&path) {
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Vec::new()
    }
}

pub fn save_favorites(favorites: &[String]) {
    let path = favorites_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(favorites) {
        let _ = std::fs::write(&path, json);
    }
}

// ── Filtered stock list persistence ──

pub fn filtered_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".config")
        .join("chart-app")
        .join("filtered.json")
}

const DEFAULT_FILTER_KEY: &str = "default";

#[derive(Deserialize)]
#[serde(untagged)]
enum FilteredStore {
    ByName(HashMap<String, Vec<String>>),
    Legacy(Vec<String>),
}

fn filtered_key(filter_name: Option<&str>) -> String {
    filter_name
        .filter(|name| !name.is_empty())
        .unwrap_or(DEFAULT_FILTER_KEY)
        .to_string()
}

pub fn load_filtered(filter_name: Option<&str>) -> Vec<String> {
    let path = filtered_path();
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };

    match serde_json::from_str::<FilteredStore>(&content) {
        Ok(FilteredStore::ByName(by_name)) => by_name
            .get(&filtered_key(filter_name))
            .cloned()
            .unwrap_or_default(),
        Ok(FilteredStore::Legacy(filtered)) => filtered,
        Err(_) => Vec::new(),
    }
}

pub fn save_filtered(filter_name: Option<&str>, filtered: &[String]) {
    let path = filtered_path();
    let mut by_name = read_filtered_store(&path).unwrap_or_default();
    by_name.insert(filtered_key(filter_name), filtered.to_vec());
    write_filtered_store(&path, &by_name);
}

/// Drops a bundle's cached results, e.g. after the bundle itself is deleted.
pub fn remove_filtered(filter_name: &str) {
    let path = filtered_path();
    let Some(mut by_name) = read_filtered_store(&path) else {
        return;
    };
    if by_name.remove(&filtered_key(Some(filter_name))).is_some() {
        write_filtered_store(&path, &by_name);
    }
}

/// Moves cached results to a new name so a rename doesn't discard the last run.
pub fn rename_filtered(old_name: &str, new_name: &str) {
    if old_name == new_name {
        return;
    }
    let path = filtered_path();
    let Some(mut by_name) = read_filtered_store(&path) else {
        return;
    };
    if let Some(v) = by_name.remove(&filtered_key(Some(old_name))) {
        by_name.insert(filtered_key(Some(new_name)), v);
        write_filtered_store(&path, &by_name);
    }
}

fn read_filtered_store(path: &Path) -> Option<HashMap<String, Vec<String>>> {
    let content = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str::<FilteredStore>(&content).ok()? {
        FilteredStore::ByName(by_name) => Some(by_name),
        FilteredStore::Legacy(filtered) => {
            let mut by_name = HashMap::new();
            by_name.insert(DEFAULT_FILTER_KEY.to_string(), filtered);
            Some(by_name)
        }
    }
}

fn write_filtered_store(path: &Path, by_name: &HashMap<String, Vec<String>>) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(by_name) {
        let _ = std::fs::write(path, json);
    }
}

// ── Last-selected stock per tab persistence ──
//
// The UI has multiple stock tabs (全部/自选/筛选). When the user switches tabs
// or reopens the app, we restore the stock they were last viewing on that tab.
// Keys are tab identifiers ("all", "favorites", "filtered"); values are secids.

pub fn last_selected_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".config")
        .join("chart-app")
        .join("last_selected.json")
}

pub fn load_last_selected() -> HashMap<String, String> {
    let path = last_selected_path();
    if let Ok(content) = std::fs::read_to_string(&path) {
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        HashMap::new()
    }
}

pub fn save_last_selected(last: &HashMap<String, String>) {
    let path = last_selected_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(last) {
        let _ = std::fs::write(&path, json);
    }
}

// ── Selected filter persistence ──
//
// Remembers which named filter (from `settings.data_filters[].name`) the user
// last picked in the Filtered tab dropdown so it can be restored next launch.

pub fn selected_filter_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".config")
        .join("chart-app")
        .join("selected_filter.json")
}

pub fn load_selected_filter() -> Option<String> {
    let path = selected_filter_path();
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn save_selected_filter(name: &str) {
    let path = selected_filter_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(name) {
        let _ = std::fs::write(&path, json);
    }
}

// ── App settings (stock-dl config) ──

#[derive(Serialize, Deserialize, Clone)]
pub struct DlSettings {
    #[serde(default = "default_data_dir")]
    pub data_dir: String,
    #[serde(default = "default_stock_dl_binary")]
    pub binary: String,
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
}

impl Default for DlSettings {
    fn default() -> Self {
        Self {
            data_dir: default_data_dir(),
            binary: default_stock_dl_binary(),
            concurrency: default_concurrency(),
        }
    }
}

#[derive(Serialize, Clone)]
pub struct AppSettings {
    pub dl: DlSettings,
    pub data_filters: Vec<NamedFilter>,
    pub ma: Vec<usize>,
}

fn default_filter_periods() -> Vec<Period> {
    vec![Period::Daily]
}

/// A named bundle of filter lines, edited in the Filtered tab and picked from
/// the dropdown by name. All lines in `filters` must hold for a stock to match.
///
/// Each line may pin its own timeframe with a `day:` / `week:` prefix.
/// `periods` is the fallback for lines without a prefix: such a line passes if
/// it holds on any of the listed periods. Defaults to `["daily"]`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NamedFilter {
    pub name: String,
    #[serde(default)]
    pub filters: Vec<String>,
    #[serde(default = "default_filter_periods")]
    pub periods: Vec<Period>,
}

impl NamedFilter {
    /// Timeframes this bundle actually reads, for the `[日]` / `[周]` /
    /// `[日+周]` badge next to the picker. Unparsable lines are ignored — the
    /// editor reports those separately.
    pub fn active_periods(&self) -> Vec<Period> {
        let defaults = if self.periods.is_empty() {
            &[Period::Daily][..]
        } else {
            &self.periods
        };
        let mut out: Vec<Period> = Vec::new();
        for line in &self.filters {
            if is_blank_or_comment(line) {
                continue;
            }
            let periods = match parse_filter_line(line) {
                Ok(l) => l.period.map(|p| vec![p]).unwrap_or_else(|| defaults.to_vec()),
                Err(_) => continue,
            };
            for p in periods {
                if !out.contains(&p) {
                    out.push(p);
                }
            }
        }
        if out.is_empty() {
            defaults.to_vec()
        } else {
            out
        }
    }
}

fn default_data_dir() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    format!("{}/Downloads/stock-dl-data/", home)
}
fn default_stock_dl_binary() -> String {
    "stock-dl".to_string()
}

fn default_concurrency() -> usize {
    8
}
fn default_data_filters() -> Vec<NamedFilter> {
    Vec::new()
}

/// Accepts either the new shape (`[{name, filters}, ...]`) or the legacy plain
/// list of expressions (`["price > ma60", ...]`), in which case it's wrapped as
/// a single unnamed bundle so old `settings.json` files keep working.
fn deserialize_data_filters<'de, D>(deserializer: D) -> Result<Vec<NamedFilter>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Either {
        Named(Vec<NamedFilter>),
        Legacy(Vec<String>),
    }
    Ok(match Either::deserialize(deserializer)? {
        Either::Named(v) => v,
        Either::Legacy(v) if v.is_empty() => Vec::new(),
        Either::Legacy(v) => vec![NamedFilter {
            name: "default".to_string(),
            filters: v,
            periods: default_filter_periods(),
        }],
    })
}

fn default_ma() -> Vec<usize> {
    vec![10, 30, 60]
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            dl: DlSettings::default(),
            data_filters: default_data_filters(),
            ma: default_ma(),
        }
    }
}

#[derive(Deserialize, Default)]
struct LegacyAppSettings {
    #[serde(default)]
    dl: Option<LegacyDl>,
    #[serde(default = "default_data_dir")]
    data_dir: String,
    #[serde(default = "default_stock_dl_binary")]
    stock_dl_binary: String,
    #[serde(default = "default_concurrency")]
    concurrency: usize,
    #[serde(default = "default_data_filters", deserialize_with = "deserialize_data_filters")]
    data_filters: Vec<NamedFilter>,
    #[serde(default)]
    ma: Option<Vec<usize>>,
    // Legacy top-level alias for `ma`.
    #[serde(default)]
    ma_windows: Option<Vec<usize>>,
}

/// Mirrors `DlSettings` but also tolerates the legacy `ma_windows` key so we
/// can migrate it up to the top-level `ma` field if present.
#[derive(Deserialize)]
struct LegacyDl {
    #[serde(default = "default_data_dir")]
    data_dir: String,
    #[serde(default = "default_stock_dl_binary")]
    binary: String,
    #[serde(default = "default_concurrency")]
    concurrency: usize,
    #[serde(default)]
    ma_windows: Option<Vec<usize>>,
}

impl<'de> Deserialize<'de> for AppSettings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let legacy = LegacyAppSettings::deserialize(deserializer)?;
        let (dl, dl_ma_windows) = match legacy.dl {
            Some(d) => (
                DlSettings {
                    data_dir: d.data_dir,
                    binary: d.binary,
                    concurrency: d.concurrency,
                },
                d.ma_windows,
            ),
            None => (
                DlSettings {
                    data_dir: legacy.data_dir,
                    binary: legacy.stock_dl_binary,
                    concurrency: legacy.concurrency,
                },
                None,
            ),
        };

        // Resolve MA windows from the first location that exists, newest
        // spelling winning: top-level `ma` → top-level `ma_windows` →
        // `dl.ma_windows` → default.
        let ma = legacy
            .ma
            .or(legacy.ma_windows)
            .or(dl_ma_windows)
            .filter(|v| !v.is_empty())
            .unwrap_or_else(default_ma);

        Ok(Self {
            dl,
            data_filters: legacy.data_filters,
            ma,
        })
    }
}

pub fn settings_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".config")
        .join("chart-app")
        .join("settings.json")
}

pub fn load_settings() -> AppSettings {
    let path = settings_path();
    if let Ok(content) = std::fs::read_to_string(&path) {
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        let settings = AppSettings::default();
        save_settings(&settings);
        settings
    }
}

pub fn save_settings(settings: &AppSettings) {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(settings) {
        let _ = std::fs::write(&path, json);
    }
}

// ── Sync status types ──

#[derive(Clone)]
pub enum SyncStatus {
    Idle,
    Running {
        progress_pct: f32,
        completed: usize,
        total: usize,
        current_stock: String,
    },
    Done {
        success: usize,
        failed: usize,
    },
    Error(String),
}

pub enum SyncMessage {
    Progress {
        pct: f32,
        completed: usize,
        total: usize,
        stock_label: String,
    },
    Finished(Result<(usize, usize), String>),
}

/// Parse stock-dl progress line: `[45.32% 234/516] 600519 贵州茅台 -> ...`
pub fn parse_progress_line(line: &str) -> Option<(f32, usize, usize, String)> {
    let line = line.trim();
    if !line.starts_with('[') {
        return None;
    }
    let bracket_end = line.find(']')?;
    let inner = &line[1..bracket_end];
    // inner = "45.32% 234/516"
    let mut parts = inner.split_whitespace();
    let pct_str = parts.next()?;
    let pct: f32 = pct_str.trim_end_matches('%').parse().ok()?;
    let fraction = parts.next()?;
    let (completed_str, total_str) = fraction.split_once('/')?;
    let completed: usize = completed_str.parse().ok()?;
    let total: usize = total_str.parse().ok()?;

    // Rest after "] " is stock info: "600519 贵州茅台 -> ..."
    let rest = &line[bracket_end + 1..].trim_start();
    // Take stock code + name (before " -> ")
    let stock_label = rest.split(" -> ").next().unwrap_or(rest).to_string();

    Some((pct, completed, total, stock_label))
}

/// Parse stock-dl final line: `finished: success=N, failed=M`
pub fn parse_finished_line(line: &str) -> Option<(usize, usize)> {
    let line = line.trim();
    if !line.starts_with("finished:") {
        return None;
    }
    let mut success = None;
    let mut failed = None;
    for part in line.split(|c: char| c == ',' || c == ' ') {
        if let Some(val) = part.strip_prefix("success=") {
            success = val.parse().ok();
        } else if let Some(val) = part.strip_prefix("failed=") {
            failed = val.parse().ok();
        }
    }
    Some((success?, failed?))
}

/// Parse a summary field line from the new stock-dl output format.
///
/// Lines look like `  updated  : 42 (with 1234 new bars)` or `  skipped  : 5015 (already fresh today)`.
/// Returns `Some(("updated"|"skipped"|"failed"|"stocks", N))` or `None`.
pub fn parse_dl_summary_field(line: &str) -> Option<(&str, usize)> {
    let trimmed = line.trim();
    let colon = trimmed.find(':')?;
    let key = trimmed[..colon].trim();
    // "data dir" has a space and its value is a path, not a count — skip it.
    if key.contains(' ') {
        return None;
    }
    let rest = trimmed[colon + 1..].trim();
    let n: usize = rest.split_whitespace().next()?.parse().ok()?;
    Some((key, n))
}

// ── Stock data types ──

#[derive(Deserialize, Clone)]
pub struct StockInfo {
    pub code: String,
    pub name: String,
    pub secid: String,
}

#[derive(Clone, Deserialize)]
pub struct TradeAgentResult {
    pub r#match: bool,
    pub confidence: f64,
    #[allow(dead_code)]
    pub explanation: String,
}

#[derive(Clone)]
pub struct Candle {
    pub timestamp: String,
    pub open: f64,
    pub close: f64,
    pub high: f64,
    pub low: f64,
    pub volume: f64,
    pub amount: Option<f64>,
}

#[derive(Clone)]
pub struct MaLine {
    pub period: usize,
    pub values: Vec<f64>, // same length as candles; NaN where insufficient data
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Period {
    Daily,
    Weekly,
}

impl Period {
    pub fn label(&self) -> &str {
        match self {
            Period::Daily => "日线",
            Period::Weekly => "周线",
        }
    }
}

pub fn load_stock_list(data_dir: &Path) -> Vec<(StockInfo, PathBuf)> {
    let mut stocks = Vec::new();
    if let Ok(entries) = std::fs::read_dir(data_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let info_path = path.join("info.json");
                if let Ok(content) = std::fs::read_to_string(&info_path) {
                    if let Ok(info) = serde_json::from_str::<StockInfo>(&content) {
                        stocks.push((info, path));
                    }
                }
            }
        }
    }
    stocks.sort_by(|a, b| a.0.secid.cmp(&b.0.secid));
    stocks
}

pub fn load_candles(
    stock_dir: &Path,
    period: Period,
    ma_windows: &[usize],
) -> (Vec<Candle>, Vec<MaLine>) {
    let daily = read_daily_csv(&stock_dir.join("daily.csv"));
    let candles = match period {
        Period::Daily => daily,
        Period::Weekly => aggregate_weekly(&daily),
    };

    let ma_lines = build_ma_lines(&candles, ma_windows);

    (candles, ma_lines)
}

fn build_ma_lines(candles: &[Candle], ma_windows: &[usize]) -> Vec<MaLine> {
    let closes: Vec<f64> = candles.iter().map(|c| c.close).collect();
    ma_windows
        .iter()
        .map(|&p| MaLine {
            period: p,
            values: calculate_ma(&closes, p),
        })
        .collect()
}

fn read_daily_csv(csv_path: &Path) -> Vec<Candle> {
    let mut candles = Vec::new();

    let Ok(mut rdr) = csv::Reader::from_path(csv_path) else {
        return candles;
    };

    let headers = match rdr.headers() {
        Ok(h) => h.clone(),
        Err(_) => return candles,
    };

    let mut col_map: HashMap<String, usize> = HashMap::new();
    for (i, h) in headers.iter().enumerate() {
        col_map.insert(h.to_lowercase().trim().to_string(), i);
    }

    let get_f64 =
        |record: &csv::StringRecord, name: &str, map: &HashMap<String, usize>| -> Option<f64> {
            map.get(name)
                .and_then(|&i| record.get(i))
                .and_then(|s| s.parse().ok())
        };

    // Accept either "date" (new schema) or "timestamp" (legacy) for the date column.
    let date_col = col_map
        .get("date")
        .copied()
        .or_else(|| col_map.get("timestamp").copied());

    for result in rdr.records() {
        let Ok(record) = result else { continue };

        let ts = date_col
            .and_then(|i| record.get(i))
            .unwrap_or("")
            .to_string();

        let (Some(o), Some(c), Some(h), Some(l)) = (
            get_f64(&record, "open", &col_map),
            get_f64(&record, "close", &col_map),
            get_f64(&record, "high", &col_map),
            get_f64(&record, "low", &col_map),
        ) else {
            continue;
        };

        let v = get_f64(&record, "volume", &col_map).unwrap_or(0.0);
        let amount = get_f64(&record, "amount", &col_map).filter(|value| *value > 0.0);
        candles.push(Candle {
            timestamp: ts,
            open: o,
            close: c,
            high: h,
            low: l,
            volume: v,
            amount,
        });
    }

    candles
}

/// Group daily candles into ISO weeks (Mon–Sun) starting from the first day's
/// Monday. Each output candle uses the first day's open, the last day's close,
/// the max high, the min low, and summed volume.
fn aggregate_weekly(daily: &[Candle]) -> Vec<Candle> {
    let mut weekly: Vec<Candle> = Vec::new();
    let mut current_week: Option<i64> = None;

    for c in daily {
        let week = match parse_ymd(&c.timestamp).map(|(y, m, d)| week_key(y, m, d)) {
            Some(w) => w,
            None => continue,
        };

        match current_week {
            Some(cw) if cw == week => {
                let last = weekly.last_mut().unwrap();
                last.high = last.high.max(c.high);
                last.low = last.low.min(c.low);
                last.close = c.close;
                last.volume += c.volume;
                last.amount = match (last.amount, c.amount) {
                    (Some(lhs), Some(rhs)) => Some(lhs + rhs),
                    _ => None,
                };
                last.timestamp = c.timestamp.clone();
            }
            _ => {
                weekly.push(c.clone());
                current_week = Some(week);
            }
        }
    }

    weekly
}

/// Parse a "YYYY-MM-DD[...]" timestamp into (year, month, day). Returns None
/// if the leading 10 characters don't form a valid date.
fn parse_ymd(ts: &str) -> Option<(i32, u32, u32)> {
    if ts.len() < 10 {
        return None;
    }
    let bytes = ts.as_bytes();
    if bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let y: i32 = ts[0..4].parse().ok()?;
    let m: u32 = ts[5..7].parse().ok()?;
    let d: u32 = ts[8..10].parse().ok()?;
    Some((y, m, d))
}

/// Compute a Monday-aligned week index (ISO-like). Uses Howard Hinnant's
/// civil-from-days algorithm so we stay dependency-free.
fn week_key(y: i32, m: u32, d: u32) -> i64 {
    let year = if m <= 2 { y - 1 } else { y };
    let era = if year >= 0 {
        year / 400
    } else {
        (year - 399) / 400
    };
    let yoe = (year - era * 400) as i64; // 0..=399
    let mp: i64 = if m > 2 { m as i64 - 3 } else { m as i64 + 9 };
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era as i64 * 146097 + doe - 719468; // days since 1970-01-01 (Thu)
    (days + 3).div_euclid(7) // Monday-aligned week index
}

fn calculate_ma(closes: &[f64], period: usize) -> Vec<f64> {
    let n = closes.len();
    let mut result = vec![f64::NAN; n];
    if n < period || period == 0 {
        return result;
    }
    let mut sum: f64 = closes[..period].iter().sum();
    result[period - 1] = sum / period as f64;
    for i in period..n {
        sum += closes[i] - closes[i - period];
        result[i] = sum / period as f64;
    }
    result
}

// ── Arc structures (越振越强, `arc_*` columns) ──
//
// Stripped of its candles the shape is two strokes: a steep rally, then a long
// stretch that drifts gently upward. The fight that follows the rally keeps
// being settled a little higher, and the lows of that stretch trace a rising
// arc. The detector measures exactly that, with three least-squares fits and
// no notion of individual swings:
//
//   rally    a straight stroke from the low it started at to its top
//   centre   a line through the closes since the top (the "震荡重心")
//   bottoms  a parabola hugging the underside of the lows since the top
//
// Every bar that is a local high is tried as the rally's top; the stretch from
// there to the latest bar must then pass the checks in `arc_at`. Each top that
// does is one `ArcStructure` — a stock usually has none, sometimes one or two.
// Filters read them through the `arc_*` columns.
//
// Prices are handled as ln(price). Tolerances are fractions of the rally's
// height or of `sigma`, the scatter of the closes around the centre line, so
// the same rules hold for any volatility and timeframe. They were calibrated
// on the shapes kept as `arc_sketches` and `arc_example` in the tests.

/// Only the most recent bars are searched.
const ARC_MAX_BARS: usize = 500;
/// The stretch after the rally lasts at least this many bars, and at least
/// three times as long as the rally — which itself is at most
/// `ARC_MAX_RALLY_BARS`.
const ARC_MIN_BARS: usize = 20;
const ARC_MAX_RALLY_BARS: usize = 60;
/// A rally top is the highest high within this many bars either side (the
/// first of them, if several bars share it).
const ARC_TOP_WINDOW: usize = 5;
/// The rally gains at least 8% (as ln)…
const ARC_MIN_RALLY: f64 = 0.076_961_041;
/// …in one direct leg: averaged over its bars, the close has covered at least
/// this share of the way up. A straight climb scores 0.5; a long base that
/// ends in a spike scores close to 0, and only the spike counts as the rally.
const ARC_MIN_RALLY_FILL: f64 = 0.4;
/// No low since the top gives back more than this share of the rally.
const ARC_MAX_GIVEBACK: f64 = 0.75;
/// The centre line starts at least this far up the rally…
const ARC_MIN_CENTRE_START: f64 = 0.35;
/// …rises over the whole stretch by this share of the rally's height (it must
/// rise, but once it has climbed a full rally the breakout has already run)…
const ARC_CENTRE_RISE: (f64, f64) = (0.05, 1.0);
/// …and is at least this many times less steep than the rally.
const ARC_MIN_STEEPER: f64 = 5.5;
/// Scatter of the closes around the centre line. A typical swing is about
/// 3 sigma, so this caps swings at 0.75 of the rally's height…
const ARC_MAX_SCATTER: f64 = 0.25;
/// …and at 50% in absolute terms (ln 1.5 / 3).
const ARC_MAX_SCATTER_ABS: f64 = 0.135_155_036;
/// It is a fight, not a glide: the closes cross the centre line this often.
const ARC_MIN_CROSSINGS: usize = 3;
/// The latest close is no further below the centre line than the first of
/// these (in sigmas; it would be breaking down), nor above it than the second.
const ARC_LAST_CLOSE: (f64, f64) = (-2.0, 3.5);
/// "重心不断抬高": cut into quarters, each one's mean close is higher than the
/// one before; a quarter may fall short by at most this many sigmas.
const ARC_CENTRE_SLACK: f64 = 0.5;
/// The swings keep a rhythm: scatter in the second half relative to the first.
const ARC_HALF_SCATTER: (f64, f64) = (0.4, 2.5);
/// The bottoms fit is asymmetric least squares: a low above the curve weighs
/// this much, a low below it `1 -` this much. That settles the curve on the
/// underside of the lows, while a stray shadow or two cannot drag it down
/// ("个别例外"). Re-weighted for a fixed number of rounds.
const ARC_ENVELOPE_WEIGHT: f64 = 0.08;
const ARC_ENVELOPE_ROUNDS: usize = 12;
/// The arc under the lows rises by at least this many sigmas…
const ARC_MIN_LIFT: f64 = 1.0;
/// …and accelerates: it bends upward, its midpoint hanging below the straight
/// line between its ends by at least this share of the lift. A straight line
/// scores 0, so steadily rising lows don't qualify, let alone flattening ones…
const ARC_MIN_BEND: f64 = 0.10;
/// …does not sink below its own starting point by more than this many sigmas
/// (a "U" is a decline followed by a recovery, not an arc)…
const ARC_MAX_SINK: f64 = 1.25;
/// …is undercut by no single low by more than this…
const ARC_MAX_UNDERCUT: f64 = 2.5;
/// …and the latest low isn't falling out of it by more than this.
const ARC_LOW_SLACK: f64 = 0.5;

pub const ARC_COLUMNS: [&str; 8] = [
    "arc_len", "arc_run", "arc_peak", "arc_amp", "arc_rise", "arc_lift", "arc_bend", "arc_break",
];

/// One rally-then-rising-consolidation structure that is still in force on the
/// latest bar. Percentages are plain price changes (10.0 = +10%).
#[derive(Debug, Clone, PartialEq)]
pub struct ArcStructure {
    /// Bars from the low the rally started at to the latest bar.
    pub len: usize,
    /// The rally: starting low to its top, in %.
    pub run: f64,
    /// Starting low to the highest high reached since, in % — how far the
    /// whole move has already carried. Equals `run` until the rally's top is
    /// exceeded.
    pub peak: f64,
    /// Typical swing since the top (3 sigma around the centre line), in %.
    pub amp: f64,
    /// How far the centre line has risen since the top, in %.
    pub rise: f64,
    /// How far the arc under the lows has risen since the top, in %.
    pub lift: f64,
    /// Curvature of that arc: how far its midpoint hangs below the straight
    /// line between its ends, in % of the lift. At least 10 (accelerating);
    /// 20–60 is a clear arc.
    pub bend: f64,
    /// Latest close vs the highest high since the rally's top, in % (≤ 0).
    pub breakout: f64,
    /// Where the structure sits on the chart, for drawing it.
    pub shape: ArcShape,
}

/// The bars and fitted curves of an `ArcStructure`, as indices into the
/// candles it was found in.
#[derive(Debug, Clone, PartialEq)]
pub struct ArcShape {
    /// The low the rally started from.
    pub start: usize,
    /// The rally's top.
    pub top: usize,
    /// The latest bar; the centre line and the arc run from `top + 1` to here.
    pub end: usize,
    /// Centre line: ln(price) = .0 + .1 · (bars since the top).
    centre: (f64, f64),
    /// Arc under the lows: ln(price) = c0 + c1·x + c2·x², with x running from
    /// 0 at the top to 1 at the latest bar.
    arc: [f64; 3],
}

impl ArcShape {
    pub fn centre_price(&self, bar: f64) -> f64 {
        (self.centre.0 + self.centre.1 * (bar - self.top as f64)).exp()
    }

    pub fn arc_price(&self, bar: f64) -> f64 {
        let x = (bar - self.top as f64) / (self.end - self.top) as f64;
        (self.arc[0] + self.arc[1] * x + self.arc[2] * x * x).exp()
    }
}

/// Log prices of the bars being searched, plus running sums that make the
/// cheap checks O(1) per candidate top.
struct ArcSeries {
    high: Vec<f64>,
    low: Vec<f64>,
    close: Vec<f64>,
    /// `sum_close[i]` = sum of `close[..i]`; `sum_bar_close` weights each by its index.
    sum_close: Vec<f64>,
    sum_bar_close: Vec<f64>,
    /// `low_from[i]` = lowest low in `low[i..]`.
    low_from: Vec<f64>,
}

/// Every arc structure that ends on the last of `candles`, longest first.
pub fn find_arc_structures(candles: &[Candle]) -> Vec<ArcStructure> {
    let window = candles.len().saturating_sub(ARC_MAX_BARS);
    let recent = &candles[window..];
    // Forward-adjusted prices can be zero or negative far in the past; work on
    // the stretch after the last such bar, where logs are defined.
    let start = recent
        .iter()
        .rposition(|c| !(c.low > 0.0 && c.high > 0.0 && c.close > 0.0))
        .map_or(0, |i| i + 1);
    let recent = &recent[start..];
    let first_bar = window + start;
    let n = recent.len();
    if n < ARC_MIN_BARS + 4 {
        return Vec::new();
    }

    let close: Vec<f64> = recent.iter().map(|c| c.close.ln()).collect();
    let low: Vec<f64> = recent.iter().map(|c| c.low.ln()).collect();
    let mut sum_close = vec![0.0; n + 1];
    let mut sum_bar_close = vec![0.0; n + 1];
    for (i, c) in close.iter().enumerate() {
        sum_close[i + 1] = sum_close[i] + c;
        sum_bar_close[i + 1] = sum_bar_close[i] + i as f64 * c;
    }
    let mut low_from = low.clone();
    for i in (0..n - 1).rev() {
        low_from[i] = low_from[i].min(low_from[i + 1]);
    }
    let series = ArcSeries {
        high: recent.iter().map(|c| c.high.ln()).collect(),
        low,
        close,
        sum_close,
        sum_bar_close,
        low_from,
    };

    let last = n - 1;
    let mut out: Vec<ArcStructure> = (3..=last - ARC_MIN_BARS)
        .filter(|&top| {
            let before = top.saturating_sub(ARC_TOP_WINDOW)..top;
            let after = top + 1..=(top + ARC_TOP_WINDOW).min(last);
            series.high[before].iter().all(|&h| h < series.high[top])
                && series.high[after].iter().all(|&h| h <= series.high[top])
        })
        .filter_map(|top| arc_at(&series, top))
        .collect();
    for arc in &mut out {
        arc.shape.start += first_bar;
        arc.shape.top += first_bar;
        arc.shape.end += first_bar;
    }
    out.sort_by(|a, b| b.len.cmp(&a.len));
    out
}

/// Judges the structure whose rally tops out at bar `top`: the rally leading
/// up to it and the stretch from there to the latest bar.
fn arc_at(s: &ArcSeries, top: usize) -> Option<ArcStructure> {
    let last = s.close.len() - 1;
    let len = last - top; // bars since the top
    let top_high = s.high[top];

    // The rally stroke. Walking back from the top over successively lower
    // lows, keep the lowest one from which the climb is still one direct leg.
    let reach = (len / 3).clamp(3, ARC_MAX_RALLY_BARS);
    let mut start = top - 1;
    let mut lowest = s.low[start];
    for i in (top.saturating_sub(reach)..top).rev() {
        if s.low[i] > lowest {
            continue;
        }
        lowest = s.low[i];
        let between = top - i - 1; // closes strictly inside the stroke
        let fill = if between == 0 {
            1.0
        } else {
            let mean_close = (s.sum_close[top] - s.sum_close[i + 1]) / between as f64;
            (mean_close - s.low[i]) / (top_high - s.low[i])
        };
        if fill >= ARC_MIN_RALLY_FILL {
            start = i;
        }
    }
    let rally_bars = (top - start) as f64;
    let rally = top_high - s.low[start];
    if rally < ARC_MIN_RALLY || s.high[start..top].iter().any(|&h| h > top_high) {
        return None;
    }
    // The gains are held: giving most of the rally back is a round trip, not
    // a confirmation of it.
    if top_high - s.low_from[top + 1] > ARC_MAX_GIVEBACK * rally {
        return None;
    }

    // Centre of gravity: the least-squares line through the closes since the
    // top, as `base + slope · t` with t = 1 at the first bar after it.
    let n = len as f64;
    let sum_t = n * (n + 1.0) / 2.0;
    let sum_tt = n * (n + 1.0) * (2.0 * n + 1.0) / 6.0;
    let sum_y = s.sum_close[last + 1] - s.sum_close[top + 1];
    let sum_ty = s.sum_bar_close[last + 1] - s.sum_bar_close[top + 1] - top as f64 * sum_y;
    let slope = (n * sum_ty - sum_t * sum_y) / (n * sum_tt - sum_t * sum_t);
    let base = (sum_y - slope * sum_t) / n;
    let rise = slope * n;
    if rise < ARC_CENTRE_RISE.0 * rally
        || rise > ARC_CENTRE_RISE.1 * rally
        || slope * ARC_MIN_STEEPER > rally / rally_bars
        || base - s.low[start] < ARC_MIN_CENTRE_START * rally
    {
        return None;
    }

    // How the closes sit around that line.
    let closes = &s.close[top + 1..];
    let residuals: Vec<f64> = closes
        .iter()
        .enumerate()
        .map(|(i, c)| c - (base + slope * (i + 1) as f64))
        .collect();
    let sigma = scatter(&residuals);
    if sigma <= 0.0 || sigma > ARC_MAX_SCATTER * rally || sigma > ARC_MAX_SCATTER_ABS {
        return None;
    }
    let mut sides = residuals.iter().filter(|r| **r != 0.0).map(|r| *r > 0.0);
    let mut side = sides.next()?;
    let mut crossings = 0;
    for next in sides {
        if next != side {
            crossings += 1;
            side = next;
        }
    }
    let latest = residuals[len - 1] / sigma;
    if crossings < ARC_MIN_CROSSINGS || latest < ARC_LAST_CLOSE.0 || latest > ARC_LAST_CLOSE.1 {
        return None;
    }
    // The centre keeps rising, quarter by quarter…
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let quarters: Vec<f64> = (0..4)
        .map(|k| mean(&closes[k * len / 4..(k + 1) * len / 4]))
        .collect();
    if quarters.windows(2).any(|q| q[1] - q[0] < -ARC_CENTRE_SLACK * sigma) {
        return None;
    }
    // …and neither half swings much more wildly than the other.
    let (first_half, second_half) = residuals.split_at(len / 2);
    let ratio = scatter(second_half) / scatter(first_half).max(1e-12);
    if ratio < ARC_HALF_SCATTER.0 || ratio > ARC_HALF_SCATTER.1 {
        return None;
    }

    // The bottoms: a parabola under the lows, on time scaled so the stretch
    // runs from x = 0 at the top to x = 1 at the latest bar.
    let lows = &s.low[top + 1..];
    let x = |i: usize| (i + 1) as f64 / n;
    let curve = |c: &[f64; 3], i: usize| c[0] + c[1] * x(i) + c[2] * x(i) * x(i);
    let mut weights = vec![1.0; len];
    let mut arc = [0.0; 3];
    for _ in 0..ARC_ENVELOPE_ROUNDS {
        arc = fit_parabola((0..len).map(|i| (x(i), lows[i], weights[i])))?;
        for (i, w) in weights.iter_mut().enumerate() {
            *w = if lows[i] < curve(&arc, i) {
                1.0 - ARC_ENVELOPE_WEIGHT
            } else {
                ARC_ENVELOPE_WEIGHT
            };
        }
    }
    let [_, b, c] = arc;
    let lift = b + c; // rise of the arc from x = 0 to x = 1
    if lift < ARC_MIN_LIFT * sigma {
        return None;
    }
    // Midpoint sag of a parabola below its chord is c/4.
    let bend = (c / 4.0) / lift;
    // Lowest point of the arc relative to where it starts.
    let vertex = if c > 1e-12 { -b / (2.0 * c) } else { 0.0 };
    let sink = if vertex >= 1.0 {
        (-(b + c)).max(0.0)
    } else if vertex > 0.0 {
        b * b / (4.0 * c)
    } else {
        0.0
    };
    let deepest = (0..len).map(|i| lows[i] - curve(&arc, i)).fold(f64::MAX, f64::min);
    if bend < ARC_MIN_BEND
        || sink > ARC_MAX_SINK * sigma
        || -deepest > ARC_MAX_UNDERCUT * sigma
        || lows[len - 1] - curve(&arc, len - 1) < -ARC_LOW_SLACK * sigma
    {
        return None;
    }

    let highest = s.high[top..].iter().copied().fold(f64::MIN, f64::max);
    let pct = |log_ratio: f64| (log_ratio.exp() - 1.0) * 100.0;
    Some(ArcStructure {
        len: last - start,
        run: pct(rally),
        peak: pct(highest - s.low[start]),
        amp: pct(3.0 * sigma),
        rise: pct(rise),
        lift: pct(lift),
        bend: bend * 100.0,
        breakout: pct(s.close[last] - highest),
        shape: ArcShape {
            start,
            top,
            end: last,
            centre: (base, slope),
            arc,
        },
    })
}

/// Root-mean-square distance of `values` from their own mean.
fn scatter(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    (values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64).sqrt()
}

/// Weighted least-squares `y = a + b·x + c·x²` over `(x, y, weight)` points,
/// returned as `[a, b, c]`. `None` when the points don't determine a parabola.
fn fit_parabola(points: impl Iterator<Item = (f64, f64, f64)>) -> Option<[f64; 3]> {
    // Normal equations, solved by Gaussian elimination with partial pivoting.
    let mut m = [[0.0f64; 4]; 3];
    for (x, y, w) in points {
        let powers = [1.0, x, x * x];
        for r in 0..3 {
            for c in 0..3 {
                m[r][c] += w * powers[r] * powers[c];
            }
            m[r][3] += w * powers[r] * y;
        }
    }
    for col in 0..3 {
        let pivot = (col..3).max_by(|&a, &b| m[a][col].abs().total_cmp(&m[b][col].abs()))?;
        if m[pivot][col].abs() < 1e-12 {
            return None;
        }
        m.swap(col, pivot);
        for row in 0..3 {
            if row != col {
                let factor = m[row][col] / m[col][col];
                for k in col..4 {
                    m[row][k] -= factor * m[col][k];
                }
            }
        }
    }
    Some([m[0][3] / m[0][0], m[1][3] / m[1][1], m[2][3] / m[2][2]])
}

// ── Data filter types and logic ──
//
// A filter bundle is a list of user-editable lines. Each line is
//
//     [period ':'] bool_expr
//
// where `period` is `day`/`daily` or `week`/`weekly` (omitted ⟹ the bundle's
// default periods), and `bool_expr` combines comparisons with `and` / `or`
// (`and` binds tighter) and optional parentheses:
//
//     week: close > ma100
//     day: close > ma120 and (ma60 > ma120 or ma120 > ma120[-1])
//
// Lines are ANDed together: a stock matches only if every line holds.
//
// The `arc_*` columns describe an arc structure (see above) rather than a bar.
// A stock can hold several structures at once, so they are bound like a
// variable: within one line, on one timeframe, every `arc_*` refers to the
// same structure, and the line holds if any one structure satisfies it. To
// describe one structure with several conditions, join them with `and` on a
// single line.

#[derive(Debug, Clone)]
pub struct DataFilter {
    pub left: FilterOperand,
    pub operator: FilterOperator,
    pub right: FilterOperand,
    #[allow(dead_code)]
    pub raw: String,
}

/// Boolean combination of comparisons within a single filter line.
#[derive(Debug, Clone)]
pub enum FilterExpr {
    Cmp(DataFilter),
    And(Vec<FilterExpr>),
    Or(Vec<FilterExpr>),
}

impl FilterExpr {
    /// Visits every comparison in the tree.
    fn for_each_cmp(&self, f: &mut impl FnMut(&DataFilter)) {
        match self {
            FilterExpr::Cmp(c) => f(c),
            FilterExpr::And(v) | FilterExpr::Or(v) => {
                for e in v {
                    e.for_each_cmp(f);
                }
            }
        }
    }

    /// Visits every column reference, divisors included.
    fn for_each_operand(&self, f: &mut impl FnMut(&FilterOperand)) {
        self.for_each_cmp(&mut |c| {
            for side in [&c.left, &c.right] {
                f(side);
                if let Some(d) = &side.divisor {
                    f(d);
                }
            }
        });
    }

    fn uses_arc(&self) -> bool {
        let mut found = false;
        self.for_each_operand(&mut |op| found |= is_arc_column(&op.column));
        found
    }
}

fn is_arc_column(column: &str) -> bool {
    column.starts_with("arc_")
}

/// One line of a filter bundle: an optional timeframe prefix plus the
/// expression that must hold on that timeframe.
#[derive(Debug, Clone)]
pub struct FilterLine {
    /// `None` ⟹ evaluate on the bundle's configured periods.
    pub period: Option<Period>,
    pub expr: FilterExpr,
    #[allow(dead_code)]
    pub raw: String,
}

#[derive(Debug, Clone)]
pub struct FilterOperand {
    pub column: String,
    pub offset: usize,              // 0 = latest row, N = N bars ago
    pub divisor: Option<Box<FilterOperand>>, // Some(d) ⟹ this value / d
}

#[derive(Debug, Clone, Copy)]
pub enum FilterOperator {
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
}

/// Parses the lines of a bundle, skipping blank lines and `#` / `//` comments.
/// Errors are prefixed with the 1-based line number the user sees in the editor.
pub fn parse_filter_lines<S: AsRef<str>>(lines: &[S]) -> Result<Vec<FilterLine>, String> {
    let mut parsed = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if is_blank_or_comment(line.as_ref()) {
            continue;
        }
        match parse_filter_line(line.as_ref()) {
            Ok(l) => parsed.push(l),
            Err(e) => return Err(format!("第 {} 行: {}", i + 1, e)),
        }
    }
    Ok(parsed)
}

pub fn is_blank_or_comment(line: &str) -> bool {
    let t = line.trim();
    t.is_empty() || t.starts_with('#') || t.starts_with("//")
}

/// Parses a single line: optional `day:` / `week:` prefix + boolean expression.
pub fn parse_filter_line(line: &str) -> Result<FilterLine, String> {
    let raw = line.trim();
    let (period, body) = split_period_prefix(raw)?;
    let expr = parse_filter_expr(body)?;
    Ok(FilterLine {
        period,
        expr,
        raw: raw.to_string(),
    })
}

fn split_period_prefix(s: &str) -> Result<(Option<Period>, &str), String> {
    // Only a leading `<word>:` counts as a prefix; nothing else in the grammar
    // uses ':' so an unrecognised head is a typo worth reporting.
    let Some(colon) = s.find(':') else {
        return Ok((None, s));
    };
    let head = s[..colon].trim().to_ascii_lowercase();
    let rest = &s[colon + 1..];
    match head.as_str() {
        "day" | "daily" | "d" => Ok((Some(Period::Daily), rest)),
        "week" | "weekly" | "w" => Ok((Some(Period::Weekly), rest)),
        _ => Err(format!(
            "\":\" 只能用于行首的周期前缀 (day: 或 week:), 这里是 \"{}\"",
            head
        )),
    }
}

/// `or` has the lowest precedence, then `and`, then a bare comparison.
pub fn parse_filter_expr(s: &str) -> Result<FilterExpr, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("条件为空".to_string());
    }
    if let Some(parts) = split_top_level(s, "or") {
        return Ok(FilterExpr::Or(parse_all(&parts)?));
    }
    if let Some(parts) = split_top_level(s, "and") {
        return Ok(FilterExpr::And(parse_all(&parts)?));
    }
    if let Some(inner) = strip_outer_parens(s)? {
        return parse_filter_expr(inner);
    }
    Ok(FilterExpr::Cmp(parse_data_filter(s)?))
}

fn parse_all(parts: &[&str]) -> Result<Vec<FilterExpr>, String> {
    parts.iter().map(|p| parse_filter_expr(p)).collect()
}

/// Splits on a keyword that appears outside parentheses and on word
/// boundaries. Returns `None` when the keyword doesn't occur at that level.
fn split_top_level<'a>(s: &'a str, kw: &str) -> Option<Vec<&'a str>> {
    let b = s.as_bytes();
    let k = kw.as_bytes();
    let mut depth: i32 = 0;
    let mut parts: Vec<&str> = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < b.len() {
        match b[i] {
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                i += 1;
            }
            _ => {
                let end = i + k.len();
                let matches = depth == 0
                    && end <= b.len()
                    && b[i..end].eq_ignore_ascii_case(k)
                    && (i == 0 || !is_word_byte(b[i - 1]))
                    && (end >= b.len() || !is_word_byte(b[end]));
                if matches {
                    parts.push(&s[start..i]);
                    start = end;
                    i = end;
                } else {
                    i += 1;
                }
            }
        }
    }
    if parts.is_empty() {
        return None;
    }
    parts.push(&s[start..]);
    Some(parts)
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// `Some(inner)` when the whole expression is wrapped in one pair of parens.
/// Errors on unbalanced parentheses so typos surface at parse time.
fn strip_outer_parens(s: &str) -> Result<Option<&str>, String> {
    let mut depth: i32 = 0;
    let mut closes_at_end = false;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return Err(format!("括号不匹配: {}", s));
                }
                closes_at_end = depth == 0 && i + 1 == s.len();
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err(format!("括号不匹配: {}", s));
    }
    if s.starts_with('(') && closes_at_end {
        Ok(Some(&s[1..s.len() - 1]))
    } else {
        Ok(None)
    }
}

pub fn parse_data_filter(expr: &str) -> Result<DataFilter, String> {
    let (left_str, op, right_str) = if let Some(pos) = expr.find(">=") {
        (&expr[..pos], FilterOperator::GreaterThanOrEqual, &expr[pos + 2..])
    } else if let Some(pos) = expr.find("<=") {
        (&expr[..pos], FilterOperator::LessThanOrEqual, &expr[pos + 2..])
    } else if let Some(pos) = expr.find('>') {
        (&expr[..pos], FilterOperator::GreaterThan, &expr[pos + 1..])
    } else if let Some(pos) = expr.find('<') {
        (&expr[..pos], FilterOperator::LessThan, &expr[pos + 1..])
    } else {
        return Err(format!("缺少比较运算符 (>, >=, <, <=): {}", expr.trim()));
    };

    let left = parse_filter_operand(left_str.trim())?;
    let right = parse_filter_operand(right_str.trim())?;

    Ok(DataFilter {
        left,
        operator: op,
        right,
        raw: expr.to_string(),
    })
}

fn parse_filter_operand(s: &str) -> Result<FilterOperand, String> {
    if s.is_empty() {
        return Err("缺少操作数".to_string());
    }

    // Support division: "price/price[200]" means current price / price 200 bars ago.
    if let Some(slash_pos) = s.find('/') {
        let num_str = s[..slash_pos].trim();
        let den_str = s[slash_pos + 1..].trim();
        let numerator = parse_single_operand(num_str)?;
        let denominator = parse_single_operand(den_str)?;
        return Ok(FilterOperand {
            column: numerator.column,
            offset: numerator.offset,
            divisor: Some(Box::new(denominator)),
        });
    }

    parse_single_operand(s)
}

fn parse_single_operand(s: &str) -> Result<FilterOperand, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("缺少操作数".to_string());
    }

    let (column, offset) = if let Some(bracket_start) = s.find('[') {
        let bracket_end = s
            .find(']')
            .ok_or_else(|| format!("缺少 ]: {}", s))?;
        if bracket_end < bracket_start || !s[bracket_end + 1..].trim().is_empty() {
            return Err(format!("无效的操作数: {}", s));
        }
        let col = s[..bracket_start].trim();
        let offset_str = s[bracket_start + 1..bracket_end].trim();
        let offset_val: i64 = offset_str
            .parse()
            .map_err(|_| format!("无效的偏移量: {}", s))?;
        // Accept price[200] (positive = bars ago) and price[-200] (negative = bars ago)
        (col.to_string(), offset_val.unsigned_abs() as usize)
    } else {
        (s.to_string(), 0)
    };

    // Numeric literals (e.g. the `2.0` in `price/price[-200] <= 2.0`) pass
    // through as-is; everything else must name a column we can resolve.
    if column.parse::<f64>().is_ok() {
        return Ok(FilterOperand {
            column,
            offset: 0,
            divisor: None,
        });
    }

    // Alias "price" → "close"
    let column = column.to_ascii_lowercase();
    let column = if column == "price" {
        "close".to_string()
    } else {
        column
    };

    validate_column(&column)?;
    if offset != 0 && is_arc_column(&column) {
        return Err(format!("{} 描述的是整段走势, 不支持 [N]", column));
    }

    Ok(FilterOperand { column, offset, divisor: None })
}

/// Rejects unknown columns at parse time — otherwise a typo like `clsoe > ma10`
/// would parse fine and silently match nothing.
fn validate_column(column: &str) -> Result<(), String> {
    if matches!(column, "open" | "close" | "high" | "low" | "volume") {
        return Ok(());
    }
    if let Some(rest) = column.strip_prefix("ma") {
        return match rest.parse::<usize>() {
            Ok(p) if p > 0 => Ok(()),
            _ => Err(format!("无效的均线: {} (示例: ma10, ma250)", column)),
        };
    }
    if ARC_COLUMNS.contains(&column) {
        return Ok(());
    }
    Err(format!(
        "未知字段: {} (可用: close/price, open, high, low, volume, maN, {})",
        column,
        ARC_COLUMNS.join(", ")
    ))
}

/// A stock matches when every line holds. A line with an explicit `day:` /
/// `week:` prefix is evaluated on that timeframe; an unprefixed line is
/// evaluated on `default_periods` and passes if it holds on any of them.
///
/// A line reading `arc_*` columns holds on a timeframe if any one arc
/// structure found there satisfies the whole line.
pub fn evaluate_filters_on_stock(
    stock_dir: &Path,
    lines: &[FilterLine],
    ma_windows: &[usize],
    default_periods: &[Period],
) -> bool {
    if lines.is_empty() {
        return true;
    }

    // Union configured windows with any `maN` references in the filters so
    // expressions like `ma120 > ma240` resolve even if those periods aren't
    // in the user's chart config.
    let mut windows: Vec<usize> = ma_windows.to_vec();
    for line in lines {
        line.expr.for_each_operand(&mut |op| {
            if let Some(p) = op
                .column
                .strip_prefix("ma")
                .and_then(|s| s.parse::<usize>().ok())
            {
                if !windows.contains(&p) {
                    windows.push(p);
                }
            }
        });
    }

    let defaults: &[Period] = if default_periods.is_empty() {
        &[Period::Daily]
    } else {
        default_periods
    };

    let mut data = StockData {
        dir: stock_dir,
        windows: &windows,
        candles: HashMap::new(),
        arcs: HashMap::new(),
    };

    // Plain lines go first: they are cheap and reject most stocks before any
    // arc detection has to run.
    let (arc_lines, plain_lines): (Vec<&FilterLine>, Vec<&FilterLine>) =
        lines.iter().partition(|l| l.expr.uses_arc());
    plain_lines.into_iter().chain(arc_lines).all(|line| match line.period {
        Some(p) => data.line_holds(&line.expr, p),
        None => defaults.iter().any(|&p| data.line_holds(&line.expr, p)),
    })
}

/// The arc structure to draw on a chart of `period` for a stock this bundle
/// matched: one that satisfies the bundle's `arc_*` lines on that timeframe,
/// preferring the longest. `None` when the bundle has
/// no `arc_*` line for this timeframe or nothing qualifies there (the stock
/// may have matched on the other timeframe, or the data has changed since).
pub fn arc_to_draw(
    lines: &[FilterLine],
    default_periods: &[Period],
    period: Period,
    candles: &[Candle],
    ma_lines: &[MaLine],
) -> Option<ArcStructure> {
    let defaults: &[Period] = if default_periods.is_empty() {
        &[Period::Daily]
    } else {
        default_periods
    };
    let arc_lines: Vec<&FilterLine> = lines
        .iter()
        .filter(|l| l.expr.uses_arc())
        .filter(|l| l.period.map_or(defaults.contains(&period), |p| p == period))
        .collect();
    if arc_lines.is_empty() || candles.is_empty() {
        return None;
    }
    let satisfied = |arc: &ArcStructure| {
        let ctx = FilterContext { candles, ma_lines, arc: Some(arc) };
        arc_lines.iter().filter(|l| eval_expr(&ctx, &l.expr)).count()
    };
    find_arc_structures(candles)
        .into_iter()
        .map(|arc| (satisfied(&arc), arc))
        .filter(|(n, _)| *n > 0)
        .max_by_key(|(n, arc)| (*n, arc.len))
        .map(|(_, arc)| arc)
}

/// One stock's candles and arc structures, each computed at most once per
/// timeframe and shared across lines.
struct StockData<'a> {
    dir: &'a Path,
    windows: &'a [usize],
    candles: HashMap<Period, (Vec<Candle>, Vec<MaLine>)>,
    arcs: HashMap<Period, Vec<ArcStructure>>,
}

impl StockData<'_> {
    /// Same result as `load_candles`, but the CSV is read only once: weekly
    /// candles are aggregated from the daily ones already in memory.
    fn load(&mut self, period: Period) {
        if !self.candles.contains_key(&Period::Daily) {
            let daily = read_daily_csv(&self.dir.join("daily.csv"));
            let ma_lines = build_ma_lines(&daily, self.windows);
            self.candles.insert(Period::Daily, (daily, ma_lines));
        }
        if !self.candles.contains_key(&period) {
            let candles = aggregate_weekly(&self.candles[&Period::Daily].0);
            let ma_lines = build_ma_lines(&candles, self.windows);
            self.candles.insert(period, (candles, ma_lines));
        }
    }

    fn line_holds(&mut self, expr: &FilterExpr, period: Period) -> bool {
        self.load(period);
        let (candles, ma_lines) = &self.candles[&period];
        if candles.is_empty() {
            return false;
        }
        let holds_with = |arc: Option<&ArcStructure>| {
            eval_expr(&FilterContext { candles, ma_lines, arc }, expr)
        };
        if !expr.uses_arc() {
            return holds_with(None);
        }
        let arcs = self
            .arcs
            .entry(period)
            .or_insert_with(|| find_arc_structures(candles));
        // With no structure the `arc_*` comparisons are simply unresolved
        // (false), which still lets an `or` branch carry the line.
        if arcs.is_empty() {
            holds_with(None)
        } else {
            arcs.iter().any(|arc| holds_with(Some(arc)))
        }
    }
}

fn eval_expr(ctx: &FilterContext<'_>, expr: &FilterExpr) -> bool {
    match expr {
        FilterExpr::Cmp(f) => eval_cmp(ctx, f),
        FilterExpr::And(v) => v.iter().all(|e| eval_expr(ctx, e)),
        FilterExpr::Or(v) => v.iter().any(|e| eval_expr(ctx, e)),
    }
}

/// A comparison whose operands can't be resolved (not enough history, MA still
/// warming up) counts as false rather than aborting the whole bundle — that
/// keeps `or` branches usable on short series.
fn eval_cmp(ctx: &FilterContext<'_>, filter: &DataFilter) -> bool {
    let (Some(lv), Some(rv)) = (
        resolve_operand_value(ctx, &filter.left),
        resolve_operand_value(ctx, &filter.right),
    ) else {
        return false;
    };

    match filter.operator {
        FilterOperator::GreaterThan => lv > rv,
        FilterOperator::GreaterThanOrEqual => lv >= rv,
        FilterOperator::LessThan => lv < rv,
        FilterOperator::LessThanOrEqual => lv <= rv,
    }
}

struct FilterContext<'a> {
    candles: &'a [Candle],
    ma_lines: &'a [MaLine],
    /// The structure `arc_*` columns currently refer to, if any.
    arc: Option<&'a ArcStructure>,
}

fn resolve_operand_value(ctx: &FilterContext<'_>, operand: &FilterOperand) -> Option<f64> {
    let base = resolve_single_value(ctx, operand)?;
    if let Some(div) = &operand.divisor {
        let dv = resolve_single_value(ctx, div)?;
        if dv == 0.0 {
            return None;
        }
        Some(base / dv)
    } else {
        Some(base)
    }
}

fn resolve_single_value(ctx: &FilterContext<'_>, operand: &FilterOperand) -> Option<f64> {
    // Try parsing as numeric literal first (e.g. "50" in "cbw < 50")
    if let Ok(val) = operand.column.parse::<f64>() {
        return Some(val);
    }

    if is_arc_column(&operand.column) {
        let arc = ctx.arc?;
        return match operand.column.as_str() {
            "arc_len" => Some(arc.len as f64),
            "arc_run" => Some(arc.run),
            "arc_peak" => Some(arc.peak),
            "arc_amp" => Some(arc.amp),
            "arc_rise" => Some(arc.rise),
            "arc_lift" => Some(arc.lift),
            "arc_bend" => Some(arc.bend),
            "arc_break" => Some(arc.breakout),
            _ => None,
        };
    }

    // offset 0 = latest, offset N = N bars ago
    let idx = ctx.candles.len().checked_sub(operand.offset + 1)?;

    // MA columns (ma10, ma30, ma60, …) come from on-demand MA lines
    if let Some(period_str) = operand.column.strip_prefix("ma") {
        if let Ok(period) = period_str.parse::<usize>() {
            let ma = ctx.ma_lines.iter().find(|m| m.period == period)?;
            let val = *ma.values.get(idx)?;
            if val.is_nan() {
                return None;
            }
            return Some(val);
        }
    }

    let c = ctx.candles.get(idx)?;
    match operand.column.as_str() {
        "open" => Some(c.open),
        "close" => Some(c.close),
        "high" => Some(c.high),
        "low" => Some(c.low),
        "volume" => Some(c.volume),
        _ => None,
    }
}

// ── Filter progress types ──

#[derive(Clone)]
pub enum FilterStatus {
    Idle,
    /// Conditions were edited since the last run, so the cached match list was
    /// dropped — the tab must not show results that predate the edit.
    Stale,
    Running {
        progress_pct: f32,
        completed: usize,
        total: usize,
        matched: usize,
        current_stock: String,
    },
    Done {
        matched: usize,
        total: usize,
    },
    Error(String),
}

pub enum FilterMessage {
    Progress {
        completed: usize,
        total: usize,
        matched: usize,
        stock_label: String,
    },
    Finished(Result<(usize, usize), String>),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses editor-style lines, panicking on syntax errors.
    fn filter_lines(exprs: &[&str]) -> Vec<FilterLine> {
        parse_filter_lines(exprs).unwrap()
    }

    /// Writes `daily.csv` with the given closes (o/h/l/c all equal) into a
    /// fresh temp dir named after the test.
    fn stock_dir_with_closes(name: &str, closes: &[f64]) -> PathBuf {
        let dir = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut csv = String::from("date,open,close,high,low,volume,amount\n");
        for (i, v) in closes.iter().enumerate() {
            // Sequential calendar days starting 2025-01-01, so weekly
            // aggregation sees real week boundaries.
            let day = i as u32;
            let (m, d) = (day / 28 + 1, day % 28 + 1);
            csv.push_str(&format!("2025-{m:02}-{d:02},{v},{v},{v},{v},100,\n"));
        }
        std::fs::write(dir.join("daily.csv"), csv).unwrap();
        dir
    }

    #[test]
    fn parses_period_prefix() {
        let l = parse_filter_line("week: close > ma100").unwrap();
        assert_eq!(l.period, Some(Period::Weekly));
        let l = parse_filter_line("day: close > close[-10]").unwrap();
        assert_eq!(l.period, Some(Period::Daily));
        // Aliases and casing
        assert_eq!(
            parse_filter_line("WEEKLY: close > ma5").unwrap().period,
            Some(Period::Weekly)
        );
        // No prefix ⟹ bundle default
        assert_eq!(parse_filter_line("close > ma5").unwrap().period, None);
        // Unknown prefix is an error, not a silently ignored line
        assert!(parse_filter_line("month: close > ma5").is_err());
    }

    #[test]
    fn parses_and_or_with_precedence() {
        // `and` binds tighter: a or (b and c)
        let expr = parse_filter_expr("close > ma5 or ma5 > ma10 and ma10 > ma20").unwrap();
        let FilterExpr::Or(branches) = expr else {
            panic!("expected Or at the top level");
        };
        assert_eq!(branches.len(), 2);
        assert!(matches!(branches[0], FilterExpr::Cmp(_)));
        let FilterExpr::And(inner) = &branches[1] else {
            panic!("expected And under Or");
        };
        assert_eq!(inner.len(), 2);
    }

    #[test]
    fn parentheses_override_precedence() {
        let expr = parse_filter_expr("(close > ma5 or ma5 > ma10) and ma10 > ma20").unwrap();
        let FilterExpr::And(branches) = expr else {
            panic!("expected And at the top level");
        };
        assert!(matches!(branches[0], FilterExpr::Or(_)));

        // Unbalanced parens are rejected rather than silently mis-parsed.
        assert!(parse_filter_expr("(close > ma5").is_err());
        assert!(parse_filter_expr("close > ma5)").is_err());
    }

    #[test]
    fn rejects_unknown_columns_and_operators() {
        assert!(parse_filter_expr("clsoe > ma10").is_err());
        assert!(parse_filter_expr("ma0 > ma10").is_err());
        assert!(parse_filter_expr("close ma10").is_err());
        assert!(parse_filter_expr("close > ").is_err());
        // Keyword-lookalikes inside identifiers must not split the expression
        assert!(matches!(
            parse_filter_expr("close > ma10").unwrap(),
            FilterExpr::Cmp(_)
        ));
    }

    #[test]
    fn skips_blank_and_comment_lines_and_reports_line_numbers() {
        let lines = vec![
            "# 长期均线向上".to_string(),
            String::new(),
            "close > ma10".to_string(),
        ];
        assert_eq!(parse_filter_lines(&lines).unwrap().len(), 1);

        let bad = vec!["close > ma10".to_string(), "bogus > ma10".to_string()];
        let err = parse_filter_lines(&bad).unwrap_err();
        assert!(err.starts_with("第 2 行:"), "got: {err}");
    }

    #[test]
    fn or_line_matches_when_either_side_holds() {
        // Rising closes 1..=20: price > ma10 holds, price < ma10 does not.
        let dir = stock_dir_with_closes(
            "chart_app_filter_or",
            &(1..=20).map(|i| i as f64).collect::<Vec<_>>(),
        );
        let ma = vec![10];

        let f = filter_lines(&["price < ma10 or price > ma10"]);
        assert!(evaluate_filters_on_stock(&dir, &f, &ma, &[Period::Daily]));

        let f = filter_lines(&["price < ma10 or ma10 > price"]);
        assert!(!evaluate_filters_on_stock(&dir, &f, &ma, &[Period::Daily]));

        // `and` still requires both sides
        let f = filter_lines(&["price > ma10 and price < ma10"]);
        assert!(!evaluate_filters_on_stock(&dir, &f, &ma, &[Period::Daily]));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn per_line_period_prefix_selects_timeframe() {
        // 40 daily bars rising 1..=40 ⟹ ~6 weekly bars, also rising.
        let dir = stock_dir_with_closes(
            "chart_app_filter_line_period",
            &(1..=40).map(|i| i as f64).collect::<Vec<_>>(),
        );
        let ma = vec![];

        // ma5 exists on daily (40 bars) but not on weekly (6 bars → it does,
        // barely); use ma20 which only resolves on the daily series.
        let f = filter_lines(&["day: price > ma20"]);
        assert!(evaluate_filters_on_stock(&dir, &f, &ma, &[Period::Daily]));

        let f = filter_lines(&["week: price > ma20"]);
        assert!(!evaluate_filters_on_stock(&dir, &f, &ma, &[Period::Daily]));

        // Mixed timeframes in one bundle: both lines must hold, each on its own.
        let f = filter_lines(&["week: price > ma3", "day: price > close[-10]"]);
        assert!(evaluate_filters_on_stock(&dir, &f, &ma, &[Period::Daily]));

        // A prefixed line ignores the bundle's default periods.
        let f = filter_lines(&["week: price > ma20"]);
        assert!(!evaluate_filters_on_stock(
            &dir,
            &f,
            &ma,
            &[Period::Daily, Period::Weekly]
        ));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn active_periods_reflects_line_prefixes() {
        let f = NamedFilter {
            name: "mixed".to_string(),
            filters: vec![
                "week: close > ma100".to_string(),
                "day: close > close[-10]".to_string(),
            ],
            periods: vec![Period::Daily],
        };
        let p = f.active_periods();
        assert!(p.contains(&Period::Daily) && p.contains(&Period::Weekly));

        let f = NamedFilter {
            name: "weekly-only".to_string(),
            filters: vec!["week: close > ma100".to_string()],
            periods: vec![Period::Daily],
        };
        assert_eq!(f.active_periods(), vec![Period::Weekly]);

        let f = NamedFilter {
            name: "unprefixed".to_string(),
            filters: vec!["close > ma100".to_string()],
            periods: vec![Period::Daily],
        };
        assert_eq!(f.active_periods(), vec![Period::Daily]);
    }

    #[test]
    fn parses_dl_summary_fields() {
        // updated line
        assert_eq!(
            parse_dl_summary_field("  updated  : 42 (with 1234 new bars)"),
            Some(("updated", 42))
        );
        // skipped line
        assert_eq!(
            parse_dl_summary_field("  skipped  : 5015 (already fresh today)"),
            Some(("skipped", 5015))
        );
        // failed line (only shown when > 0)
        assert_eq!(
            parse_dl_summary_field("  failed   : 3"),
            Some(("failed", 3))
        );
        // stocks line
        assert_eq!(
            parse_dl_summary_field("  stocks   : 5015"),
            Some(("stocks", 5015))
        );
        // data dir line must be ignored (multi-word key, path value)
        assert_eq!(
            parse_dl_summary_field("  data dir : /Users/mxiaofeng/Downloads/stock-dl-data/"),
            None
        );
        // "Download complete." header line must not match
        assert_eq!(parse_dl_summary_field("Download complete."), None);
    }

    #[test]
    fn parses_legacy_settings_without_removed_fields() {
        // Settings on disk no longer have ma_windows/source/periods/... but
        // must still deserialize cleanly with sensible defaults.
        let legacy = r#"{
          "dl": {
            "data_dir": "/some/dir",
            "binary": "stock-dl"
          },
          "data_filters": ["price > ma60"]
        }"#;
        let parsed: AppSettings = serde_json::from_str(legacy).unwrap();
        assert_eq!(parsed.dl.data_dir, "/some/dir");
        assert_eq!(parsed.dl.binary, "stock-dl");
        assert_eq!(parsed.dl.concurrency, 8);
        assert_eq!(
            parsed.data_filters,
            vec![NamedFilter {
                name: "default".to_string(),
                filters: vec!["price > ma60".to_string()],
                periods: vec![Period::Daily],
            }]
        );
        assert_eq!(parsed.ma, vec![10, 30, 60]);
    }

    #[test]
    fn parses_named_data_filters() {
        let json = r#"{
          "dl": { "data_dir": "/d", "binary": "stock-dl" },
          "data_filters": [
            { "name": "ma-cluster", "filters": ["cluster >= 60"] },
            { "name": "ma60", "filters": ["price >= ma60"] }
          ]
        }"#;
        let parsed: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.data_filters.len(), 2);
        assert_eq!(parsed.data_filters[0].name, "ma-cluster");
        assert_eq!(parsed.data_filters[0].filters, vec!["cluster >= 60".to_string()]);
        assert_eq!(parsed.data_filters[1].name, "ma60");
        assert_eq!(parsed.data_filters[1].filters, vec!["price >= ma60".to_string()]);
    }

    #[test]
    fn parses_inclusive_filter_operators() {
        let ge = parse_data_filter("volume >= 60").unwrap();
        assert!(matches!(ge.operator, FilterOperator::GreaterThanOrEqual));
        assert_eq!(ge.left.column, "volume");
        assert_eq!(ge.right.column, "60");

        let le = parse_data_filter("price <= ma30").unwrap();
        assert!(matches!(le.operator, FilterOperator::LessThanOrEqual));
        assert_eq!(le.left.column, "close");
        assert_eq!(le.right.column, "ma30");
    }

    #[test]
    fn ma_config_is_read_from_top_level() {
        let json = r#"{
          "dl": { "data_dir": "/d", "binary": "stock-dl" },
          "ma": [10, 30, 60, 120, 240]
        }"#;
        let parsed: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.ma, vec![10, 30, 60, 120, 240]);
    }

    #[test]
    fn ma_config_migrates_from_dl_ma_windows() {
        // Older settings stored MA windows under dl.ma_windows. Make sure those
        // surface as the new top-level `ma` so users don't lose their config.
        let json = r#"{
          "dl": {
            "data_dir": "/d",
            "binary": "stock-dl",
            "ma_windows": [5, 20, 60]
          }
        }"#;
        let parsed: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.ma, vec![5, 20, 60]);
    }

    #[test]
    fn ma_config_migrates_from_top_level_ma_windows() {
        let json = r#"{
          "dl": { "data_dir": "/d", "binary": "stock-dl" },
          "ma_windows": [7, 14, 28]
        }"#;
        let parsed: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.ma, vec![7, 14, 28]);
    }

    #[test]
    fn empty_ma_config_falls_back_to_default() {
        let json = r#"{ "ma": [] }"#;
        let parsed: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.ma, vec![10, 30, 60]);
    }

    #[test]
    fn parses_legacy_settings_with_dropped_fields_ignored() {
        // Older settings files may still have source/periods/include_st. These
        // must not cause a parse failure.
        let legacy = r#"{
          "dl": {
            "data_dir": "/some/dir",
            "binary": "stock-dl",
            "source": "akshare",
            "include_st": false,
            "periods": ["daily"]
          }
        }"#;
        let parsed: AppSettings = serde_json::from_str(legacy).unwrap();
        assert_eq!(parsed.dl.concurrency, 8);
    }

    #[test]
    fn weekly_aggregation_groups_monday_to_sunday() {
        // Days Mon 2025-12-29 through Mon 2026-01-05 span two ISO weeks.
        let daily = vec![
            Candle {
                timestamp: "2025-12-29".into(),
                open: 10.0,
                close: 11.0,
                high: 11.5,
                low: 9.5,
                volume: 100.0,
                amount: Some(1_050.0),
            },
            Candle {
                timestamp: "2025-12-30".into(),
                open: 11.0,
                close: 12.0,
                high: 12.5,
                low: 10.5,
                volume: 200.0,
                amount: Some(2_300.0),
            },
            Candle {
                timestamp: "2026-01-02".into(),
                open: 12.0,
                close: 13.0,
                high: 13.5,
                low: 11.5,
                volume: 300.0,
                amount: Some(3_750.0),
            },
            // Next week (Mon)
            Candle {
                timestamp: "2026-01-05".into(),
                open: 13.0,
                close: 14.0,
                high: 14.5,
                low: 12.5,
                volume: 400.0,
                amount: Some(5_420.0),
            },
            Candle {
                timestamp: "2026-01-06".into(),
                open: 14.0,
                close: 13.5,
                high: 14.8,
                low: 13.0,
                volume: 500.0,
                amount: Some(6_900.0),
            },
        ];
        let weekly = aggregate_weekly(&daily);
        assert_eq!(weekly.len(), 2);

        // Week 1: opens at first Monday, closes Friday of that week
        assert_eq!(weekly[0].timestamp, "2026-01-02");
        assert!((weekly[0].open - 10.0).abs() < 1e-9);
        assert!((weekly[0].close - 13.0).abs() < 1e-9);
        assert!((weekly[0].high - 13.5).abs() < 1e-9);
        assert!((weekly[0].low - 9.5).abs() < 1e-9);
        assert!((weekly[0].volume - 600.0).abs() < 1e-9);
        assert_eq!(weekly[0].amount, Some(7_100.0));

        // Week 2
        assert_eq!(weekly[1].timestamp, "2026-01-06");
        assert!((weekly[1].open - 13.0).abs() < 1e-9);
        assert!((weekly[1].close - 13.5).abs() < 1e-9);
        assert!((weekly[1].volume - 900.0).abs() < 1e-9);
        assert_eq!(weekly[1].amount, Some(12_320.0));
    }

    #[test]
    fn week_key_keeps_same_week_for_mon_through_fri() {
        let mon = week_key(2026, 1, 5);
        let fri = week_key(2026, 1, 9);
        assert_eq!(mon, fri);
        let next_mon = week_key(2026, 1, 12);
        assert_eq!(next_mon, mon + 1);
    }

    #[test]
    fn ma_is_computed_from_closes() {
        let closes: Vec<f64> = (1..=15).map(|i| i as f64).collect();
        let ma = calculate_ma(&closes, 5);
        assert!(ma[0].is_nan());
        assert!(ma[3].is_nan());
        // MA5 at index 4 = mean(1..=5) = 3.0
        assert!((ma[4] - 3.0).abs() < 1e-9);
        // MA5 at index 14 = mean(11..=15) = 13.0
        assert!((ma[14] - 13.0).abs() < 1e-9);
    }

    #[test]
    fn filter_resolves_on_demand_ma() {
        let dir = std::env::temp_dir().join("chart_app_filter_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // 12 rows of monotonically rising closes so MA10 exists from row 9.
        let mut csv = String::from("date,open,close,high,low,volume,amount\n");
        for i in 1..=12 {
            csv.push_str(&format!(
                "2026-01-{:02},{v}.0,{v}.0,{v}.0,{v}.0,100,\n",
                i,
                v = i
            ));
        }
        std::fs::write(dir.join("daily.csv"), csv).unwrap();

        let ma = vec![10, 30, 60];

        // price > ma10 should hold on a rising series
        let filters = filter_lines(&["price > ma10"]);
        assert!(evaluate_filters_on_stock(&dir, &filters, &ma, &[Period::Daily]));

        // price < ma10 should not hold
        let filters = filter_lines(&["price < ma10"]);
        assert!(!evaluate_filters_on_stock(&dir, &filters, &ma, &[Period::Daily]));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn filter_resolves_ma_outside_configured_windows() {
        // Even if the user only configures [10, 30, 60], a filter that
        // references ma5 should still work because the evaluator unions the
        // filter's MA references into the computed windows.
        let dir = std::env::temp_dir().join("chart_app_filter_ma_outside");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut csv = String::from("date,open,close,high,low,volume,amount\n");
        for i in 1..=12 {
            csv.push_str(&format!(
                "2026-01-{:02},{v}.0,{v}.0,{v}.0,{v}.0,100,\n",
                i,
                v = i
            ));
        }
        std::fs::write(dir.join("daily.csv"), csv).unwrap();

        let configured = vec![10, 30, 60];
        let filters = filter_lines(&["price > ma5"]);
        assert!(evaluate_filters_on_stock(&dir, &filters, &configured, &[Period::Daily]));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn filter_resolves_inclusive_ma_bullish_chain() {
        let dir = std::env::temp_dir().join("chart_app_filter_ma_bullish");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut csv = String::from("date,open,close,high,low,volume,amount\n");
        for i in 1..=140 {
            csv.push_str(&format!(
                "2026-01-{:02},{v}.0,{v}.0,{v}.0,{v}.0,100,\n",
                (i % 28) + 1,
                v = i
            ));
        }
        std::fs::write(dir.join("daily.csv"), csv).unwrap();

        let configured = vec![10, 30, 60];
        let filters = filter_lines(&["price >= ma30", "ma30 >= ma60", "ma60 >= ma120"]);
        assert!(evaluate_filters_on_stock(&dir, &filters, &configured, &[Period::Daily]));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_candles_returns_configured_ma_lines() {
        let dir = std::env::temp_dir().join("chart_app_load_candles_ma");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut csv = String::from("date,open,close,high,low,volume,amount\n");
        for i in 1..=20 {
            csv.push_str(&format!(
                "2026-01-{:02},{v}.0,{v}.0,{v}.0,{v}.0,100,\n",
                i,
                v = i
            ));
        }
        std::fs::write(dir.join("daily.csv"), csv).unwrap();

        let (_candles, ma_lines) = load_candles(&dir, Period::Daily, &[5, 10, 20]);
        let periods: Vec<usize> = ma_lines.iter().map(|m| m.period).collect();
        assert_eq!(periods, vec![5, 10, 20]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_division_operand() {
        let f = parse_data_filter("price/price[200] <= 2.0").unwrap();
        assert!(matches!(f.operator, FilterOperator::LessThanOrEqual));
        // numerator: latest close
        assert_eq!(f.left.column, "close");
        assert_eq!(f.left.offset, 0);
        // divisor: close 200 bars ago
        let div = f.left.divisor.as_ref().unwrap();
        assert_eq!(div.column, "close");
        assert_eq!(div.offset, 200);
        // right side: literal 2.0
        assert_eq!(f.right.column, "2.0");
        assert!(f.right.divisor.is_none());
    }

    #[test]
    fn filter_division_ratio_passes_and_fails() {
        let dir = std::env::temp_dir().join("chart_app_filter_div_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // 250 rows: closes rise from 1 to 250 (latest = 250, row 200 bars ago = 50)
        // ratio = 250/50 = 5.0
        let mut csv = String::from("date,open,close,high,low,volume,amount\n");
        for i in 1u32..=250 {
            let date = format!("2025-{:02}-{:02}", (i / 31) + 1, (i % 28) + 1);
            csv.push_str(&format!(
                "{date},{v}.0,{v}.0,{v}.0,{v}.0,100,\n",
                v = i
            ));
        }
        std::fs::write(dir.join("daily.csv"), csv).unwrap();

        let ma = vec![];

        // ratio 5.0 > 2.0 → passes
        let filters = filter_lines(&["price/price[200] > 2.0"]);
        assert!(evaluate_filters_on_stock(&dir, &filters, &ma, &[Period::Daily]));

        // ratio 5.0 <= 2.0 → fails
        let filters = filter_lines(&["price/price[200] <= 2.0"]);
        assert!(!evaluate_filters_on_stock(&dir, &filters, &ma, &[Period::Daily]));

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Arc structures ──

    /// Hand-drawn examples of the shape the `arc_*` detector is for, traced as
    /// polylines in picture pixels (x → time, y grows downward). Each starts
    /// flat, rallies, then swings sideways with bottoms climbing along an arc.
    fn arc_sketches() -> [Vec<(f64, f64)>; 3] {
        let pts = |v: &[(i32, i32)]| v.iter().map(|&(x, y)| (x as f64, y as f64)).collect();
        [
            pts(&[
                (25, 735), (45, 735), (130, 545), (150, 455), (205, 605), (243, 425),
                (320, 555), (360, 390), (440, 537), (485, 368), (595, 495), (640, 355),
                (720, 418), (780, 418), (828, 282),
            ]),
            pts(&[
                (45, 1220), (60, 1220), (210, 920), (240, 1107), (345, 905), (375, 1087),
                (490, 860), (570, 1100), (612, 860), (670, 1062), (700, 858), (765, 970),
                (790, 825), (840, 930), (890, 770), (905, 835), (907, 660),
            ]),
            pts(&[
                (30, 1650), (60, 1650), (200, 1362), (300, 1535), (327, 1535), (365, 1325),
                (520, 1548), (587, 1320), (680, 1520), (725, 1300), (770, 1430), (812, 1255),
                (840, 1355), (885, 1165), (905, 1245),
            ]),
        ]
    }

    /// Daily candles following a polyline: 4 px per bar, 0.1% of price per px,
    /// each bar opening at the previous close with a small shadow either side.
    fn candles_from_sketch(points: &[(f64, f64)]) -> Vec<Candle> {
        let first_bar = (points[0].0 / 4.0) as usize;
        let last_bar = (points[points.len() - 1].0 / 4.0) as usize;
        let mut candles: Vec<Candle> = Vec::new();
        for bar in first_bar..=last_bar {
            let x = bar as f64 * 4.0;
            let y = match points.windows(2).find(|w| w[0].0 <= x && x <= w[1].0) {
                Some(w) => w[0].1 + (w[1].1 - w[0].1) * (x - w[0].0) / (w[1].0 - w[0].0),
                None => points[0].1,
            };
            let close = 10.0 * (-y * 0.001).exp();
            let open = candles.last().map_or(close, |c| c.close);
            // Sequential calendar days from 2025-01-01, as in `stock_dir_with_closes`.
            let day = candles.len() as u32;
            candles.push(Candle {
                timestamp: format!("2025-{:02}-{:02}", day / 28 + 1, day % 28 + 1),
                open,
                close,
                high: open.max(close) * 1.004,
                low: open.min(close) * 0.996,
                volume: 100.0,
                amount: None,
            });
        }
        candles
    }

    /// A sketch from scratch: a flat start at `base`, then the given turning
    /// points (pixel heights, alternating top / bottom) 60 px apart.
    fn swing_sketch(base: f64, turns: &[f64]) -> Vec<(f64, f64)> {
        let mut pts = vec![(0.0, base), (20.0, base)];
        pts.extend(turns.iter().enumerate().map(|(i, &y)| (80.0 + 60.0 * i as f64, y)));
        pts
    }

    /// A real instance of the shape, picked by eye: 东航物流 (sh601156) daily,
    /// forward-adjusted, as (date, open, high, low, close). A tight base, a
    /// +40% rally from 2023-11-28 to 2023-12-13, then months of swings whose
    /// lows rise along an arc — apart from one long lower shadow on 2024-02-05.
    const ARC_EXAMPLE: &[(&str, f64, f64, f64, f64)] = &[
        ("2023-10-09", 11.56, 11.57, 11.47, 11.49), ("2023-10-10", 11.5, 11.55, 11.33, 11.33),
        ("2023-10-11", 11.34, 11.4, 11.29, 11.32), ("2023-10-12", 11.4, 11.44, 11.35, 11.41),
        ("2023-10-13", 11.37, 11.41, 11.27, 11.3), ("2023-10-16", 11.3, 11.34, 10.97, 11.03),
        ("2023-10-17", 11.02, 11.04, 10.98, 11.0), ("2023-10-18", 10.99, 11.01, 10.7, 10.74),
        ("2023-10-19", 10.7, 10.8, 10.66, 10.68), ("2023-10-20", 10.64, 10.73, 10.62, 10.65),
        ("2023-10-23", 10.62, 10.63, 10.35, 10.4), ("2023-10-24", 10.46, 10.57, 10.39, 10.55),
        ("2023-10-25", 10.6, 10.69, 10.6, 10.61), ("2023-10-26", 10.59, 10.63, 10.53, 10.62),
        ("2023-10-27", 10.61, 10.91, 10.57, 10.84), ("2023-10-30", 10.81, 10.93, 10.79, 10.9),
        ("2023-10-31", 10.85, 10.96, 10.81, 10.88), ("2023-11-01", 10.95, 10.95, 10.79, 10.84),
        ("2023-11-02", 10.84, 10.89, 10.8, 10.84), ("2023-11-03", 10.88, 10.92, 10.81, 10.89),
        ("2023-11-06", 10.93, 11.32, 10.93, 11.26), ("2023-11-07", 11.22, 11.35, 11.17, 11.23),
        ("2023-11-08", 11.23, 11.25, 11.11, 11.16), ("2023-11-09", 11.12, 11.24, 11.12, 11.17),
        ("2023-11-10", 11.16, 11.25, 11.1, 11.21), ("2023-11-13", 11.21, 11.32, 11.14, 11.32),
        ("2023-11-14", 11.33, 11.34, 11.22, 11.29), ("2023-11-15", 11.37, 11.41, 11.26, 11.3),
        ("2023-11-16", 11.32, 11.34, 11.18, 11.25), ("2023-11-17", 11.22, 11.38, 11.2, 11.34),
        ("2023-11-20", 11.35, 11.42, 11.23, 11.39), ("2023-11-21", 11.4, 11.43, 11.34, 11.36),
        ("2023-11-22", 11.34, 11.39, 11.24, 11.24), ("2023-11-23", 11.25, 11.31, 11.21, 11.28),
        ("2023-11-24", 11.29, 11.29, 11.2, 11.26), ("2023-11-27", 11.25, 11.27, 11.16, 11.22),
        ("2023-11-28", 11.2, 11.28, 11.12, 11.26), ("2023-11-29", 11.26, 11.61, 11.25, 11.5),
        ("2023-11-30", 11.59, 12.72, 11.51, 12.72), ("2023-12-01", 12.89, 13.82, 12.82, 13.55),
        ("2023-12-04", 13.79, 14.26, 13.4, 13.4), ("2023-12-05", 13.4, 13.59, 13.12, 13.17),
        ("2023-12-06", 13.31, 14.03, 13.01, 13.75), ("2023-12-07", 13.75, 14.27, 13.75, 13.93),
        ("2023-12-08", 13.77, 14.02, 13.52, 13.58), ("2023-12-11", 13.56, 14.6, 13.44, 14.56),
        ("2023-12-12", 14.6, 15.01, 14.42, 14.8), ("2023-12-13", 14.8, 15.56, 14.75, 15.24),
        ("2023-12-14", 15.18, 15.55, 14.97, 15.07), ("2023-12-15", 14.88, 15.13, 14.67, 14.83),
        ("2023-12-18", 14.97, 15.16, 14.67, 14.7), ("2023-12-19", 14.68, 14.74, 13.93, 14.31),
        ("2023-12-20", 14.47, 14.71, 14.12, 14.2), ("2023-12-21", 14.14, 14.57, 14.02, 14.31),
        ("2023-12-22", 14.44, 14.44, 14.04, 14.21), ("2023-12-25", 14.12, 14.27, 13.81, 13.9),
        ("2023-12-26", 13.9, 14.09, 13.52, 13.7), ("2023-12-27", 13.77, 14.34, 13.77, 14.32),
        ("2023-12-28", 14.27, 14.47, 14.05, 14.09), ("2023-12-29", 14.13, 14.3, 14.01, 14.1),
        ("2024-01-02", 14.09, 14.41, 13.99, 14.32), ("2024-01-03", 14.18, 14.24, 13.78, 13.92),
        ("2024-01-04", 13.87, 14.17, 13.81, 14.1), ("2024-01-05", 14.06, 14.13, 13.66, 13.78),
        ("2024-01-08", 13.71, 13.92, 13.57, 13.62), ("2024-01-09", 13.6, 13.99, 13.52, 13.82),
        ("2024-01-10", 13.81, 13.9, 13.65, 13.66), ("2024-01-11", 13.68, 14.07, 13.65, 14.01),
        ("2024-01-12", 14.01, 14.26, 13.86, 13.89), ("2024-01-15", 13.96, 14.57, 13.78, 14.23),
        ("2024-01-16", 14.24, 15.31, 14.07, 15.21), ("2024-01-17", 15.22, 15.87, 15.02, 15.37),
        ("2024-01-18", 15.36, 15.39, 14.62, 15.13), ("2024-01-19", 15.12, 15.27, 14.8, 15.16),
        ("2024-01-22", 15.34, 15.77, 14.62, 14.78), ("2024-01-23", 14.55, 16.07, 14.53, 16.01),
        ("2024-01-24", 16.17, 16.7, 15.82, 16.6), ("2024-01-25", 16.35, 16.99, 16.16, 16.94),
        ("2024-01-26", 16.82, 16.85, 16.45, 16.69), ("2024-01-29", 16.32, 16.61, 14.99, 15.17),
        ("2024-01-30", 15.09, 15.57, 14.98, 15.02), ("2024-01-31", 14.87, 15.24, 14.32, 14.47),
        ("2024-02-01", 14.39, 14.92, 14.18, 14.65), ("2024-02-02", 14.73, 14.92, 13.65, 14.03),
        ("2024-02-05", 13.59, 13.99, 12.87, 13.4), ("2024-02-06", 13.12, 14.76, 12.92, 14.7),
        ("2024-02-07", 14.85, 15.62, 14.57, 15.6), ("2024-02-08", 15.51, 16.28, 15.14, 15.2),
        ("2024-02-19", 15.63, 16.12, 15.19, 15.81), ("2024-02-20", 15.62, 16.45, 15.49, 16.33),
        ("2024-02-21", 16.12, 16.77, 16.0, 16.26), ("2024-02-22", 16.15, 16.37, 16.0, 16.22),
        ("2024-02-23", 16.12, 16.31, 15.92, 15.92), ("2024-02-26", 15.65, 15.9, 15.06, 15.16),
        ("2024-02-27", 15.17, 15.21, 14.77, 14.99), ("2024-02-28", 14.93, 15.23, 14.72, 14.72),
        ("2024-02-29", 14.8, 15.04, 14.63, 14.84), ("2024-03-01", 14.83, 14.86, 14.23, 14.72),
        ("2024-03-04", 14.67, 15.71, 14.51, 15.44), ("2024-03-05", 15.32, 15.37, 14.92, 15.13),
        ("2024-03-06", 15.04, 15.43, 15.0, 15.19), ("2024-03-07", 15.18, 15.66, 15.07, 15.22),
        ("2024-03-08", 15.35, 15.62, 15.02, 15.57), ("2024-03-11", 15.32, 15.47, 14.71, 14.84),
        ("2024-03-12", 14.85, 15.13, 14.77, 14.96), ("2024-03-13", 15.01, 15.36, 14.89, 15.28),
        ("2024-03-14", 15.28, 15.89, 15.18, 15.43), ("2024-03-15", 15.25, 15.4, 15.12, 15.29),
        ("2024-03-18", 15.3, 16.11, 15.25, 16.02), ("2024-03-19", 15.9, 15.98, 15.39, 15.51),
        ("2024-03-20", 15.5, 16.57, 15.43, 16.52), ("2024-03-21", 16.66, 16.68, 15.9, 16.04),
        ("2024-03-22", 15.94, 15.98, 15.11, 15.2), ("2024-03-25", 15.2, 15.61, 15.02, 15.28),
        ("2024-03-26", 15.21, 15.37, 14.72, 14.88), ("2024-03-27", 15.22, 16.44, 15.22, 16.03),
        ("2024-03-28", 16.25, 16.37, 15.73, 15.84), ("2024-03-29", 15.83, 16.89, 15.78, 16.51),
        ("2024-04-01", 16.54, 17.68, 16.28, 17.61), ("2024-04-02", 17.42, 17.79, 17.33, 17.58),
        ("2024-04-03", 17.62, 18.0, 17.12, 17.29), ("2024-04-08", 16.82, 16.82, 15.99, 16.42),
        ("2024-04-09", 16.42, 16.8, 16.32, 16.42), ("2024-04-10", 16.34, 16.73, 16.34, 16.64),
        ("2024-04-11", 16.62, 16.74, 16.3, 16.4), ("2024-04-12", 16.42, 16.81, 16.42, 16.59),
        ("2024-04-15", 16.55, 17.4, 16.37, 17.0), ("2024-04-16", 17.18, 17.21, 16.32, 16.34),
        ("2024-04-17", 16.44, 16.84, 16.22, 16.82), ("2024-04-18", 16.74, 17.06, 16.23, 16.3),
        ("2024-04-19", 16.24, 16.31, 15.9, 16.1), ("2024-04-22", 16.06, 16.52, 15.98, 16.21),
        ("2024-04-23", 16.02, 16.22, 15.72, 16.11), ("2024-04-24", 16.94, 17.58, 16.47, 17.5),
        ("2024-04-25", 17.5, 17.85, 17.43, 17.82), ("2024-04-26", 17.82, 18.15, 17.66, 18.01),
        ("2024-04-29", 17.94, 18.2, 17.47, 17.81), ("2024-04-30", 16.67, 17.82, 16.67, 17.62),
    ];

    /// `ARC_EXAMPLE` as candles, up to and including `as_of`.
    fn arc_example(as_of: &str) -> Vec<Candle> {
        ARC_EXAMPLE
            .iter()
            .filter(|row| row.0 <= as_of)
            .map(|&(date, open, high, low, close)| Candle {
                timestamp: date.to_string(),
                open,
                close,
                high,
                low,
                volume: 100.0,
                amount: None,
            })
            .collect()
    }

    /// Writes full OHLC candles as `daily.csv` into a fresh temp dir.
    fn stock_dir_with_candles(name: &str, candles: &[Candle]) -> PathBuf {
        let dir = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut csv = String::from("date,open,close,high,low,volume,amount\n");
        for c in candles {
            csv.push_str(&format!(
                "{},{},{},{},{},{},\n",
                c.timestamp, c.open, c.close, c.high, c.low, c.volume
            ));
        }
        std::fs::write(dir.join("daily.csv"), csv).unwrap();
        dir
    }

    #[test]
    fn arc_found_on_each_sketch_at_every_stage() {
        // The shape should be recognised while it is still forming, not only
        // once the last swing is in: also try with trailing vertices removed.
        for (i, sketch) in arc_sketches().iter().enumerate() {
            // In the first sketch the bottoms climb in a straight line until
            // the last two swings; only then is there an accelerating arc.
            let stages = if i == 0 { 0..3 } else { 0..4 };
            for cut in stages {
                let candles = candles_from_sketch(&sketch[..sketch.len() - cut]);
                assert!(
                    !find_arc_structures(&candles).is_empty(),
                    "sketch {} minus {} vertices",
                    i + 1,
                    cut
                );
            }
        }
    }

    #[test]
    fn arc_metrics_describe_the_sketch() {
        // Second sketch: a ~36% rally over the first 41 bars, then 174 bars of
        // ~19% swings whose centre and bottoms both climb about a fifth, the
        // bottoms along a clearly bent arc.
        let arcs = find_arc_structures(&candles_from_sketch(&arc_sketches()[1]));
        assert_eq!(arcs.len(), 1, "{arcs:?}");
        let a = &arcs[0];
        assert_eq!((a.shape.start, a.shape.top, a.shape.end), (0, 41, 215));
        assert_eq!(a.len, 215);
        assert!((a.run - 36.0).abs() < 1.5, "{a:?}");
        assert!((a.amp - 19.0).abs() < 1.5, "{a:?}");
        assert!((a.rise - 19.0).abs() < 1.5, "{a:?}");
        assert!((a.lift - 21.0).abs() < 1.5, "{a:?}");
        assert!(a.bend > 20.0, "{a:?}");
        assert!(a.breakout < 0.0 && a.breakout > -10.0, "{a:?}");
        // The later swings topped out well above the rally's own top: from
        // the starting low the whole move has carried ~57%.
        assert!((a.peak - 57.0).abs() < 2.0, "{a:?}");
    }

    #[test]
    fn arc_recognises_the_real_example() {
        for as_of in ["2024-03-15", "2024-03-29", "2024-04-15", "2024-04-30"] {
            let candles = arc_example(as_of);
            let arcs = find_arc_structures(&candles);
            // Longest first: the structure that starts with the big rally.
            let a = arcs.first().unwrap_or_else(|| panic!("nothing as of {as_of}"));
            assert_eq!(candles[a.shape.start].timestamp, "2023-11-28", "as of {as_of}");
            assert_eq!(candles[a.shape.top].timestamp, "2023-12-13", "as of {as_of}");
            assert_eq!(a.shape.end, candles.len() - 1);
            assert!((a.run - 40.0).abs() < 1.0, "{a:?}");
        }
        // Three weeks after the 2024-02-05 selloff the lows had not lifted
        // yet: at that point there was no arc to speak of.
        assert!(find_arc_structures(&arc_example("2024-02-29")).is_empty());
    }

    #[test]
    fn arc_rejects_shapes_that_only_resemble_it() {
        let found = |turns: &[f64]| {
            find_arc_structures(&candles_from_sketch(&swing_sketch(800.0, turns))).len()
        };
        // Lower pixel = higher price. Bottoms 650 → 640 → 610 → 560 under
        // nearly level tops: the real thing.
        assert_eq!(found(&[500.0, 650.0, 490.0, 640.0, 480.0, 610.0, 470.0, 560.0, 460.0]), 1);
        // Bottoms rising in a straight line (650 → 620 → 590 → 560): higher
        // every time, but not accelerating.
        assert_eq!(found(&[500.0, 650.0, 490.0, 620.0, 480.0, 590.0, 470.0, 560.0, 460.0]), 0);
        // The same swings with the bottoms stepping down instead.
        assert_eq!(found(&[500.0, 560.0, 490.0, 610.0, 480.0, 640.0, 470.0, 650.0, 460.0]), 0);
        // Rising at first, then rolling over.
        assert_eq!(found(&[500.0, 650.0, 440.0, 590.0, 430.0, 600.0, 470.0, 650.0, 520.0]), 0);
        // A steep channel: every leg up is as big as the "rally" — a trend,
        // not a rally being digested.
        assert_eq!(found(&[500.0, 650.0, 390.0, 540.0, 280.0, 430.0, 170.0, 320.0, 60.0]), 0);
        // A round trip: the rally is given back and price swings near its base.
        assert_eq!(found(&[500.0, 780.0, 620.0, 770.0, 600.0, 750.0, 580.0, 720.0, 560.0]), 0);
    }

    #[test]
    fn arc_gone_once_price_breaks_down() {
        let mut sketch = arc_sketches()[1].clone();
        sketch.truncate(sketch.len() - 1);
        assert!(!find_arc_structures(&candles_from_sketch(&sketch)).is_empty());
        // …then a slide back to where the swings began.
        sketch.push((980.0, 1095.0));
        let arcs = find_arc_structures(&candles_from_sketch(&sketch));
        assert!(arcs.is_empty(), "{arcs:?}");
    }

    #[test]
    fn arc_shape_locates_the_strokes_on_the_chart() {
        let candles = arc_example("2024-03-29");
        let lines = filter_lines(&["arc_len >= 40"]);
        let arc = arc_to_draw(&lines, &[Period::Daily], Period::Daily, &candles, &[])
            .expect("structure to draw");
        let shape = &arc.shape;
        let (first, last) = (shape.top + 1, shape.end);

        // The centre line rises, and runs through the middle of the closes.
        assert!(shape.centre_price(last as f64) > shape.centre_price(first as f64));
        let above = (first..=last)
            .filter(|&i| candles[i].close > shape.centre_price(i as f64))
            .count();
        let share = above as f64 / (last - first + 1) as f64;
        assert!((0.3..=0.7).contains(&share), "{share}");

        // The arc rises too and sits under the centre line. It hugs the lows
        // from below: only the bottoms of the swings dip under it, a few bars
        // in all — deepest of them the 2024-02-05 shadow, which it ignores
        // rather than follows.
        assert!(shape.arc_price(last as f64) > shape.arc_price(first as f64));
        assert!(shape.arc_price(last as f64) < shape.centre_price(last as f64));
        let below: Vec<&str> = (first..=last)
            .filter(|&i| candles[i].low < shape.arc_price(i as f64) * 0.99)
            .map(|i| candles[i].timestamp.as_str())
            .collect();
        assert!(below.contains(&"2024-02-05"), "{below:?}");
        assert!(below.len() * 10 <= last - first + 1, "{below:?}");
        let shadow = candles.iter().position(|c| c.timestamp == "2024-02-05").unwrap();
        assert!(candles[shadow].low < shape.arc_price(shadow as f64) * 0.95);

        // Nothing to draw where the bundle has no arc line for the timeframe,
        // where no structure meets it, or for a bundle without `arc_*`.
        let draw = |line: &str| {
            arc_to_draw(&filter_lines(&[line]), &[Period::Daily], Period::Daily, &candles, &[])
        };
        assert!(draw("week: arc_len >= 40").is_none());
        assert!(draw("arc_len >= 400").is_none());
        assert!(draw("close > 0").is_none());
    }

    #[test]
    fn arc_columns_parse_and_take_no_offset() {
        assert!(parse_filter_line("arc_len >= 40 and arc_peak <= 100").is_ok());
        assert!(parse_filter_line("week: arc_break >= -10").is_ok());
        assert!(parse_filter_line("arc_len[-1] >= 40").is_err());
        assert!(parse_filter_line("arc_foo >= 4").is_err());
    }

    #[test]
    fn arc_filter_binds_one_structure_per_line() {
        // By mid-April the example holds two structures: the big one (90 bars
        // since a +40% rally) and a later one that takes the February rebound
        // as its rally (42 bars since a +30% one).
        let dir = stock_dir_with_candles("chart_app_arc_filter", &arc_example("2024-04-15"));
        let ma = vec![];
        let holds = |lines: &[&str], periods: &[Period]| {
            evaluate_filters_on_stock(&dir, &filter_lines(lines), &ma, periods)
        };
        let daily = &[Period::Daily];

        assert!(holds(&["arc_len >= 80"], daily));
        assert!(holds(&["arc_run <= 35"], daily));
        assert!(!holds(&["arc_len >= 200"], daily));
        // On one line both conditions must be met by the same structure…
        assert!(!holds(&["arc_len >= 80 and arc_run <= 35"], daily));
        // …while separate lines may each be met by a different one.
        assert!(holds(&["arc_len >= 80", "arc_run <= 35"], daily));

        // Each timeframe has its own structures: 130 daily bars are fewer
        // than 30 weekly ones, so nothing on the weekly chart is 80 bars long.
        assert!(holds(&["day: arc_len >= 80"], daily));
        assert!(!holds(&["week: arc_len >= 80"], daily));
        // An unprefixed line passes if any of the bundle's periods has one.
        assert!(holds(&["arc_len >= 80"], &[Period::Weekly, Period::Daily]));
        assert!(!holds(&["arc_len >= 80"], &[Period::Weekly]));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn arc_columns_are_unresolved_without_a_structure() {
        // A steady climb has no rally-then-pause in it, so there is nothing
        // for `arc_*` to describe: comparisons are false either way round.
        let closes: Vec<f64> = (1..=120).map(|i| 10.0 + i as f64 * 0.1).collect();
        let dir = stock_dir_with_closes("chart_app_arc_none", &closes);
        let ma = vec![];
        let holds = |line: &str| {
            evaluate_filters_on_stock(&dir, &filter_lines(&[line]), &ma, &[Period::Daily])
        };

        assert!(!holds("arc_len >= 1"));
        assert!(!holds("arc_len < 1"));
        // …which still lets an `or` branch carry the line.
        assert!(holds("arc_len >= 1 or close > ma20"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
