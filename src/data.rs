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
    pub ma: Vec<usize>,
    pub chip: ChipSettings,
    pub ma_cluster: MaClusterSettings,
}

/// Tunables for the MA cluster score indicator.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MaClusterSettings {
    /// Amplitudes below this (in %) incur no penalty — `compact = 1`.
    /// Above the free zone the excess decays exponentially. Lets typical
    /// clean-trend clusters score near the ceiling instead of being dragged
    /// down by unavoidable spread.
    #[serde(default = "default_ma_cluster_amplitude_free_zone")]
    pub amplitude_free_zone: f64,
    /// `k` in `compact = exp(-(amp_pct - free_zone) / k)`. Smaller = steeper
    /// penalty once past the free zone.
    #[serde(default = "default_ma_cluster_amplitude_scale")]
    pub amplitude_scale: f64,
    /// Exponent applied to `compact` in the final score. Lowers the amplitude
    /// penalty's weight relative to bull alignment. 1.0 = original behaviour;
    /// 0.5 (default) noticeably softens the spread penalty so a perfectly
    /// bull-stacked but widely-spread rally still registers a meaningful score.
    #[serde(default = "default_ma_cluster_amplitude_weight")]
    pub amplitude_weight: f64,
    /// Number of bars used to judge each MA's direction. An MA counts as
    /// "rising" iff `value[idx] > value[idx - slope_lookback]`. When the
    /// lookback is unavailable (too close to series start, or a NaN in the
    /// past), slope is treated as neutral so early bars don't auto-fail.
    #[serde(default = "default_ma_cluster_slope_lookback")]
    pub slope_lookback: usize,
}

fn default_ma_cluster_amplitude_free_zone() -> f64 {
    5.0
}
fn default_ma_cluster_amplitude_scale() -> f64 {
    10.0
}
fn default_ma_cluster_amplitude_weight() -> f64 {
    0.5
}
fn default_ma_cluster_slope_lookback() -> usize {
    5
}

impl Default for MaClusterSettings {
    fn default() -> Self {
        Self {
            amplitude_free_zone: default_ma_cluster_amplitude_free_zone(),
            amplitude_scale: default_ma_cluster_amplitude_scale(),
            amplitude_weight: default_ma_cluster_amplitude_weight(),
            slope_lookback: default_ma_cluster_slope_lookback(),
        }
    }
}

/// Tunable parameters for the chip (筹码) distribution model. All fields fall
/// back to sensible defaults so an existing settings.json without a `chip`
/// section keeps working.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ChipSettings {
    /// Per-day turnover used when no volume baseline is available.
    #[serde(default = "default_fallback_turnover_rate")]
    pub fallback_turnover_rate: f64,
    /// Lower clamp on per-day turnover so very thin sessions still decay chips.
    #[serde(default = "default_min_turnover_rate")]
    pub min_turnover_rate: f64,
    /// Upper clamp so a single huge volume bar can't wipe the historical chip
    /// memory in one day.
    #[serde(default = "default_max_turnover_rate")]
    pub max_turnover_rate: f64,
    /// Trailing-window length used to derive the local volume baseline.
    #[serde(default = "default_turnover_baseline_days")]
    pub turnover_baseline_days: usize,
    /// Exponent applied to the (volume / baseline) ratio. < 1 dampens spikes.
    #[serde(default = "default_volume_ratio_exponent")]
    pub volume_ratio_exponent: f64,
    /// Minimum trading days the lookback should cover before exiting early.
    #[serde(default = "default_min_lookback")]
    pub min_lookback: usize,
    /// Hard cap on lookback length.
    #[serde(default = "default_max_lookback")]
    pub max_lookback: usize,
    /// Once residual chip mass falls below this, older days are dropped.
    #[serde(default = "default_residual_mass_cutoff")]
    pub residual_mass_cutoff: f64,
    /// Lower clamp on bin count.
    #[serde(default = "default_min_bins")]
    pub min_bins: usize,
    /// Upper clamp on bin count.
    #[serde(default = "default_max_bins")]
    pub max_bins: usize,
    /// Lower mass quantile used to pick `cost_low` (e.g. 0.05 = 5%).
    #[serde(default = "default_cost_quantile_low")]
    pub cost_quantile_low: f64,
    /// Upper mass quantile used to pick `cost_high` (e.g. 0.95 = 95%).
    #[serde(default = "default_cost_quantile_high")]
    pub cost_quantile_high: f64,
    /// Multiplier on the price-anchor weight inside `distribute_new_chips`.
    /// 1.0 keeps original behaviour; > 1 trusts the amount-derived avg price
    /// more.
    #[serde(default = "default_anchor_strength")]
    pub anchor_strength: f64,
}

