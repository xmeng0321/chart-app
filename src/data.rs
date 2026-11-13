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
// The shape: a leg up — anything from a violent rally to a single hammer
// candle — then a sideways fight in which every pullback bottoms a little
// higher than the last, the bottoms tracing a rising, convex ("exponential")
// arc while the tops stay in a band. Buyers step in earlier on each dip.
//
// None of it has a fixed period, so instead of a look-back window the detector
// works from swing pivots:
//
//   1. Reduce closes to a zigzag that ignores moves smaller than a threshold,
//      then move each pivot to the true high/low of its swing. Every threshold
//      in `ARC_SCALES` is tried, so small and large structures are both found.
//   2. Take each swing low as a possible start. The leg up from it is the
//      rally; the swing lows after it are the bottoms, and `arc_from` decides
//      whether they sit on such an arc right up to the latest bar.
//
// Each surviving stretch is one `ArcStructure`; a stock usually has zero to a
// handful. Filters read them through the `arc_*` columns.
//
// The tolerances below are fractions of the structure's own typical pullback
// (`amp`), which makes them scale-free. They were calibrated on hand-drawn
// examples of the shape (see `arc_sketches` in the tests) with some margin.

/// Zigzag reversal thresholds (fraction of price) the detector tries.
const ARC_SCALES: [f64; 10] = [0.04, 0.05, 0.063, 0.08, 0.10, 0.125, 0.16, 0.20, 0.25, 0.32];
/// Only the most recent bars are searched.
const ARC_MAX_BARS: usize = 500;
/// Bottoms after the rally: fewer isn't an arc yet, more means the zigzag is
/// too fine for this structure (a coarser scale will describe it better).
const ARC_MIN_LOWS: usize = 3;
const ARC_MAX_LOWS: usize = 8;
/// The sideways part must last at least this many bars.
const ARC_MIN_BARS: usize = 20;
/// A bottom that undercuts the previous one by less than this still counts as
/// holding (double bottoms aren't breaks).
const ARC_SLACK: f64 = 0.2;
/// The last bottom sits at least this far above the first.
const ARC_MIN_LIFT: f64 = 0.4;
/// Curvature floor: 0 is a straight line, negative is a rise that is running
/// out of steam. Slightly negative still passes — a hand-drawn arc isn't exact.
const ARC_MIN_BEND: f64 = -0.1;
/// How far the fitted arc may sink below its own first bottom (a "U" is a
/// decline followed by a recovery, not an arc).
const ARC_MAX_SINK: f64 = 0.35;
/// RMS distance of the bottoms from the fitted arc.
const ARC_MAX_RMS: f64 = 0.2;
/// With five or more bottoms one may be ignored when fitting ("偶尔破例"), as
/// long as it only dips below the arc, and by no more than this.
const ARC_MAX_UNDERCUT: f64 = 0.5;
/// Pullbacks give back at least this share of the preceding advance —
/// otherwise it is a trend with shallow dips, not a fight.
const ARC_MIN_RETRACE: f64 = 0.5;
/// Tops stay in a band this wide ("横盘")…
const ARC_MAX_TOP_SPAN: f64 = 1.2;
/// …that neither falls by more than this…
const ARC_MAX_TOP_DROP: f64 = 0.3;
/// …nor rises faster than the bottoms by more than this (a widening range).
const ARC_MAX_TOP_LEAD: f64 = 0.25;
/// The arc must still be in force: the last bottom is at most this many
/// typical bottom-to-bottom gaps old…
const ARC_MAX_STALE: f64 = 2.0;
/// …and price hasn't fallen further than this below where the bottoms'
/// average rate of climb puts them today.
const ARC_FLOOR_SLACK: f64 = 0.25;
/// The rally is the prelude, not the bulk: at most this share of the bars.
const ARC_MAX_RALLY_SHARE: f64 = 0.4;
/// Past this far above the band's highest top the breakout has already run.
const ARC_MAX_BREAK: f64 = 1.0;

