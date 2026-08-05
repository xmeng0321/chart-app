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
    /// Visits every comparison in the tree (used to collect MA references).
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
    Err(format!(
        "未知字段: {} (可用: close/price, open, high, low, volume, maN)",
        column
    ))
}

/// A stock matches when every line holds. A line with an explicit `day:` /
/// `week:` prefix is evaluated on that timeframe; an unprefixed line is
/// evaluated on `default_periods` and passes if it holds on any of them.
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
        line.expr.for_each_cmp(&mut |f| {
            for operand in [&f.left, &f.right] {
                for op in [Some(operand), operand.divisor.as_deref()].into_iter().flatten() {
                    if let Some(p) = op
                        .column
                        .strip_prefix("ma")
                        .and_then(|s| s.parse::<usize>().ok())
                    {
                        if !windows.contains(&p) {
                            windows.push(p);
                        }
                    }
                }
            }
        });
    }

    let defaults: &[Period] = if default_periods.is_empty() {
        &[Period::Daily]
    } else {
        default_periods
    };

    // Candles are loaded at most once per timeframe and shared across lines.
    let mut cache: HashMap<Period, (Vec<Candle>, Vec<MaLine>)> = HashMap::new();

    for line in lines {
        let pass = match line.period {
            Some(p) => eval_line_on_period(stock_dir, &mut cache, &windows, &line.expr, p),
            None => defaults
                .iter()
                .any(|&p| eval_line_on_period(stock_dir, &mut cache, &windows, &line.expr, p)),
        };
        if !pass {
            return false;
        }
    }
    true
}

fn eval_line_on_period(
    stock_dir: &Path,
    cache: &mut HashMap<Period, (Vec<Candle>, Vec<MaLine>)>,
    windows: &[usize],
    expr: &FilterExpr,
    period: Period,
) -> bool {
    let (candles, ma_lines) = cache
        .entry(period)
        .or_insert_with(|| load_candles(stock_dir, period, windows));
    if candles.is_empty() {
        return false;
    }
    let ctx = FilterContext {
        candles,
        ma_lines,
    };
    eval_expr(&ctx, expr)
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
}