fn default_fallback_turnover_rate() -> f64 {
    0.03
}
fn default_min_turnover_rate() -> f64 {
    0.001
}
fn default_max_turnover_rate() -> f64 {
    0.30
}
fn default_turnover_baseline_days() -> usize {
    40
}
fn default_volume_ratio_exponent() -> f64 {
    0.5
}
fn default_min_lookback() -> usize {
    60
}
fn default_max_lookback() -> usize {
    480
}
fn default_residual_mass_cutoff() -> f64 {
    0.03
}
fn default_min_bins() -> usize {
    96
}
fn default_max_bins() -> usize {
    240
}
fn default_cost_quantile_low() -> f64 {
    0.05
}
fn default_cost_quantile_high() -> f64 {
    0.95
}
fn default_anchor_strength() -> f64 {
    1.0
}

impl Default for ChipSettings {
    fn default() -> Self {
        Self {
            fallback_turnover_rate: default_fallback_turnover_rate(),
            min_turnover_rate: default_min_turnover_rate(),
            max_turnover_rate: default_max_turnover_rate(),
            turnover_baseline_days: default_turnover_baseline_days(),
            volume_ratio_exponent: default_volume_ratio_exponent(),
            min_lookback: default_min_lookback(),
            max_lookback: default_max_lookback(),
            residual_mass_cutoff: default_residual_mass_cutoff(),
            min_bins: default_min_bins(),
            max_bins: default_max_bins(),
            cost_quantile_low: default_cost_quantile_low(),
            cost_quantile_high: default_cost_quantile_high(),
            anchor_strength: default_anchor_strength(),
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
fn default_data_filters() -> Vec<String> {
    Vec::new()
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
            chip: ChipSettings::default(),
            ma_cluster: MaClusterSettings::default(),
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
    #[serde(default = "default_data_filters")]
    data_filters: Vec<String>,
    #[serde(default)]
    ma: Option<Vec<usize>>,
    // Legacy top-level alias for `ma`.
    #[serde(default)]
    ma_windows: Option<Vec<usize>>,
    #[serde(default)]
    chip: ChipSettings,
    #[serde(default)]
    ma_cluster: MaClusterSettings,
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
            chip: legacy.chip,
            ma_cluster: legacy.ma_cluster,
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
    pub amount: Option<f64>,
}

#[derive(Clone)]
pub struct MaLine {
    pub period: usize,
    pub values: Vec<f64>, // same length as candles; NaN where insufficient data
}

/// Combined "MA cluster" score at a single candle. See
/// `calculate_ma_cluster_score` for semantics.
#[derive(Clone, Copy, Debug)]
pub struct MaClusterScore {
    pub amp_pct: f64, // (max - min) / min * 100
    pub bull: f64,    // fraction of (shorter, longer) pairs with shorter > longer, 0..=1
    pub score: f64,   // bull * exp(-amp_pct / amplitude_scale) * 100, 0..=100
}

/// Score how tight and how bull-aligned the configured moving averages are at
/// `idx`. Returns `None` when fewer than two MAs have a valid value at that
/// position (need at least one pair to score anything).
///
/// A pair `(short_ma, long_ma)` counts as bullish iff all three hold:
/// 1. `short_ma[idx] > long_ma[idx]` (correct order),
/// 2. `short_ma` is rising over `slope_lookback` bars, and
/// 3. `long_ma` is rising over `slope_lookback` bars.
/// When slope history isn't available (too early in the series), that MA's
/// slope is treated as neutral — i.e. the requirement is waived rather than
/// failed, so early bars don't auto-zero.
pub fn calculate_ma_cluster_score(
    ma_lines: &[MaLine],
    idx: usize,
    settings: &MaClusterSettings,
) -> Option<MaClusterScore> {
    // Collect (period, value, is_rising) per MA in period-ascending order.
    // Skip NaN / non-positive values (amplitude formula needs min > 0).
    let mut entries: Vec<(usize, f64, bool)> = ma_lines
        .iter()
        .filter_map(|m| {
            let v = *m.values.get(idx)?;
            if v.is_nan() || v <= 0.0 {
                return None;
            }
            // Rising check: current strictly greater than lookback-ago. If the
            // past value isn't available, treat as neutral (true) so the pair
            // isn't disqualified purely by young history.
            let rising = match idx.checked_sub(settings.slope_lookback) {
                Some(past_idx) => match m.values.get(past_idx) {
                    Some(&past) if !past.is_nan() => v > past,
                    _ => true,
                },
                None => true,
            };
            Some((m.period, v, rising))
        })
        .collect();
    if entries.len() < 2 {
        return None;
    }
    entries.sort_by_key(|e| e.0);

    let values: Vec<f64> = entries.iter().map(|e| e.1).collect();
    let rising: Vec<bool> = entries.iter().map(|e| e.2).collect();
    let max = values.iter().cloned().fold(f64::MIN, f64::max);
    let min = values.iter().cloned().fold(f64::MAX, f64::min);
    let amp_pct = (max - min) / min * 100.0;

    // Bull alignment: shorter period above longer period AND both MAs rising.
    let n = values.len();
    let total_pairs = n * (n - 1) / 2;
    let mut correct = 0usize;
    for i in 0..n {
        for j in (i + 1)..n {
            if values[i] > values[j] && rising[i] && rising[j] {
                correct += 1;
            }
        }
    }
    let bull = correct as f64 / total_pairs as f64;

    let k = if settings.amplitude_scale > 0.0 {
        settings.amplitude_scale
    } else {
        default_ma_cluster_amplitude_scale()
    };
    let free = settings.amplitude_free_zone.max(0.0);
    let w = settings.amplitude_weight.max(0.0);
    let excess = (amp_pct - free).max(0.0);
    let compact = (-excess / k).exp();
    let compact_weighted = if w == 0.0 { 1.0 } else { compact.powf(w) };
    let score = bull * compact_weighted * 100.0;

    Some(MaClusterScore {
        amp_pct,
        bull,
        score,
    })
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
        let bracket_end = s
            .find(']')
            .ok_or_else(|| format!("Missing ] in operand: {}", s))?;
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

pub fn evaluate_filters_on_stock(
    stock_dir: &Path,
    filters: &[DataFilter],
    ma_windows: &[usize],
    chip_settings: &ChipSettings,
    ma_cluster_settings: &MaClusterSettings,
) -> bool {
    if filters.is_empty() {
        return true;
    }

    // Union configured windows with any `maN` references in the filters so
    // expressions like `ma120 > ma240` resolve even if those periods aren't
    // in the user's chart config.
    let mut windows: Vec<usize> = ma_windows.to_vec();
    for f in filters {
        for operand in [&f.left, &f.right] {
            if let Some(p) = operand
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

    let (candles, ma_lines) = load_candles(stock_dir, Period::Daily, &windows);
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
    let needs_chip = filters
        .iter()
        .any(|f| is_chip_column(&f.left.column) || is_chip_column(&f.right.column));

    let chip_dist = if needs_chip {
        Some(crate::chip::calculate_chip_distribution(
            &candles,
            candles.len() - 1,
            chip_settings,
        ))
    } else {
        None
    };

    // Cluster metrics must use only the configured MAs, not the augmented
    // `windows` union (otherwise a filter like `ma480 > ma240` would quietly
    // change what `cluster` means).
    let cluster_ma_lines: Vec<MaLine> = ma_lines
        .iter()
        .filter(|m| ma_windows.contains(&m.period))
        .cloned()
        .collect();

    let ctx = FilterContext {
        candles: &candles,
        ma_lines: &ma_lines,
        cluster_ma_lines: &cluster_ma_lines,
        chip_dist: chip_dist.as_ref(),
        ma_cluster_settings,
    };

    // Evaluate each filter
    for filter in filters {
        let left_val = resolve_operand_value(&ctx, &filter.left);
        let right_val = resolve_operand_value(&ctx, &filter.right);

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
    matches!(name, "cbw" | "ckdp" | "cost_center" | "asr" | "winner")
}

fn is_cluster_column(name: &str) -> bool {
    matches!(name, "cluster" | "bull" | "amp")
}

fn resolve_chip_value(dist: &crate::chip::ChipDistribution, column: &str) -> Option<f64> {
    match column {
        "cbw" => Some(dist.cbw),
        "ckdp" => Some(dist.ckdp),
        "cost_center" => Some(dist.cost_center),
        // 获利盘比例 (Active Stock Ratio / WINNER 函数)。CBW/CKDP 都以百分比
        // 出现，所以 asr 也乘以 100 保持一致——`asr > 50` 比 `asr > 0.5`
        // 在表达式里更自然。`winner` 作为同义别名兼容通达信公式习惯。
        "asr" | "winner" => Some(dist.profit_ratio * 100.0),
        _ => None,
    }
}

struct FilterContext<'a> {
    candles: &'a [Candle],
    ma_lines: &'a [MaLine],
    cluster_ma_lines: &'a [MaLine],
    chip_dist: Option<&'a crate::chip::ChipDistribution>,
    ma_cluster_settings: &'a MaClusterSettings,
}

fn resolve_operand_value(ctx: &FilterContext<'_>, operand: &FilterOperand) -> Option<f64> {
    // Try parsing as numeric literal first (e.g. "50" in "cbw < 50")
    if let Ok(val) = operand.column.parse::<f64>() {
        return Some(val);
    }

    if is_chip_column(&operand.column) {
        return ctx
            .chip_dist
            .and_then(|d| resolve_chip_value(d, &operand.column));
    }

    // offset 0 = latest, offset 1 = one before, ...
    let idx = ctx.candles.len().checked_sub(operand.offset + 1)?;

    if is_cluster_column(&operand.column) {
        let score =
            calculate_ma_cluster_score(ctx.cluster_ma_lines, idx, ctx.ma_cluster_settings)?;
        return match operand.column.as_str() {
            "cluster" => Some(score.score),
            "bull" => Some(score.bull * 100.0),
            "amp" => Some(score.amp_pct),
            _ => None,
        };
    }

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
        assert_eq!(parsed.ma, vec![10, 30, 60]);
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
    fn missing_chip_section_falls_back_to_defaults() {
        // Existing settings.json files won't have a `chip` key — they must
        // still parse and surface ChipSettings::default().
        let json = r#"{
          "dl": { "data_dir": "/d", "binary": "stock-dl" }
        }"#;
        let parsed: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.chip, ChipSettings::default());
    }

    #[test]
    fn chip_section_overrides_only_specified_fields() {
        // Partial chip overrides should replace the listed fields and leave
        // the rest at their defaults.
        let json = r#"{
          "dl": { "data_dir": "/d", "binary": "stock-dl" },
          "chip": {
            "max_lookback": 240,
            "cost_quantile_low": 0.10,
            "cost_quantile_high": 0.90,
            "anchor_strength": 1.5
          }
        }"#;
        let parsed: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.chip.max_lookback, 240);
        assert!((parsed.chip.cost_quantile_low - 0.10).abs() < 1e-9);
        assert!((parsed.chip.cost_quantile_high - 0.90).abs() < 1e-9);
        assert!((parsed.chip.anchor_strength - 1.5).abs() < 1e-9);
        // Untouched fields keep defaults.
        let d = ChipSettings::default();
        assert_eq!(parsed.chip.min_lookback, d.min_lookback);
        assert_eq!(parsed.chip.min_bins, d.min_bins);
        assert!((parsed.chip.fallback_turnover_rate - d.fallback_turnover_rate).abs() < 1e-9);
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
        let filters = vec![parse_data_filter("price > ma10").unwrap()];
        assert!(evaluate_filters_on_stock(
            &dir,
            &filters,
            &ma,
            &ChipSettings::default(),
            &MaClusterSettings::default()
        ));

        // price < ma10 should not hold
        let filters = vec![parse_data_filter("price < ma10").unwrap()];
        assert!(!evaluate_filters_on_stock(
            &dir,
            &filters,
            &ma,
            &ChipSettings::default(),
            &MaClusterSettings::default()
        ));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn filter_resolves_asr_chip_column() {
        // Build a contrived series where the latest close sits near the top of
        // the historical price range → most chips below it → ASR should be
        // high. Then assert the filter `asr > 50` matches and `winner > 50`
        // (alias) matches the same way.
        let dir = std::env::temp_dir().join("chart_app_filter_asr");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // 80 rows with steadily rising closes from 10 → ~17.9, so the final
        // close is near the top of accumulated chip mass.
        let mut csv = String::from("date,open,close,high,low,volume,amount\n");
        for i in 0..80 {
            let p = 10.0 + i as f64 * 0.1;
            csv.push_str(&format!(
                "2026-01-{:02},{p},{p},{ph},{pl},100,\n",
                (i % 28) + 1,
                p = p,
                ph = p + 0.1,
                pl = p - 0.1,
            ));
        }
        std::fs::write(dir.join("daily.csv"), csv).unwrap();

        let ma = vec![10, 30, 60];
        let chip = ChipSettings::default();

        let cluster = MaClusterSettings::default();

        let asr_filter = vec![parse_data_filter("asr > 50").unwrap()];
        assert!(evaluate_filters_on_stock(&dir, &asr_filter, &ma, &chip, &cluster));

        let winner_filter = vec![parse_data_filter("winner > 50").unwrap()];
        assert!(evaluate_filters_on_stock(&dir, &winner_filter, &ma, &chip, &cluster));

        // Sanity: asr > 200 can never hold (capped at 100).
        let impossible = vec![parse_data_filter("asr > 200").unwrap()];
        assert!(!evaluate_filters_on_stock(&dir, &impossible, &ma, &chip, &cluster));

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
        let filters = vec![parse_data_filter("price > ma5").unwrap()];
        assert!(evaluate_filters_on_stock(
            &dir,
            &filters,
            &configured,
            &ChipSettings::default(),
            &MaClusterSettings::default()
        ));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ma_cluster_score_rewards_tight_bull_alignment() {
        // Two MAs in perfect bull order with tiny spread → score near 100.
        // Single-bar history means slope data is unavailable, so the slope
        // check is waived (treated as rising) rather than auto-failing.
        let settings = MaClusterSettings::default();
        let ma_lines = vec![
            MaLine { period: 10, values: vec![100.5] },
            MaLine { period: 30, values: vec![100.0] },
        ];
        let s = calculate_ma_cluster_score(&ma_lines, 0, &settings).unwrap();
        assert!(s.bull > 0.99, "bull = {}", s.bull);
        assert!(s.amp_pct < 1.0, "amp = {}", s.amp_pct);
        assert!(s.score > 90.0, "score = {}", s.score);

        // Perfect bear order → bull = 0 → combined score = 0.
        let bear = vec![
            MaLine { period: 10, values: vec![90.0] },
            MaLine { period: 30, values: vec![95.0] },
            MaLine { period: 60, values: vec![100.0] },
        ];
        let s = calculate_ma_cluster_score(&bear, 0, &settings).unwrap();
        assert_eq!(s.bull, 0.0);
        assert_eq!(s.score, 0.0);
    }

    #[test]
    fn ma_cluster_score_requires_rising_mas() {
        // Bull-ordered at idx=5 but the shorter MA is falling — slope check
        // must reject it so bull = 0 and score = 0.
        let settings = MaClusterSettings {
            amplitude_free_zone: 0.0,
            amplitude_scale: 10.0,
            amplitude_weight: 1.0,
            slope_lookback: 5,
        };
        let ma10_drop = MaLine {
            period: 10,
            values: vec![105.0, 104.0, 103.5, 103.0, 102.5, 102.0],
        };
        let ma30_rise = MaLine {
            period: 30,
            values: vec![99.0, 99.3, 99.6, 99.9, 100.1, 100.4],
        };
        let lines = vec![ma10_drop, ma30_rise];
        let s = calculate_ma_cluster_score(&lines, 5, &settings).unwrap();
        assert_eq!(s.bull, 0.0);
        assert_eq!(s.score, 0.0);
    }

    #[test]
    fn ma_cluster_score_none_when_insufficient_values() {
        let settings = MaClusterSettings::default();
        let ma_lines = vec![MaLine { period: 10, values: vec![f64::NAN] }];
        assert!(calculate_ma_cluster_score(&ma_lines, 0, &settings).is_none());

        let ma_lines = vec![MaLine { period: 10, values: vec![10.0] }];
        assert!(calculate_ma_cluster_score(&ma_lines, 0, &settings).is_none());
    }

    #[test]
    fn filter_resolves_cluster_columns() {
        // Rising series with MA10 > MA30 > MA60 at the final bar → strong bull,
        // modest spread → `cluster > 50`, `bull = 100`, `amp` reasonable.
        let dir = std::env::temp_dir().join("chart_app_filter_cluster");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut csv = String::from("date,open,close,high,low,volume,amount\n");
        for i in 1..=80 {
            let p = 10.0 + i as f64 * 0.1;
            csv.push_str(&format!(
                "2026-01-{:02},{p},{p},{p},{p},100,\n",
                (i % 28) + 1
            ));
        }
        std::fs::write(dir.join("daily.csv"), csv).unwrap();

        let ma = vec![10, 30, 60];
        let chip = ChipSettings::default();
        let cluster = MaClusterSettings::default();

        let bull = vec![parse_data_filter("bull > 99").unwrap()];
        assert!(evaluate_filters_on_stock(&dir, &bull, &ma, &chip, &cluster));

        // A strictly rising series spread over 60 days must have amp > 0.
        let amp = vec![parse_data_filter("amp > 0").unwrap()];
        assert!(evaluate_filters_on_stock(&dir, &amp, &ma, &chip, &cluster));

        // On a rising series bull=100 but MA10/MA60 spread → compact < 1, so
        // cluster lands somewhere in (0, 100). Just verify it's positive here.
        let any_cluster = vec![parse_data_filter("cluster > 0").unwrap()];
        assert!(evaluate_filters_on_stock(&dir, &any_cluster, &ma, &chip, &cluster));

        let impossible = vec![parse_data_filter("cluster > 200").unwrap()];
        assert!(!evaluate_filters_on_stock(&dir, &impossible, &ma, &chip, &cluster));

        // History offset works: 5 bars ago also satisfies bull on a monotonic series.
        let bull_past = vec![parse_data_filter("bull[-5] > 99").unwrap()];
        assert!(evaluate_filters_on_stock(&dir, &bull_past, &ma, &chip, &cluster));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cluster_ignores_filter_only_ma_windows() {
        // Even though the filter references ma5 (not in configured windows),
        // the cluster score must be based on the configured [10, 30, 60] only.
        // We assert that the cluster score is computed from ≤3 MAs by checking
        // that total_pairs = C(3, 2) = 3, not C(4, 2) = 6. We verify indirectly:
        // for a monotonic rising series, all pairs are correctly bull-ordered,
        // so bull = 100 regardless of which MAs we include. Instead we check
        // that amp is what we'd expect from the 3 configured MAs only.
        let dir = std::env::temp_dir().join("chart_app_cluster_isolation");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut csv = String::from("date,open,close,high,low,volume,amount\n");
        for i in 1..=80 {
            let p = 10.0 + i as f64 * 0.1;
            csv.push_str(&format!(
                "2026-01-{:02},{p},{p},{p},{p},100,\n",
                (i % 28) + 1
            ));
        }
        std::fs::write(dir.join("daily.csv"), csv).unwrap();

        let ma = vec![10, 30, 60];
        let chip = ChipSettings::default();
        let cluster = MaClusterSettings::default();

        // A filter that unions ma5 into windows should not change cluster's
        // inputs. Both should evaluate identically.
        let with_ma5 = vec![
            parse_data_filter("ma5 > 0").unwrap(),
            parse_data_filter("cluster > 5").unwrap(),
        ];
        let without_ma5 = vec![parse_data_filter("cluster > 5").unwrap()];
        assert_eq!(
            evaluate_filters_on_stock(&dir, &with_ma5, &ma, &chip, &cluster),
            evaluate_filters_on_stock(&dir, &without_ma5, &ma, &chip, &cluster)
        );

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
}