pub const ARC_COLUMNS: [&str; 7] = [
    "arc_lows", "arc_len", "arc_run", "arc_amp", "arc_lift", "arc_bend", "arc_break",
];

/// One rally-then-rising-bottoms structure that is still in force on the
/// latest bar. Percentages are plain price changes (10.0 = +10%).
#[derive(Debug, Clone, PartialEq)]
pub struct ArcStructure {
    /// Bottoms (swing lows) made after the rally.
    pub lows: usize,
    /// Bars from the low the rally started at to the latest bar.
    pub len: usize,
    /// The rally: starting low to the first top, in %.
    pub run: f64,
    /// Typical pullback: median top-to-bottom swing, in %.
    pub amp: f64,
    /// How far the bottoms have climbed: last bottom vs first, in %.
    pub lift: f64,
    /// Curvature of the arc through the bottoms: how far its midpoint hangs
    /// below the straight line from first to last bottom, in % of the lift.
    /// 0 = straight, 10–30 = a clear arc, negative = flattening out.
    pub bend: f64,
    /// Latest close vs the highest top of the sideways part, in %. Negative
    /// while still inside the range, positive once it has broken out.
    pub breakout: f64,
}

#[derive(Clone, Copy)]
struct Swing {
    idx: usize,
    peak: bool,
}

/// Every arc structure that ends on the last of `candles`.
pub fn find_arc_structures(candles: &[Candle]) -> Vec<ArcStructure> {
    let recent = &candles[candles.len().saturating_sub(ARC_MAX_BARS)..];
    // Forward-adjusted prices can be zero or negative far in the past; work on
    // the stretch after the last such bar, where logs are defined.
    let start = recent
        .iter()
        .rposition(|c| !(c.low > 0.0 && c.high > 0.0 && c.close > 0.0))
        .map_or(0, |i| i + 1);
    let recent = &recent[start..];
    if recent.len() < ARC_MIN_BARS {
        return Vec::new();
    }
    let logh: Vec<f64> = recent.iter().map(|c| c.high.ln()).collect();
    let logl: Vec<f64> = recent.iter().map(|c| c.low.ln()).collect();
    let logc: Vec<f64> = recent.iter().map(|c| c.close.ln()).collect();

    let mut out = Vec::new();
    for scale in ARC_SCALES {
        let pivots = refine_swings(&zigzag(&logc, scale.ln_1p()), &logh, &logl);
        for s in (0..pivots.len().saturating_sub(1)).rev() {
            if pivots[s].peak {
                continue;
            }
            if let Some(arc) = arc_from(&logh, &logl, &logc, &pivots[s..]) {
                // Neighbouring scales often yield the very same pivots.
                if !out.contains(&arc) {
                    out.push(arc);
                }
            }
        }
    }
    out
}

/// Swing pivots of a log-price series: a pivot is confirmed once price moves
/// `threshold` away from it. Pivots alternate low/high; the final entry is the
/// running extreme of the leg in progress, which may still extend.
fn zigzag(logc: &[f64], threshold: f64) -> Vec<Swing> {
    let mut pivots = Vec::new();
    let (mut hi, mut lo) = (0usize, 0usize);
    let mut ext = 0usize;
    let mut rising: Option<bool> = None;
    for (i, &x) in logc.iter().enumerate().skip(1) {
        match rising {
            None => {
                if x > logc[hi] {
                    hi = i;
                }
                if x < logc[lo] {
                    lo = i;
                }
                if x - logc[lo] >= threshold {
                    pivots.push(Swing { idx: lo, peak: false });
                    rising = Some(true);
                    ext = i;
                } else if logc[hi] - x >= threshold {
                    pivots.push(Swing { idx: hi, peak: true });
                    rising = Some(false);
                    ext = i;
                }
            }
            Some(true) => {
                if x > logc[ext] {
                    ext = i;
                } else if logc[ext] - x >= threshold {
                    pivots.push(Swing { idx: ext, peak: true });
                    rising = Some(false);
                    ext = i;
                }
            }
            Some(false) => {
                if x < logc[ext] {
                    ext = i;
                } else if x - logc[ext] >= threshold {
                    pivots.push(Swing { idx: ext, peak: false });
                    rising = Some(true);
                    ext = i;
                }
            }
        }
    }
    if let Some(up) = rising {
        pivots.push(Swing { idx: ext, peak: up });
    }
    pivots
}

