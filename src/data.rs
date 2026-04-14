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

pub fn load_filtered() -> Vec<String> {
    let path = filtered_path();
    if let Ok(content) = std::fs::read_to_string(&path) {
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Vec::new()
    }
}

pub fn save_filtered(filtered: &[String]) {
    let path = filtered_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(filtered) {
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
    pub data_filters: Vec<String>,
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
fn default_data_filters() -> Vec<String> {
    Vec::new()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            dl: DlSettings::default(),
            data_filters: default_data_filters(),
        }
    }
}

#[derive(Deserialize, Default)]
struct LegacyAppSettings {
    #[serde(default)]
    dl: Option<DlSettings>,
    #[serde(default = "default_data_dir")]
    data_dir: String,
    #[serde(default = "default_stock_dl_binary")]
    stock_dl_binary: String,
    #[serde(default = "default_concurrency")]
    concurrency: usize,
    #[serde(default = "default_data_filters")]
    data_filters: Vec<String>,
}

impl<'de> Deserialize<'de> for AppSettings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let legacy = LegacyAppSettings::deserialize(deserializer)?;
        let dl = legacy.dl.unwrap_or_else(|| DlSettings {
            data_dir: legacy.data_dir,
            binary: legacy.stock_dl_binary,
            concurrency: legacy.concurrency,
        });

        Ok(Self {
            dl,
            data_filters: legacy.data_filters,
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
}

#[derive(Clone)]
pub struct MaLine {
    pub period: usize,
    pub values: Vec<f64>, // same length as candles; NaN where insufficient data
}

#[derive(Clone, Copy, PartialEq)]
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

/// Default MA windows computed on demand (no MA columns in the new data).
pub const DEFAULT_MA_WINDOWS: &[usize] = &[10, 30, 60];

pub fn load_candles(stock_dir: &Path, period: Period) -> (Vec<Candle>, Vec<MaLine>) {
    let daily = read_daily_csv(&stock_dir.join("daily.csv"));
    let candles = match period {
        Period::Daily => daily,
        Period::Weekly => aggregate_weekly(&daily),
    };

    let closes: Vec<f64> = candles.iter().map(|c| c.close).collect();
    let ma_lines = DEFAULT_MA_WINDOWS
        .iter()
        .map(|&p| MaLine {
            period: p,
            values: calculate_ma(&closes, p),
        })
        .collect();

    (candles, ma_lines)
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
        candles.push(Candle {
            timestamp: ts,
            open: o,
            close: c,
            high: h,
            low: l,
            volume: v,
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
    let era = if year >= 0 { year / 400 } else { (year - 399) / 400 };
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

#[derive(Debug, Clone)]
pub struct DataFilter {
    pub left: FilterOperand,
    pub operator: FilterOperator,
    pub right: FilterOperand,
    #[allow(dead_code)]
    pub raw: String,
}

#[derive(Debug, Clone)]
pub struct FilterOperand {
    pub column: String,
    pub offset: usize, // 0 = latest row, 1 = row[-1], etc.
}

#[derive(Debug, Clone, Copy)]
pub enum FilterOperator {
    GreaterThan,
    LessThan,
}

pub fn parse_data_filter(expr: &str) -> Result<DataFilter, String> {
    let (left_str, op, right_str) = if let Some(pos) = expr.find('>') {
        (&expr[..pos], FilterOperator::GreaterThan, &expr[pos + 1..])
    } else if let Some(pos) = expr.find('<') {
        (&expr[..pos], FilterOperator::LessThan, &expr[pos + 1..])
    } else {
        return Err(format!("No > or < operator found in filter: {}", expr));
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
        return Err("Empty filter operand".to_string());
    }

    let (column, offset) = if let Some(bracket_start) = s.find('[') {
        let bracket_end = s.find(']').ok_or_else(|| format!("Missing ] in operand: {}", s))?;
        let col = s[..bracket_start].trim();
        let offset_str = s[bracket_start + 1..bracket_end].trim();
        let offset_val: i64 = offset_str
            .parse()
            .map_err(|_| format!("Invalid offset in operand: {}", s))?;
        if offset_val > 0 {
            return Err(format!("Offset must be <= 0 in operand: {}", s));
        }
        (col.to_string(), (-offset_val) as usize)
    } else {
        (s.to_string(), 0)
    };

    // Alias "price" → "close"
    let column = if column == "price" {
        "close".to_string()
    } else {
        column.to_lowercase()
    };

    Ok(FilterOperand { column, offset })
}

pub fn evaluate_filters_on_stock(stock_dir: &Path, filters: &[DataFilter]) -> bool {
    if filters.is_empty() {
        return true;
    }

    let (candles, ma_lines) = load_candles(stock_dir, Period::Daily);
    if candles.is_empty() {
        return false;
    }

    // Determine how many trailing rows we need
    let max_offset = filters
        .iter()
        .flat_map(|f| [f.left.offset, f.right.offset])
        .max()
        .unwrap_or(0);
    if candles.len() <= max_offset {
        return false;
    }

    // Lazily compute chip distribution only when a filter needs it
    let needs_chip = filters.iter().any(|f| {
        is_chip_column(&f.left.column) || is_chip_column(&f.right.column)
    });

    let chip_dist = if needs_chip {
        Some(crate::chip::calculate_chip_distribution(
            &candles,
            candles.len() - 1,
        ))
    } else {
        None
    };

    // Evaluate each filter
    for filter in filters {
        let left_val = resolve_operand_value(&candles, &ma_lines, &filter.left, chip_dist.as_ref());
        let right_val = resolve_operand_value(&candles, &ma_lines, &filter.right, chip_dist.as_ref());

        let (Some(lv), Some(rv)) = (left_val, right_val) else {
            return false;
        };

        let pass = match filter.operator {
            FilterOperator::GreaterThan => lv > rv,
            FilterOperator::LessThan => lv < rv,
        };

        if !pass {
            return false;
        }
    }

    true
}

fn is_chip_column(name: &str) -> bool {
    matches!(name, "cbw" | "ckdp" | "cost_center")
}

fn resolve_chip_value(
    dist: &crate::chip::ChipDistribution,
    column: &str,
) -> Option<f64> {
    match column {
        "cbw" => Some(dist.cbw),
        "ckdp" => Some(dist.ckdp),
        "cost_center" => Some(dist.cost_center),
        _ => None,
    }
}

fn resolve_operand_value(
    candles: &[Candle],
    ma_lines: &[MaLine],
    operand: &FilterOperand,
    chip_dist: Option<&crate::chip::ChipDistribution>,
) -> Option<f64> {
    // Try parsing as numeric literal first (e.g. "50" in "cbw < 50")
    if let Ok(val) = operand.column.parse::<f64>() {
        return Some(val);
    }

    if is_chip_column(&operand.column) {
        return chip_dist.and_then(|d| resolve_chip_value(d, &operand.column));
    }

    // offset 0 = latest, offset 1 = one before, ...
    let idx = candles.len().checked_sub(operand.offset + 1)?;

    // MA columns (ma10, ma30, ma60, …) come from on-demand MA lines
    if let Some(period_str) = operand.column.strip_prefix("ma") {
        if let Ok(period) = period_str.parse::<usize>() {
            let ma = ma_lines.iter().find(|m| m.period == period)?;
            let val = *ma.values.get(idx)?;
            if val.is_nan() {
                return None;
            }
            return Some(val);
        }
    }

    let c = candles.get(idx)?;
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
        assert_eq!(parsed.data_filters, vec!["price > ma60".to_string()]);
    }

    #[test]
    fn parses_legacy_settings_with_dropped_fields_ignored() {
        // Older settings files may still have ma_windows/source/periods. These
        // must not cause a parse failure.
        let legacy = r#"{
          "dl": {
            "data_dir": "/some/dir",
            "binary": "stock-dl",
            "ma_windows": [10,30,60],
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
            Candle { timestamp: "2025-12-29".into(), open: 10.0, close: 11.0, high: 11.5, low: 9.5, volume: 100.0 },
            Candle { timestamp: "2025-12-30".into(), open: 11.0, close: 12.0, high: 12.5, low: 10.5, volume: 200.0 },
            Candle { timestamp: "2026-01-02".into(), open: 12.0, close: 13.0, high: 13.5, low: 11.5, volume: 300.0 },
            // Next week (Mon)
            Candle { timestamp: "2026-01-05".into(), open: 13.0, close: 14.0, high: 14.5, low: 12.5, volume: 400.0 },
            Candle { timestamp: "2026-01-06".into(), open: 14.0, close: 13.5, high: 14.8, low: 13.0, volume: 500.0 },
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

        // Week 2
        assert_eq!(weekly[1].timestamp, "2026-01-06");
        assert!((weekly[1].open - 13.0).abs() < 1e-9);
        assert!((weekly[1].close - 13.5).abs() < 1e-9);
        assert!((weekly[1].volume - 900.0).abs() < 1e-9);
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

        // price > ma10 should hold on a rising series
        let filters = vec![parse_data_filter("price > ma10").unwrap()];
        assert!(evaluate_filters_on_stock(&dir, &filters));

        // price < ma10 should not hold
        let filters = vec![parse_data_filter("price < ma10").unwrap()];
        assert!(!evaluate_filters_on_stock(&dir, &filters));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