/// Moves each close-based pivot to the bar with the highest high (lowest low)
/// of its swing, so shadows count: a bottom is where price actually traded.
fn refine_swings(pivots: &[Swing], logh: &[f64], logl: &[f64]) -> Vec<Swing> {
    let last_bar = logh.len() - 1;
    pivots
        .iter()
        .enumerate()
        .map(|(k, p)| {
            // The swing reaches from just after the previous pivot to just
            // before the next one.
            let from = match k {
                0 => p.idx.saturating_sub(3),
                _ => (pivots[k - 1].idx + 1).min(p.idx),
            };
            let to = match pivots.get(k + 1) {
                Some(next) => next.idx.saturating_sub(1).max(p.idx),
                None => last_bar,
            };
            let series = if p.peak { logh } else { logl };
            let mut best = from;
            for i in from..=to {
                let better = if p.peak {
                    series[i] > series[best]
                } else {
                    series[i] < series[best]
                };
                if better {
                    best = i;
                }
            }
            Swing { idx: best, peak: p.peak }
        })
        .collect()
}

/// Judges the stretch from `pivots[0]` (a swing low, where the rally starts)
/// to the latest bar. The final pivot is the leg still in progress.
fn arc_from(logh: &[f64], logl: &[f64], logc: &[f64], pivots: &[Swing]) -> Option<ArcStructure> {
    let (running, confirmed) = pivots.split_last()?;
    if confirmed.len() < 2 {
        return None;
    }
    let last_bar = logc.len() - 1;
    let last_close = logc[last_bar];
    let start = confirmed[0].idx;
    let first_top = confirmed[1].idx;

    let lows: Vec<usize> = confirmed[2..].iter().filter(|p| !p.peak).map(|p| p.idx).collect();
    let tops: Vec<usize> = confirmed[1..].iter().filter(|p| p.peak).map(|p| p.idx).collect();
    let n = lows.len();
    if !(ARC_MIN_LOWS..=ARC_MAX_LOWS).contains(&n) || tops.len() < n {
        return None;
    }
    // The rally's starting low is never revisited.
    if logl[start + 1..].iter().any(|&l| l < logl[start]) {
        return None;
    }

    let y: Vec<f64> = lows.iter().map(|&i| logl[i]).collect();
    let top_y: Vec<f64> = tops.iter().map(|&i| logh[i]).collect();

    // Each top with the bottom that follows it.
    let mut pullbacks: Vec<f64> = (0..n).map(|i| top_y[i] - y[i]).collect();
    let mut retraces: Vec<f64> = (0..n)
        .map(|i| {
            let prev_low = if i == 0 { logl[start] } else { y[i - 1] };
            (top_y[i] - y[i]) / (top_y[i] - prev_low).max(1e-9)
        })
        .collect();
    let amp = median(&mut pullbacks)?;
    if amp <= 0.0 || median(&mut retraces)? < ARC_MIN_RETRACE {
        return None;
    }
    let slack = ARC_SLACK * amp;

    // Bottoms are "mostly rising": one exception per three comparisons is
    // forgiven, but never the latest — that would mean it is breaking now. A
    // pullback still in progress counts with the low it has reached so far.
    let mut held = y.clone();
    if !running.peak {
        held.push(logl[running.idx]);
    }
    let breaks = held.windows(2).filter(|w| w[1] < w[0] - slack).count();
    if breaks > (held.len() - 1) / 3 || held[held.len() - 1] < held[held.len() - 2] - slack {
        return None;
    }
    if y.iter().any(|&v| v < y[0] - slack) {
        return None;
    }
    let lift = y[n - 1] - y[0];
    if lift < ARC_MIN_LIFT * amp {
        return None;
    }

    // The arc: a parabola through the bottoms, on time scaled to 0..1.
    if lows.windows(2).any(|w| w[1] <= w[0]) {
        return None;
    }
    let span = (lows[n - 1] - lows[0]) as f64;
    let x: Vec<f64> = lows.iter().map(|&i| (i - lows[0]) as f64 / span).collect();
    let mut best: Option<(f64, [f64; 3], f64)> = None; // (cost, coefficients, rms)
    let droppable = if n >= 5 { 1..n - 1 } else { 0..0 };
    for dropped in std::iter::once(None).chain(droppable.map(Some)) {
        let kept = |k: &usize| Some(*k) != dropped;
        let xs: Vec<f64> = (0..n).filter(kept).map(|k| x[k]).collect();
        let ys: Vec<f64> = (0..n).filter(kept).map(|k| y[k]).collect();
        let Some(coef) = fit_parabola(&xs, &ys) else {
            continue;
        };
        let residual = |k: usize| y[k] - (coef[0] + coef[1] * x[k] + coef[2] * x[k] * x[k]);
        if let Some(d) = dropped {
            // An exception is a mild dip below the arc, nothing else.
            let r = residual(d);
            if r > 0.0 || -r > ARC_MAX_UNDERCUT * amp {
                continue;
            }
        }
        let sum_sq: f64 = (0..n).filter(kept).map(|k| residual(k).powi(2)).sum();
        let rms = (sum_sq / xs.len() as f64).sqrt() / amp;
        let cost = rms + if dropped.is_some() { 0.1 } else { 0.0 };
        if best.map_or(true, |(c, _, _)| cost < c) {
            best = Some((cost, coef, rms));
        }
    }
    let (_, [_, b, c], rms) = best?;
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
    if bend < ARC_MIN_BEND || sink > ARC_MAX_SINK * amp || rms > ARC_MAX_RMS {
        return None;
    }

    // Tops: a band, neither sliding nor running away from the bottoms.
    let top_max = top_y.iter().copied().fold(f64::MIN, f64::max);
    let top_min = top_y.iter().copied().fold(f64::MAX, f64::min);
    let half = top_y.len() / 2;
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let top_trend = mean(&top_y[top_y.len() - half..]) - mean(&top_y[..half]);
    if top_max - top_min > ARC_MAX_TOP_SPAN * amp
        || top_trend < -ARC_MAX_TOP_DROP * amp
        || top_trend > lift + ARC_MAX_TOP_LEAD * amp
    {
        return None;
    }

    // Rhythm: bottoms spread over the structure rather than bunched at one
    // end, and the last one recent enough that the arc is still the story.
    let mut gaps: Vec<f64> = lows.windows(2).map(|w| (w[1] - w[0]) as f64).collect();
    let longest_gap = gaps.iter().copied().fold(0.0, f64::max);
    if longest_gap > (0.4 + 0.6 / (n - 1) as f64) * span {
        return None;
    }
    let since = (last_bar - lows[n - 1]) as f64;
    if since > ARC_MAX_STALE * median(&mut gaps)? {
        return None;
    }
    // Where the bottoms' average climb puts them today; price must not have
    // fallen through it.
    let floor = y[n - 1] + lift / span * since;
    let current = if running.peak {
        last_close
    } else {
        last_close.min(held[held.len() - 1])
    };
    if current < floor - ARC_FLOOR_SLACK * amp {
        return None;
    }

    let len = last_bar - start;
    if first_top.saturating_sub(start) as f64 > ARC_MAX_RALLY_SHARE * len as f64
        || last_bar.saturating_sub(first_top) < ARC_MIN_BARS
        || last_close - top_max > ARC_MAX_BREAK * amp
    {
        return None;
    }

    let pct = |log_ratio: f64| (log_ratio.exp() - 1.0) * 100.0;
    Some(ArcStructure {
        lows: n,
        len,
        run: pct(logh[first_top] - logl[start]),
        amp: pct(amp),
        lift: pct(lift),
        bend: bend * 100.0,
        breakout: pct(last_close - top_max),
    })
}

/// Median of `values` (which it sorts); `None` when empty.
fn median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.total_cmp(b));
    let mid = values.len() / 2;
    Some(if values.len() % 2 == 0 {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    })
}

/// Least-squares `y = a + b·x + c·x²`, returned as `[a, b, c]`. `None` when
/// the points don't determine a parabola (fewer than three distinct `x`).
fn fit_parabola(xs: &[f64], ys: &[f64]) -> Option<[f64; 3]> {
    // Normal equations, solved by Gaussian elimination with partial pivoting.
    let mut m = [[0.0f64; 4]; 3];
    for (&x, &y) in xs.iter().zip(ys) {
        let powers = [1.0, x, x * x];
        for r in 0..3 {
            for c in 0..3 {
                m[r][c] += powers[r] * powers[c];
            }
            m[r][3] += powers[r] * y;
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
            "arc_lows" => Some(arc.lows as f64),
            "arc_len" => Some(arc.len as f64),
            "arc_run" => Some(arc.run),
            "arc_amp" => Some(arc.amp),
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

    /// A sketch from scratch: a flat start at `base`, then the given turning
    /// points (pixel heights, alternating top / bottom) 60 px apart.
    fn swing_sketch(base: f64, turns: &[f64]) -> Vec<(f64, f64)> {
        let mut pts = vec![(0.0, base), (20.0, base)];
        pts.extend(turns.iter().enumerate().map(|(i, &y)| (80.0 + 60.0 * i as f64, y)));
        pts
    }

    #[test]
    fn arc_found_on_each_sketch_at_every_stage() {
        // The shape should be recognised while it is still forming, not only
        // once the last swing is in: also try with trailing vertices removed.
        for (i, sketch) in arc_sketches().iter().enumerate() {
            for cut in 0..4 {
                let candles = candles_from_sketch(&sketch[..sketch.len() - cut]);
                let arcs = find_arc_structures(&candles);
                assert!(
                    arcs.iter().any(|a| a.lows >= 4),
                    "sketch {} minus {} vertices: {:?}",
                    i + 1,
                    cut,
                    arcs
                );
            }
        }
    }

    #[test]
    fn arc_metrics_describe_the_sketch() {
        // Second sketch: a ~35% rally, then six bottoms on ~20% swings that
        // lift ~19% along a clearly bent arc; price is still inside the range.
        let arcs = find_arc_structures(&candles_from_sketch(&arc_sketches()[1]));
        let a = arcs.iter().find(|a| a.lows == 6).expect("six-bottom structure");
        assert_eq!(a.len, 215);
        assert!((a.run - 35.5).abs() < 1.0, "{a:?}");
        assert!((a.amp - 20.7).abs() < 1.0, "{a:?}");
        assert!((a.lift - 19.4).abs() < 1.0, "{a:?}");
        assert!(a.bend > 30.0, "{a:?}");
        assert!(a.breakout < 0.0, "{a:?}");
    }

    #[test]
    fn arc_needs_bottoms_rising_on_an_arc_under_level_tops() {
        let found = |turns: &[f64]| find_arc_structures(&candles_from_sketch(&swing_sketch(800.0, turns)));

        // Bottoms 650 → 640 → 610 → 560 (lower pixel = higher price): each
        // step up bigger than the last, tops nearly level.
        let arc = found(&[500.0, 650.0, 490.0, 640.0, 480.0, 610.0, 470.0, 560.0, 460.0]);
        assert!(arc.iter().any(|a| a.lows == 4 && a.bend > 0.0), "{arc:?}");

        // The same swings with the bottoms stepping down instead.
        let falling = found(&[500.0, 560.0, 490.0, 610.0, 480.0, 640.0, 470.0, 650.0, 460.0]);
        assert!(falling.is_empty(), "{falling:?}");

        // Bottoms that rise fast and then stall (90, 30, 10): losing steam.
        let fading = found(&[500.0, 650.0, 410.0, 560.0, 380.0, 530.0, 370.0, 520.0, 360.0]);
        assert!(fading.is_empty(), "{fading:?}");

        // A steep channel: tops climb as fast as the bottoms — a trend, not a
        // sideways fight.
        let channel = found(&[500.0, 650.0, 390.0, 540.0, 280.0, 430.0, 170.0, 320.0, 60.0]);
        assert!(channel.is_empty(), "{channel:?}");
    }

    #[test]
    fn arc_gone_once_price_breaks_the_bottoms() {
        let mut sketch = arc_sketches()[1].clone();
        sketch.truncate(sketch.len() - 1);
        assert!(!find_arc_structures(&candles_from_sketch(&sketch)).is_empty());
        // …then a slide back below every bottom since the second.
        sketch.push((980.0, 1095.0));
        let arcs = find_arc_structures(&candles_from_sketch(&sketch));
        assert!(arcs.is_empty(), "{arcs:?}");
    }

    #[test]
    fn arc_columns_parse_and_take_no_offset() {
        assert!(parse_filter_line("arc_lows >= 4 and arc_bend > 0").is_ok());
        assert!(parse_filter_line("week: arc_break <= 3").is_ok());
        assert!(parse_filter_line("arc_lows[-1] >= 4").is_err());
        assert!(parse_filter_line("arc_foo >= 4").is_err());
    }

    #[test]
    fn arc_filter_binds_one_structure_per_line() {
        // The second sketch holds a six-bottom structure 215 bars long and,
        // from a later starting low, a five-bottom one 166 bars long.
        let dir = stock_dir_with_candles(
            "chart_app_arc_filter",
            &candles_from_sketch(&arc_sketches()[1]),
        );
        let ma = vec![];
        let holds = |lines: &[&str], periods: &[Period]| {
            evaluate_filters_on_stock(&dir, &filter_lines(lines), &ma, periods)
        };
        let daily = &[Period::Daily];

        assert!(holds(&["arc_lows >= 6"], daily));
        assert!(!holds(&["arc_lows >= 7"], daily));
        assert!(holds(&["arc_lows >= 6 and arc_len >= 200"], daily));
        // On one line both conditions must be met by the same structure…
        assert!(!holds(&["arc_lows >= 6 and arc_len <= 200"], daily));
        // …while separate lines may each be met by a different one.
        assert!(holds(&["arc_lows >= 6", "arc_len <= 200"], daily));


        // Each timeframe has its own structures: the same shape shows on the
        // weekly bars, but there it is ~30 bars long, not ~200.
        assert!(holds(&["week: arc_lows >= 6"], daily));
        assert!(holds(&["day: arc_len >= 100"], daily));
        assert!(!holds(&["week: arc_len >= 100"], daily));
        // An unprefixed line passes if any of the bundle's periods has one.
        assert!(holds(&["arc_len >= 100"], &[Period::Weekly, Period::Daily]));
        assert!(!holds(&["arc_len >= 100"], &[Period::Weekly]));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn arc_columns_are_unresolved_without_a_structure() {
        // A steady climb has no swings, so there is nothing for `arc_*` to
        // describe: comparisons are false either way round, not "zero".
        let closes: Vec<f64> = (1..=120).map(|i| 10.0 + i as f64 * 0.1).collect();
        let dir = stock_dir_with_closes("chart_app_arc_none", &closes);
        let ma = vec![];
        let holds = |line: &str| {
            evaluate_filters_on_stock(&dir, &filter_lines(&[line]), &ma, &[Period::Daily])
        };

        assert!(!holds("arc_lows >= 3"));
        assert!(!holds("arc_lows < 3"));
        // …which still lets an `or` branch carry the line.
        assert!(holds("arc_lows >= 3 or close > ma20"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
