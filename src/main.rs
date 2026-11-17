mod chart;
mod data;

use chart::{draw_chart, ma_color, AgentStatus, ChartState, SelectionState};
use data::*;
use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::io::BufRead;
use std::path::PathBuf;
use std::sync::mpsc;
const MODEL_OPTIONS: &[&str] = &[
    "auto",
    "MiniMax-M2.7",
    "qwen3.5-plus",
    "deepseek-reasoner",
    "gpt-5.4-mini",
];

const FILTER_SYNTAX_HELP: &str = "\
字段: close (= price), open, high, low, volume, maN (ma10, ma250, ...)
区间字段 (最近 N 根, 含当前这根):
    hhvN  最高价        llvN  最低价        vmaN  平均成交量
    posN  收盘在区间里的位置 (0 = llvN, 100 = hhvN)
    atrN  平均日振幅% (真实波幅 / 前收盘)
    例如 close/llv120 >= 1.4, pos60 >= 60, volume/vma20[-1] >= 1.5
历史值: close[-1], ma100[-2]  ([N] = N 根之前, 正负号等价)
比较: >  >=  <  <=   除法: price/price[-200] <= 2.0
单行内: and / or 与括号, 例如
    day: close > ma120 and (ma60 > ma120 or ma120 > ma120[-1])
行首周期: day: / week: (不写则用该筛选的默认周期: 一般是 day; 名称旁标 [日+周] 的是日线或周线任一), 例如
    week: close > ma100
    day: close > close[-10]
多行之间是 and; 空行与 # 开头的行会被忽略
越振越强 (一段陡直的上涨, 之后震荡的重心缓慢抬高, 底部沿加速的弧线抬升, 至今有效):
    arc_len   起涨低点至今的K线数        arc_run   起涨那一段的涨幅%
    arc_peak  起涨低点到之后最高点的涨幅%  arc_amp   典型震荡幅度%
    arc_rise  震荡重心累计抬高%          arc_lift  底部弧线累计抬高%
    arc_bend  弧线弯曲度 (至少 10, 越大越弯)
    arc_break 现价相对起涨以来最高点% (总是 <= 0)
    同一行里的 arc_* 指同一段走势, 多个条件请用 and 写在一行, 例如
    arc_len >= 40 and arc_peak <= 100 and arc_break >= -10
下破变脸 (先横盘震荡, 然后跌破箱底, 之后又收回箱体并继续回升; 箱体长短不限):
    pit_box   横盘的K线数              pit_amp   箱体高度% (箱底到箱顶, 按收盘价)
    pit_drop  跌破箱底的深度% (到坑底最低价)  pit_under 收在箱底之下的K线数
    pit_len   坑底至今的K线数           pit_up    坑底以来的涨幅%
    pit_pos   现价在箱体中的位置 (0 = 箱底, 100 = 箱顶, 大于 100 = 已涨出箱顶)
    同一行里的 pit_* 指同一段走势, 多个条件请用 and 写在一行, 例如
    pit_box >= 30 and pit_amp <= 40 and pit_len <= 40 and pit_pos <= 100";

/// The kind of list in the left panel. There is one `Filtered` tab per filter
/// bundle; which of them is open is `ChartApp::selected_filter_name`.
#[derive(Clone, Copy, PartialEq)]
enum StockTab {
    All,
    Favorites,
    Filtered,
}

impl StockTab {
    fn label(&self) -> &str {
        match self {
            StockTab::All => "全部",
            StockTab::Favorites => "自选",
            StockTab::Filtered => "筛选",
        }
    }

    /// Stable key used for persisting the "last selected stock" per tab.
    fn key(&self) -> &'static str {
        match self {
            StockTab::All => "all",
            StockTab::Favorites => "favorites",
            StockTab::Filtered => "filtered",
        }
    }
}

/// State of the filter editor dialog (a bundle's tab → 编辑, or the + tab).
struct FilterEditor {
    /// Bundle being edited; `None` when creating a new one.
    original_name: Option<String>,
    name: String,
    /// One condition per line, exactly as the user typed it.
    text: String,
    /// Timeframes for lines without a `day:` / `week:` prefix. Not editable:
    /// kept from the bundle being edited, or taken from a preset.
    periods: Vec<Period>,
    /// Set when a save attempt failed (empty name, duplicate, bad syntax).
    error: Option<String>,
    /// 删除 asks once before actually removing the bundle.
    confirm_delete: bool,
}

impl FilterEditor {
    fn new(bundle: Option<&NamedFilter>) -> Self {
        Self {
            original_name: bundle.map(|b| b.name.clone()),
            name: bundle.map(|b| b.name.clone()).unwrap_or_default(),
            text: bundle.map(|b| b.filters.join("\n")).unwrap_or_default(),
            periods: bundle.map_or_else(|| vec![Period::Daily], |b| b.periods.clone()),
            error: None,
            confirm_delete: false,
        }
    }

    /// Editor text as stored lines: trailing whitespace and trailing blank
    /// lines dropped, everything else (including comments) kept verbatim.
    fn lines(&self) -> Vec<String> {
        let mut lines: Vec<String> = self
            .text
            .lines()
            .map(|l| l.trim_end().to_string())
            .collect();
        while lines.last().is_some_and(|l| l.trim().is_empty()) {
            lines.pop();
        }
        lines
    }

    /// Per-line parse errors, recomputed every frame for live feedback.
    fn issues(&self) -> Vec<String> {
        self.text
            .lines()
            .enumerate()
            .filter(|(_, l)| !is_blank_or_comment(l))
            .filter_map(|(i, l)| {
                parse_filter_line(l)
                    .err()
                    .map(|e| format!("第 {} 行: {}", i + 1, e))
            })
            .collect()
    }
}

enum EditorAction {
    None,
    Save,
    Delete,
    Close,
}

/// What a click on a row of a stock list asks for.
enum RowClick {
    Select,
    ToggleFavorite,
}

/// Key in `last_selected.json` for the stock last viewed in a bundle's list.
fn filter_selection_key(bundle: &str) -> String {
    format!("{}:{}", StockTab::Filtered.key(), bundle)
}

/// Whether a stock passes the search box. `search_lower` is `search`
/// lowercased, computed once per list rather than once per row.
fn matches_search(info: &StockInfo, search: &str, search_lower: &str) -> bool {
    search_lower.is_empty()
        || info.code.to_lowercase().contains(search_lower)
        || info.name.contains(search)
        || info.secid.to_lowercase().contains(search_lower)
}

/// One row of a stock list: the favourite star and the stock itself.
/// `scroll_to` brings the row into view.
fn stock_row(
    ui: &mut egui::Ui,
    info: &StockInfo,
    is_fav: bool,
    selected: bool,
    scroll_to: bool,
) -> Option<RowClick> {
    let (star, star_color) = if is_fav {
        ("★", egui::Color32::from_rgb(0xFF, 0xD7, 0x00))
    } else {
        ("☆", egui::Color32::from_rgb(0x55, 0x55, 0x55))
    };
    let mut click = None;
    ui.horizontal(|ui| {
        let star = egui::RichText::new(star).color(star_color).size(14.0);
        if ui.add(egui::Button::new(star).frame(false)).clicked() {
            click = Some(RowClick::ToggleFavorite);
        }
        let stock = ui.selectable_label(selected, format!("{} {}", info.secid, info.name));
        if stock.clicked() {
            click = Some(RowClick::Select);
        }
        if scroll_to {
            stock.scroll_to_me(Some(egui::Align::Center));
        }
    });
    click
}

/// Everything the structure overlay depends on: the stock, its timeframe and
/// bar count (a sync appends bars), and the filter bundle in force — `None`
/// outside a filter bundle's tab, where nothing is drawn.
type OverlayKey = (Option<usize>, Period, usize, Option<String>);

struct ChartApp {
    stocks: Vec<(StockInfo, PathBuf)>,
    search: String,
    selected: Option<usize>,
    period: Period,
    candles: Vec<Candle>,
    ma_lines: Vec<MaLine>,
    ma_visible: Vec<bool>,
    ma_windows: Vec<usize>,
    chart_state: ChartState,
    csv_path: Option<PathBuf>,
    agent_receiver: Option<mpsc::Receiver<Result<TradeAgentResult, String>>>,
    model: String,
    csv_as_chart: bool,
    stock_tab: StockTab,
    favorites: Vec<String>,
    sync_status: SyncStatus,
    sync_receiver: Option<mpsc::Receiver<SyncMessage>>,
    /// Cached matches of every filter bundle, by bundle name (what
    /// `filtered.json` holds). A bundle without an entry has not been run
    /// since it was created or its conditions last changed.
    filtered: HashMap<String, Vec<String>>,
    /// Each bundle is run, and reports, on its own: the status line of its
    /// tab, and the channel of its scan while one runs.
    filter_status: HashMap<String, FilterStatus>,
    filter_receivers: HashMap<String, mpsc::Receiver<FilterMessage>>,
    /// The bundle in focus: the one whose tab is open, or was open last.
    /// On its tab, arrow keys walk its list and the chart marks the
    /// structures it matched. Persisted across launches.
    selected_filter_name: Option<String>,
    /// Open filter editor dialog, if any.
    filter_editor: Option<FilterEditor>,
    /// The structures drawn on the chart when the stock on screen was picked
    /// by an `arc_*` / `pit_*` filter, and what they were computed for (see
    /// `refresh_overlay`).
    overlay: MatchedShapes,
    overlay_key: Option<OverlayKey>,
    /// Toolbar switch for drawing `overlay`.
    show_overlay: bool,
    /// Last stock (secid) viewed in each tab. Persisted so a tab switch or
    /// app restart restores the user's position per-tab.
    last_selected_by_tab: HashMap<String, String>,
    /// One-shot flag: if true, the stock list scrolls to reveal the currently
    /// selected row on the next frame.
    scroll_to_selected: bool,
    /// Secid whose per-stock auto-sync (`stock-dl --codes <code>`) is currently
    /// running in the background. `None` means no auto-sync in flight.
    auto_sync_secid: Option<String>,
    auto_sync_receiver: Option<mpsc::Receiver<(String, Result<(), String>)>>,
    /// Secids we've already attempted to auto-sync this session. We fire at
    /// most once per stock per run so arrow-keying through the list doesn't
    /// spawn a stock-dl process for every row.
    auto_synced: HashSet<String>,
}

impl ChartApp {
    fn new(cc: &eframe::CreationContext<'_>, data_dir: Option<PathBuf>) -> Self {
        setup_fonts(&cc.egui_ctx);
        cc.egui_ctx.set_visuals(egui::Visuals::dark());

        let settings = load_settings();
        let data_dir = data_dir.unwrap_or_else(|| settings_data_dir(&settings));

        let stocks = load_stock_list(&data_dir);
        let favorites = load_favorites();
        let last_selected_by_tab = load_last_selected();
        // Restore the bundle last in focus, falling back to the first one so
        // there is always a focus when any filters are configured.
        let selected_filter_name = load_selected_filter()
            .filter(|n| settings.data_filters.iter().any(|f| &f.name == n))
            .or_else(|| settings.data_filters.first().map(|f| f.name.clone()));
        let filtered = load_all_filtered();
        let mut app = Self {
            stocks,
            search: String::new(),
            selected: None,
            period: Period::Daily,
            candles: Vec::new(),
            ma_lines: Vec::new(),
            ma_visible: Vec::new(),
            ma_windows: settings.ma.clone(),
            chart_state: ChartState::new(),
            csv_path: None,
            agent_receiver: None,
            model: "auto".to_string(),
            csv_as_chart: false,
            stock_tab: StockTab::All,
            favorites,
            sync_status: SyncStatus::Idle,
            sync_receiver: None,
            filtered,
            filter_status: HashMap::new(),
            filter_receivers: HashMap::new(),
            selected_filter_name,
            filter_editor: None,
            overlay: MatchedShapes::default(),
            overlay_key: None,
            show_overlay: true,
            last_selected_by_tab,
            scroll_to_selected: false,
            auto_sync_secid: None,
            auto_sync_receiver: None,
            auto_synced: HashSet::new(),
        };

        // Restore the stock last viewed on the initial tab (falls back to the
        // first visible stock if nothing was remembered).
        app.restore_selection_for_current_tab();

        app
    }

    /// Puts bundle `name` in focus — the one a `Filtered` tab shows.
    /// Remembered across launches.
    fn focus_filter(&mut self, name: &str) {
        if self.selected_filter_name.as_deref() != Some(name) {
            self.selected_filter_name = Some(name.to_string());
            save_selected_filter(name);
        }
    }

    /// Matches of the bundle in focus (none if it has not been run).
    fn focus_matches(&self) -> &[String] {
        self.selected_filter_name
            .as_ref()
            .and_then(|name| self.filtered.get(name))
            .map_or(&[], |matches| matches.as_slice())
    }

    /// Where the stock on screen is remembered: per tab, a filter bundle's
    /// tab included, so every list keeps its own place.
    fn selection_key(&self) -> String {
        match (self.stock_tab, &self.selected_filter_name) {
            (StockTab::Filtered, Some(name)) => filter_selection_key(name),
            (tab, _) => tab.key().to_string(),
        }
    }

    fn select_stock(&mut self, idx: usize) {
        self.selected = Some(idx);
        // Remember it so future tab switches / restarts can restore it.
        if let Some((info, _)) = self.stocks.get(idx) {
            self.last_selected_by_tab
                .insert(self.selection_key(), info.secid.clone());
            save_last_selected(&self.last_selected_by_tab);
        }
        if let Some(period) = self.matched_period(idx) {
            self.period = period;
        }
        self.reload_candles();
    }

    /// On a filter bundle's tab, the timeframe on which stock `idx` meets
    /// that bundle, so the chart opens where the match can be seen: the
    /// timeframe holding the structure for `arc_*` / `pit_*` bundles (the
    /// current one if both do), otherwise the bundle's timeframe when it reads
    /// only one. `None` leaves the chart on the timeframe it is on.
    fn matched_period(&self, idx: usize) -> Option<Period> {
        if self.stock_tab != StockTab::Filtered {
            return None;
        }
        let name = self.selected_filter_name.as_ref()?;
        let settings = load_settings();
        let bundle = settings.data_filters.iter().find(|f| &f.name == name)?;
        let lines = parse_filter_lines(&bundle.filters).ok()?;
        let (_, dir) = self.stocks.get(idx)?;

        let other = match self.period {
            Period::Daily => Period::Weekly,
            Period::Weekly => Period::Daily,
        };
        // The list is what the last scan found; if the data has moved on and
        // the stock no longer matches, go where the structure it was picked
        // for can still be seen.
        let still_matches =
            evaluate_filters_on_stock(dir, &lines, &self.ma_windows, &bundle.periods);
        let has_shape = |period: Period| {
            let (candles, ma_lines) = load_candles(dir, period, &self.ma_windows);
            shapes_to_draw(&lines, &bundle.periods, period, &candles, &ma_lines, still_matches)
                .has_structure()
        };
        if let Some(period) = [self.period, other].into_iter().find(|&p| has_shape(p)) {
            return Some(period);
        }
        match bundle.active_periods().as_slice() {
            [only] => Some(*only),
            _ => None,
        }
    }

    /// Select the stock the user was last viewing on the current tab, falling
    /// back to the first visible entry if there is no remembered stock (or if
    /// it is no longer in the list). Sets the scroll-to-selected flag so the
    /// UI scrolls the chosen row into view on the next frame.
    fn restore_selection_for_current_tab(&mut self) {
        let visible = self.visible_stock_indices();
        if visible.is_empty() {
            self.selected = None;
            self.candles.clear();
            self.ma_lines.clear();
            return;
        }

        let remembered = self
            .last_selected_by_tab
            .get(&self.selection_key())
            // Before bundles had a tab each, they shared one place.
            .or_else(|| self.last_selected_by_tab.get(self.stock_tab.key()))
            .and_then(|secid| {
                visible
                    .iter()
                    .find(|&&i| self.stocks[i].0.secid == *secid)
                    .copied()
            });
        let target = remembered.unwrap_or(visible[0]);

        if self.selected != Some(target) {
            self.select_stock(target);
        }
        self.scroll_to_selected = true;
    }

    fn reload_candles(&mut self) {
        if let Some(idx) = self.selected {
            let had_data = !self.candles.is_empty();
            let had_data_len = self.candles.len();
            let (_, dir) = &self.stocks[idx];
            let (candles, ma_lines) = load_candles(dir, self.period, &self.ma_windows);
            // Only daily.csv exists on disk — weekly candles are computed
            // in-memory, so trade-agent always gets the daily file.
            self.csv_path = Some(dir.join("daily.csv"));
            self.candles = candles;
            self.ma_visible = vec![true; ma_lines.len()];
            self.ma_lines = ma_lines;
            if had_data {
                // Preserve zoom level and distance from right edge
                let old_n = had_data_len as f64;
                let right_margin = old_n
                    - (self.chart_state.offset + self.chart_state.candles_in_view);
                let n = self.candles.len() as f64;
                self.chart_state.offset = n - self.chart_state.candles_in_view - right_margin;
                self.chart_state.auto_price = true;
                self.chart_state.selection = SelectionState::Idle;
                self.chart_state.agent_status = AgentStatus::Idle;
            } else {
                self.chart_state.fit_to_data(&self.candles);
            }
            self.agent_receiver = None;
        }
    }

    /// The stocks arrow keys walk through: those on the current tab that
    /// pass the search.
    fn visible_stock_indices(&self) -> Vec<usize> {
        let search_lower = self.search.to_lowercase();
        self.stocks
            .iter()
            .enumerate()
            .filter(|(_, (info, _))| {
                matches_search(info, &self.search, &search_lower)
                    && match self.stock_tab {
                        StockTab::Favorites => self.favorites.contains(&info.secid),
                        StockTab::Filtered => self.focus_matches().contains(&info.secid),
                        StockTab::All => true,
                    }
            })
            .map(|(i, _)| i)
            .collect()
    }

    fn run_trade_agent(&mut self, ctx: &egui::Context) {
        let (start, end) = match &self.chart_state.selection {
            SelectionState::Selected { start, end } => (*start, *end),
            _ => return,
        };

        let Some(csv_path) = &self.csv_path else {
            return;
        };

        let start_date = &self.candles[start].timestamp;
        let end_date = &self.candles[end].timestamp;
        // Extract date part (first 10 chars: YYYY-MM-DD)
        let start_date = if start_date.len() >= 10 {
            &start_date[..10]
        } else {
            start_date
        };
        let end_date = if end_date.len() >= 10 {
            &end_date[..10]
        } else {
            end_date
        };
        let date_range = format!("{}:{}", start_date, end_date);
        let csv_path = csv_path.clone();
        let ctx = ctx.clone();
        let model = self.model.clone();
        let csv_as_chart = self.csv_as_chart;

        self.chart_state.agent_status = AgentStatus::Running;

        let (tx, rx) = mpsc::channel();
        self.agent_receiver = Some(rx);

        let model_arg = if model != "auto" {
            format!(" --model {}", model)
        } else {
            String::new()
        };
        let chart_arg = format!(" --csv_as_chart {}", csv_as_chart);
        eprintln!(
            "\n[trade-agent] Running: trade-agent {} --date-range {}{}{}",
            csv_path.display(),
            date_range,
            model_arg,
            chart_arg
        );

        std::thread::spawn(move || {
            let mut cmd = std::process::Command::new("trade-agent");
            cmd.arg(csv_path.to_str().unwrap_or(""))
                .arg("--date-range")
                .arg(&date_range)
                .arg("--csv_as_chart")
                .arg(if csv_as_chart { "true" } else { "false" });
            if model != "auto" {
                cmd.arg("--model").arg(&model);
            }
            let result = cmd.output();

            let msg = match result {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    if !stdout.is_empty() {
                        eprintln!("[trade-agent] stdout:\n{}", stdout);
                    }
                    if !stderr.is_empty() {
                        eprintln!("[trade-agent] stderr:\n{}", stderr);
                    }
                    eprintln!("[trade-agent] exit code: {}", output.status);

                    if output.status.success() {
                        serde_json::from_slice::<TradeAgentResult>(&output.stdout)
                            .map_err(|e| format!("JSON parse error: {}", e))
                    } else {
                        Err(stderr.to_string())
                    }
                }
                Err(e) => {
                    eprintln!("[trade-agent] Failed to execute: {}", e);
                    Err(format!("Failed to run trade-agent: {}", e))
                }
            };

            let _ = tx.send(msg);
            ctx.request_repaint();
        });
    }

    /// Runs stock-dl in the background: for `symbols` only (the favourites,
    /// or one bundle's matches), or for every stock when `None`.
    fn run_sync(&mut self, ctx: &egui::Context, symbols: Option<Vec<String>>) {
        let settings = load_settings();
        let ctx = ctx.clone();

        self.sync_status = SyncStatus::Running {
            progress_pct: 0.0,
            completed: 0,
            total: 0,
            current_stock: String::new(),
        };

        let (tx, rx) = mpsc::channel();
        self.sync_receiver = Some(rx);

        std::thread::spawn(move || {
            // Expand ~ in data_dir
            let data_dir = if settings.dl.data_dir.starts_with("~/") {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                format!("{}/{}", home, &settings.dl.data_dir[2..])
            } else {
                settings.dl.data_dir.clone()
            };

            // New CLI: `stock-dl -o <dir> download [--codes ...] [-c N]`
            let mut cmd = std::process::Command::new(&settings.dl.binary);
            cmd.arg("-o").arg(&data_dir);
            cmd.arg("download");
            cmd.arg("-c").arg(settings.dl.concurrency.to_string());

            // Limit to favorites/filtered when supplied — new flag is --codes,
            // which takes bare codes (no market prefix).
            if let Some(ref syms) = symbols {
                let codes: Vec<String> = syms
                    .iter()
                    .map(|s| strip_secid_prefix(s).to_string())
                    .collect();
                cmd.arg("--codes").arg(codes.join(","));
            }

            cmd.stdout(std::process::Stdio::piped());
            // stock-dl draws its progress bar on stderr, and only when that is
            // a terminal: give it a pseudo-terminal and read the bar back.
            let pty_ours = match stderr_pty() {
                Some((ours, theirs)) => {
                    cmd.stderr(theirs);
                    Some(ours)
                }
                None => {
                    cmd.stderr(std::process::Stdio::piped());
                    None
                }
            };

            eprintln!("[stock-dl] Running: {:?}", cmd);

            let spawned = cmd.spawn();
            // `cmd` holds our copy of the terminal's child end: close it, or
            // reading our end would never see stock-dl exit.
            drop(cmd);
            let mut child = match spawned {
                Ok(child) => child,
                Err(e) => {
                    let _ = tx.send(SyncMessage::Finished(Err(format!(
                        "Failed to run {}: {}",
                        settings.dl.binary, e
                    ))));
                    ctx.request_repaint();
                    return;
                }
            };

            let stderr: Box<dyn std::io::Read + Send> = match (pty_ours, child.stderr.take()) {
                (Some(ours), _) => Box::new(ours),
                (None, Some(pipe)) => Box::new(pipe),
                (None, None) => Box::new(std::io::empty()),
            };
            let stderr_reader = {
                let (tx, ctx) = (tx.clone(), ctx.clone());
                std::thread::spawn(move || read_dl_stderr(stderr, &tx, &ctx))
            };

            // stdout: the summary, and progress lines from older stock-dl.
            let stdout = child.stdout.take().unwrap();
            let reader = std::io::BufReader::new(stdout);
            // New stock-dl format: separate updated / skipped / failed fields.
            // Old format: a single `finished: success=N, failed=M` line.
            let mut last_updated = 0usize;
            let mut last_skipped = 0usize;
            let mut last_failed = 0usize;

            for line in reader.lines() {
                let Ok(line) = line else { continue };
                eprintln!("[stock-dl] {}", line);

                if let Some((s, f)) = parse_finished_line(&line) {
                    // Old stock-dl: `finished: success=N, failed=M`
                    last_updated = s;
                    last_failed = f;
                } else if let Some((key, n)) = parse_dl_summary_field(&line) {
                    // New stock-dl: `  updated  : N`, `  skipped  : N`, `  failed   : N`
                    match key {
                        "updated" => last_updated = n,
                        "skipped" => last_skipped = n,
                        "failed" => last_failed = n,
                        _ => {}
                    }
                } else if let Some((pct, completed, total, stock_label)) =
                    parse_progress_line(&line)
                {
                    let _ = tx.send(SyncMessage::Progress {
                        pct,
                        completed,
                        total,
                        stock_label,
                    });
                    ctx.request_repaint();
                }
            }

            let status = child.wait();
            let stderr_output = stderr_reader.join().unwrap_or_default();

            // Skipped = already up-to-date, which is a successful outcome.
            let success = last_updated + last_skipped;
            let result = match status {
                Ok(s) if s.success() => Ok((success, last_failed)),
                Ok(_) => Err(if stderr_output.is_empty() {
                    "stock-dl exited with error".to_string()
                } else {
                    stderr_output
                }),
                Err(e) => Err(format!("Process error: {}", e)),
            };

            let _ = tx.send(SyncMessage::Finished(result));
            ctx.request_repaint();
        });
    }

    /// Fire a background `stock-dl --codes <code>` for the currently selected
    /// stock if we haven't already tried it this session. No-ops when a batch
    /// sync is in progress, another auto-sync is in flight, nothing is
    /// selected, or this stock has already been attempted.
    fn try_start_auto_sync(&mut self, ctx: &egui::Context) {
        if matches!(self.sync_status, SyncStatus::Running { .. }) {
            return;
        }
        if self.auto_sync_receiver.is_some() {
            return;
        }
        let Some(idx) = self.selected else { return };
        let secid = self.stocks[idx].0.secid.clone();
        if !self.auto_synced.insert(secid.clone()) {
            return;
        }

        self.auto_sync_secid = Some(secid.clone());

        let settings = load_settings();
        let code = strip_secid_prefix(&secid).to_string();
        let ctx = ctx.clone();
        let (tx, rx) = mpsc::channel();
        self.auto_sync_receiver = Some(rx);

        std::thread::spawn(move || {
            let data_dir = if settings.dl.data_dir.starts_with("~/") {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                format!("{}/{}", home, &settings.dl.data_dir[2..])
            } else {
                settings.dl.data_dir.clone()
            };

            let mut cmd = std::process::Command::new(&settings.dl.binary);
            cmd.arg("-o").arg(&data_dir);
            cmd.arg("download");
            cmd.arg("--codes").arg(&code);

            eprintln!("[auto-sync] {} -> {:?}", secid, cmd);

            let msg = match cmd.output() {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    if !stdout.is_empty() {
                        eprintln!("[auto-sync] stdout:\n{}", stdout);
                    }
                    if !stderr.is_empty() {
                        eprintln!("[auto-sync] stderr:\n{}", stderr);
                    }
                    if output.status.success() {
                        Ok(())
                    } else {
                        Err(stderr.trim().to_string())
                    }
                }
                Err(e) => Err(format!("failed to run {}: {}", settings.dl.binary, e)),
            };

            let _ = tx.send((secid, msg));
            ctx.request_repaint();
        });
    }

    fn resolve_data_dir(&self) -> PathBuf {
        settings_data_dir(&load_settings())
    }

    /// Scans every stock with bundle `name` in the background. Bundles are
    /// scanned independently, so several can run at once; the matches come
    /// back through the bundle's own channel (see `update`).
    fn run_filter(&mut self, ctx: &egui::Context, name: &str) {
        if self.filter_receivers.contains_key(name) {
            return;
        }
        let settings = load_settings();
        let parsed = settings
            .data_filters
            .iter()
            .find(|f| f.name == name)
            .ok_or_else(|| "找不到该筛选 (settings.json: data_filters)".to_string())
            .and_then(|bundle| {
                let lines = parse_filter_lines(&bundle.filters)
                    .map_err(|e| format!("过滤条件解析失败: {}", e))?;
                if lines.is_empty() {
                    return Err("该筛选没有任何条件".to_string());
                }
                Ok((lines, bundle.periods.clone()))
            });
        let (filters, filter_periods) = match parsed {
            Ok(parsed) => parsed,
            Err(e) => {
                self.filter_status.insert(name.to_string(), FilterStatus::Error(e));
                return;
            }
        };

        let ma_windows = settings.ma.clone();
        let data_dir = self.resolve_data_dir();
        let ctx = ctx.clone();

        self.filter_status.insert(
            name.to_string(),
            FilterStatus::Running {
                progress_pct: 0.0,
                completed: 0,
                total: 0,
                matched: 0,
                current_stock: String::new(),
            },
        );

        let (tx, rx) = mpsc::channel();
        self.filter_receivers.insert(name.to_string(), rx);

        std::thread::spawn(move || {
            let stocks = load_stock_list(&data_dir);
            let total = stocks.len();
            let mut matched_secids = Vec::new();

            for (i, (info, dir)) in stocks.iter().enumerate() {
                if evaluate_filters_on_stock(dir, &filters, &ma_windows, &filter_periods) {
                    matched_secids.push(info.secid.clone());
                }

                let _ = tx.send(FilterMessage::Progress {
                    completed: i + 1,
                    total,
                    matched: matched_secids.len(),
                    stock_label: format!("{} {}", info.secid, info.name),
                });
                ctx.request_repaint();
            }

            // The UI thread stores them: scans of several bundles may finish
            // together, and filtered.json is rewritten as a whole.
            matched_secids.sort();
            let _ = tx.send(FilterMessage::Finished(Ok((matched_secids, total))));
            ctx.request_repaint();
        });
    }

    /// Keeps `overlay` in step with what is on screen. Cheap to call every
    /// frame: the structures are only recomputed when the key changes.
    fn refresh_overlay(&mut self) {
        let bundle_name = (self.stock_tab == StockTab::Filtered)
            .then(|| self.selected_filter_name.clone())
            .flatten();
        let key: OverlayKey = (self.selected, self.period, self.candles.len(), bundle_name);
        if self.overlay_key.as_ref() == Some(&key) {
            return;
        }
        let shapes = key.3.as_ref().and_then(|name| {
            let settings = load_settings();
            let bundle = settings.data_filters.iter().find(|f| &f.name == name)?;
            let lines = parse_filter_lines(&bundle.filters).ok()?;
            let (_, dir) = self.stocks.get(self.selected?)?;
            // The stock is in the bundle's list, but that list is what the
            // last scan found: a sync since may have moved it out of the
            // conditions.
            let still_matches =
                evaluate_filters_on_stock(dir, &lines, &self.ma_windows, &bundle.periods);
            Some(shapes_to_draw(
                &lines,
                &bundle.periods,
                self.period,
                &self.candles,
                &self.ma_lines,
                still_matches,
            ))
        });
        self.overlay = shapes.unwrap_or_default();
        self.overlay_key = Some(key);
    }

    fn toggle_favorite(&mut self, secid: &str) {
        if self.favorites.iter().any(|s| s == secid) {
            self.favorites.retain(|s| s != secid);
        } else {
            self.favorites.push(secid.to_string());
            self.favorites.sort();
        }
        save_favorites(&self.favorites);
    }

    /// The list of the current tab: every stock, the favourites, or the
    /// matches of the bundle whose tab is open — those that pass the search.
    fn show_stock_list(&mut self, ui: &mut egui::Ui, scroll_to_selected: bool) {
        let clicked = {
            let search_lower = self.search.to_lowercase();
            let matches: HashSet<&str> = if self.stock_tab == StockTab::Filtered {
                self.focus_matches().iter().map(String::as_str).collect()
            } else {
                HashSet::new()
            };
            let mut clicked = None;
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                for (i, (info, _)) in self.stocks.iter().enumerate() {
                    if !matches_search(info, &self.search, &search_lower) {
                        continue;
                    }
                    let is_fav = self.favorites.contains(&info.secid);
                    let listed = match self.stock_tab {
                        StockTab::All => true,
                        StockTab::Favorites => is_fav,
                        StockTab::Filtered => matches.contains(info.secid.as_str()),
                    };
                    if !listed {
                        continue;
                    }
                    let selected = self.selected == Some(i);
                    let scroll_to = selected && scroll_to_selected;
                    if let Some(click) = stock_row(ui, info, is_fav, selected, scroll_to) {
                        clicked = Some((i, click));
                    }
                }
            });
            clicked
        };
        match clicked {
            Some((i, RowClick::Select)) => self.select_stock(i),
            Some((i, RowClick::ToggleFavorite)) => {
                let secid = self.stocks[i].0.secid.clone();
                self.toggle_favorite(&secid);
            }
            None => {}
        }
    }

    /// A filter bundle's tab: its own buttons and the timeframes it reads,
    /// its own status line, and below them its own list of matches.
    fn show_filter_tab(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        bundle: &NamedFilter,
        is_syncing: bool,
        scroll_to_selected: bool,
    ) {
        let dim = egui::Color32::from_gray(0x9E);
        let blue = egui::Color32::from_rgb(0x90, 0xCA, 0xF9);
        let small = |text: &str| egui::Button::new(egui::RichText::new(text).size(12.0));
        let note = |text: String, color| egui::RichText::new(text).size(11.0).color(color);

        let name = &bundle.name;
        let matches = self.filtered.get(name);
        let scanning = self.filter_receivers.contains_key(name);

        let (mut run, mut sync, mut edit) = (false, false, false);
        ui.horizontal(|ui| {
            let text = if scanning {
                "筛选中..."
            } else if matches.is_some() {
                "重新筛选"
            } else {
                "筛选"
            };
            run = ui.add_enabled(!scanning, small(text)).clicked();
            if scanning {
                ui.spinner();
            }
            let has_matches = matches.is_some_and(|m| !m.is_empty());
            sync = ui
                .add_enabled(has_matches && !is_syncing, small("同步"))
                .on_hover_text("更新这个列表里股票的行情数据")
                .clicked();
            // Not during its scan: the matches would come back for
            // conditions that have changed since.
            edit = ui.add_enabled(!scanning, small("编辑")).clicked();

            // Which timeframe(s) it reads, derived from the per-line
            // `day:` / `week:` prefixes.
            let periods = bundle.active_periods();
            let period_label = match (
                periods.contains(&Period::Daily),
                periods.contains(&Period::Weekly),
            ) {
                (true, true) => "日+周",
                (false, true) => "周",
                _ => "日",
            };
            ui.label(note(format!("[{period_label}]"), blue));
        });

        match (self.filter_status.get(name), matches) {
            (
                Some(FilterStatus::Running {
                    progress_pct,
                    completed,
                    total,
                    matched,
                    current_stock,
                }),
                _,
            ) => {
                ui.add(egui::ProgressBar::new(*progress_pct / 100.0).text(format!(
                    "{completed}/{total} 匹配:{matched} ({progress_pct:.1}%)"
                )));
                if !current_stock.is_empty() {
                    ui.label(note(current_stock.clone(), blue));
                }
            }
            (Some(FilterStatus::Done { matched, total }), _) => {
                ui.label(note(
                    format!("筛选完成: {matched}/{total} 只股票符合条件"),
                    egui::Color32::from_rgb(0x26, 0xa6, 0x9a),
                ));
            }
            (Some(FilterStatus::Error(msg)), _) => {
                ui.label(note(
                    format!("筛选错误: {msg}"),
                    egui::Color32::from_rgb(0xef, 0x53, 0x50),
                ));
            }
            (Some(FilterStatus::Stale), _) => {
                ui.label(note(
                    "条件已修改，请重新筛选".to_string(),
                    egui::Color32::from_rgb(0xFF, 0xB7, 0x4D),
                ));
            }
            // Not run this session: what the last run left behind.
            (None, Some(matches)) if matches.is_empty() => {
                ui.label(note("没有符合条件的股票".to_string(), dim));
            }
            (None, Some(matches)) => {
                ui.label(note(format!("{} 只股票符合条件", matches.len()), dim));
            }
            (None, None) => {
                ui.label(note("尚未筛选".to_string(), dim));
            }
        }
        ui.separator();

        if run {
            self.run_filter(ctx, name);
        }
        if sync {
            let matches = self.filtered.get(name).cloned();
            self.run_sync(ctx, matches);
        }
        if edit {
            self.open_filter_editor(Some(name));
        }
        self.show_stock_list(ui, scroll_to_selected);
    }

    fn open_filter_editor(&mut self, name: Option<&str>) {
        let settings = load_settings();
        let bundle = name.and_then(|n| settings.data_filters.iter().find(|f| f.name == n));
        self.filter_editor = Some(FilterEditor::new(bundle));
    }

    /// The editor dialog. Held outside `self` while drawing so the closure can
    /// mutate the editor without borrowing the whole app.
    fn show_filter_editor(&mut self, ctx: &egui::Context) {
        let Some(mut ed) = self.filter_editor.take() else {
            return;
        };

        let mut window_open = true;
        let mut action = EditorAction::None;
        let title = if ed.original_name.is_some() {
            "编辑筛选"
        } else {
            "新建筛选"
        };

        egui::Window::new(title)
            .open(&mut window_open)
            .resizable(true)
            .default_width(520.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("名称:").size(12.0));
                    ui.add(
                        egui::TextEdit::singleline(&mut ed.name)
                            .desired_width(220.0)
                            .hint_text("例如 long-term-bullish"),
                    );
                });
                if ed.original_name.is_none() {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("预设:").size(12.0));
                        for preset in preset_filters() {
                            if ui
                                .button(egui::RichText::new(&preset.name).size(12.0))
                                .on_hover_text("填入这套条件, 保存前可以修改")
                                .clicked()
                            {
                                ed.text = preset.filters.join("\n");
                                ed.name = preset.name;
                                ed.periods = preset.periods;
                                ed.error = None;
                            }
                        }
                    });
                }
                ui.add_space(4.0);
                ui.label(egui::RichText::new("条件 (每行一条，行之间是 and):").size(12.0));
                ui.add(
                    egui::TextEdit::multiline(&mut ed.text)
                        .font(egui::TextStyle::Monospace)
                        .desired_rows(10)
                        .desired_width(f32::INFINITY)
                        .code_editor()
                        .hint_text("week: close > ma100\nday: close > close[-10]"),
                );

                let issues = ed.issues();
                ui.add_space(4.0);
                if issues.is_empty() {
                    let count = ed
                        .text
                        .lines()
                        .filter(|l| !is_blank_or_comment(l))
                        .count();
                    ui.label(
                        egui::RichText::new(format!("✓ {} 个条件", count))
                            .size(11.0)
                            .color(egui::Color32::from_rgb(0x66, 0xBB, 0x6A)),
                    );
                } else {
                    for issue in &issues {
                        ui.label(
                            egui::RichText::new(issue)
                                .size(11.0)
                                .color(egui::Color32::from_rgb(0xef, 0x53, 0x50)),
                        );
                    }
                }

                if let Some(err) = &ed.error {
                    ui.label(
                        egui::RichText::new(err)
                            .size(11.0)
                            .color(egui::Color32::from_rgb(0xef, 0x53, 0x50)),
                    );
                }

                ui.add_space(4.0);
                egui::CollapsingHeader::new(egui::RichText::new("语法说明").size(12.0))
                    .id_salt("filter-syntax-help")
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(FILTER_SYNTAX_HELP)
                                .size(11.0)
                                .monospace(),
                        );
                    });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let can_save = !ed.name.trim().is_empty() && issues.is_empty();
                    if ui
                        .add_enabled(
                            can_save,
                            egui::Button::new(egui::RichText::new("保存").size(12.0)),
                        )
                        .clicked()
                    {
                        action = EditorAction::Save;
                    }
                    if ui
                        .add(egui::Button::new(egui::RichText::new("取消").size(12.0)))
                        .clicked()
                    {
                        action = EditorAction::Close;
                    }
                    if ed.original_name.is_some() {
                        ui.separator();
                        let label = if ed.confirm_delete {
                            "确认删除?"
                        } else {
                            "删除"
                        };
                        let btn = ui.add(
                            egui::Button::new(
                                egui::RichText::new(label)
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(0xef, 0x53, 0x50)),
                            ),
                        );
                        if btn.clicked() {
                            if ed.confirm_delete {
                                action = EditorAction::Delete;
                            } else {
                                ed.confirm_delete = true;
                            }
                        }
                    }
                });
            });

        match action {
            EditorAction::Save => {
                if self.save_filter_editor(&mut ed) {
                    return; // editor closed on success
                }
            }
            EditorAction::Delete => {
                if let Some(name) = ed.original_name.clone() {
                    self.delete_filter_bundle(&name);
                }
                return;
            }
            EditorAction::Close => return,
            EditorAction::None => {}
        }

        if window_open {
            self.filter_editor = Some(ed);
        }
    }

    /// Writes the edited bundle back to `settings.json`. Returns true when the
    /// save went through (the caller then drops the editor).
    fn save_filter_editor(&mut self, ed: &mut FilterEditor) -> bool {
        let name = ed.name.trim().to_string();
        if name.is_empty() {
            ed.error = Some("请输入筛选名称".to_string());
            return false;
        }

        let lines = ed.lines();
        let parsed = match parse_filter_lines(&lines) {
            Ok(p) => p,
            Err(e) => {
                ed.error = Some(e);
                return false;
            }
        };
        if parsed.is_empty() {
            ed.error = Some("至少需要一个条件".to_string());
            return false;
        }

        let mut settings = load_settings();
        let name_taken = settings
            .data_filters
            .iter()
            .any(|f| f.name == name && Some(&f.name) != ed.original_name.as_ref());
        if name_taken {
            ed.error = Some(format!("已存在同名筛选: {}", name));
            return false;
        }

        let existing = ed
            .original_name
            .as_ref()
            .and_then(|orig| settings.data_filters.iter_mut().find(|f| &f.name == orig));
        // Editing the conditions invalidates the cached match list: it was
        // produced by the *old* conditions, and leaving it on screen next to the
        // new ones is how you end up "explaining" a stock that never matched.
        let conditions_changed = match existing {
            Some(f) => {
                let changed = f.filters != lines;
                f.name = name.clone();
                f.filters = lines;
                changed
            }
            None => {
                settings.data_filters.push(NamedFilter {
                    name: name.clone(),
                    filters: lines,
                    periods: ed.periods.clone(),
                });
                true
            }
        };
        save_settings(&settings);
        // The conditions may have changed under the same bundle name.
        self.overlay_key = None;

        if let Some(orig) = &ed.original_name {
            if orig != &name {
                rename_filtered(orig, &name);
                self.rename_filter_state(orig, &name);
            }
            if conditions_changed {
                // A scan still under way was started with the old conditions:
                // dropping its channel discards what it will find.
                self.filter_receivers.remove(&name);
                self.filter_status.insert(name.clone(), FilterStatus::Stale);
            }
        }
        if conditions_changed {
            remove_filtered(&name);
            self.filtered.remove(&name);
        }

        // The saved bundle's tab opens.
        self.focus_filter(&name);
        self.stock_tab = StockTab::Filtered;
        self.restore_selection_for_current_tab();
        true
    }

    /// Carries what this session knows about a bundle over to its new name.
    fn rename_filter_state(&mut self, old: &str, new: &str) {
        if let Some(matches) = self.filtered.remove(old) {
            self.filtered.insert(new.to_string(), matches);
        }
        if let Some(status) = self.filter_status.remove(old) {
            self.filter_status.insert(new.to_string(), status);
        }
        if let Some(receiver) = self.filter_receivers.remove(old) {
            self.filter_receivers.insert(new.to_string(), receiver);
        }
        if let Some(secid) = self.last_selected_by_tab.remove(&filter_selection_key(old)) {
            self.last_selected_by_tab
                .insert(filter_selection_key(new), secid);
            save_last_selected(&self.last_selected_by_tab);
        }
        if self.selected_filter_name.as_deref() == Some(old) {
            self.selected_filter_name = Some(new.to_string());
        }
    }

    fn delete_filter_bundle(&mut self, name: &str) {
        let mut settings = load_settings();
        settings.data_filters.retain(|f| f.name != name);
        save_settings(&settings);
        remove_filtered(name);

        self.filtered.remove(name);
        self.filter_status.remove(name);
        self.filter_receivers.remove(name);
        if self
            .last_selected_by_tab
            .remove(&filter_selection_key(name))
            .is_some()
        {
            save_last_selected(&self.last_selected_by_tab);
        }

        // If it was in focus, the first bundle left takes over; with none
        // left, its tab gives way to 全部.
        if self.selected_filter_name.as_deref() == Some(name) {
            self.selected_filter_name = None;
            if let Some(first) = settings.data_filters.first() {
                self.focus_filter(&first.name);
            }
            if self.stock_tab == StockTab::Filtered {
                if self.selected_filter_name.is_none() {
                    self.stock_tab = StockTab::All;
                }
                self.restore_selection_for_current_tab();
            }
        }
    }

    fn refresh_stock_list(&mut self) {
        let data_dir = self.resolve_data_dir();
        let prev_secid = self.selected.map(|i| self.stocks[i].0.secid.clone());

        self.stocks = load_stock_list(&data_dir);

        if let Some(secid) = prev_secid {
            if let Some(idx) = self.stocks.iter().position(|s| s.0.secid == secid) {
                self.select_stock(idx);
            } else if !self.stocks.is_empty() {
                self.select_stock(0);
            } else {
                self.selected = None;
                self.candles.clear();
                self.ma_lines.clear();
            }
        } else if !self.stocks.is_empty() {
            self.select_stock(0);
        }
    }
}

impl eframe::App for ChartApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Poll trade-agent result
        if let Some(ref rx) = self.agent_receiver {
            if let Ok(result) = rx.try_recv() {
                match result {
                    Ok(r) => self.chart_state.agent_status = AgentStatus::Done(r),
                    Err(e) => self.chart_state.agent_status = AgentStatus::Error(e),
                }
                self.agent_receiver = None;
            }
        }

        // Poll sync progress
        let mut sync_done = false;
        if let Some(ref rx) = self.sync_receiver {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    SyncMessage::Progress {
                        pct,
                        completed,
                        total,
                        stock_label,
                    } => {
                        self.sync_status = SyncStatus::Running {
                            progress_pct: pct,
                            completed,
                            total,
                            current_stock: stock_label,
                        };
                    }
                    SyncMessage::Finished(result) => {
                        match result {
                            Ok((s, f)) => {
                                self.sync_status = SyncStatus::Done {
                                    success: s,
                                    failed: f,
                                };
                            }
                            Err(e) => {
                                self.sync_status = SyncStatus::Error(e);
                            }
                        }
                        sync_done = true;
                    }
                }
            }
        }
        if sync_done {
            self.sync_receiver = None;
            self.refresh_stock_list();
        }

        // Poll filter scans: every bundle reports through its own channel.
        let mut finished = Vec::new();
        for (name, rx) in &self.filter_receivers {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    FilterMessage::Progress {
                        completed,
                        total,
                        matched,
                        stock_label,
                    } => {
                        let progress_pct = if total > 0 {
                            (completed as f32 / total as f32) * 100.0
                        } else {
                            0.0
                        };
                        self.filter_status.insert(
                            name.clone(),
                            FilterStatus::Running {
                                progress_pct,
                                completed,
                                total,
                                matched,
                                current_stock: stock_label,
                            },
                        );
                    }
                    FilterMessage::Finished(result) => finished.push((name.clone(), result)),
                }
            }
        }
        for (name, result) in finished {
            self.filter_receivers.remove(&name);
            match result {
                Ok((matches, total)) => {
                    let matched = matches.len();
                    save_filtered(Some(&name), &matches);
                    self.filtered.insert(name.clone(), matches);
                    self.filter_status
                        .insert(name.clone(), FilterStatus::Done { matched, total });
                    // The list on screen changed under the selection.
                    if self.stock_tab == StockTab::Filtered
                        && self.selected_filter_name.as_deref() == Some(name.as_str())
                    {
                        self.restore_selection_for_current_tab();
                    }
                }
                Err(e) => {
                    self.filter_status.insert(name, FilterStatus::Error(e));
                }
            }
        }

        // Poll single-stock auto-sync
        if let Some(ref rx) = self.auto_sync_receiver {
            if let Ok((secid, result)) = rx.try_recv() {
                if let Err(e) = &result {
                    eprintln!("[auto-sync] {} failed: {}", secid, e);
                }
                self.auto_sync_secid = None;
                self.auto_sync_receiver = None;
                // Pull in fresh candles only if the user is still looking at
                // the stock we just synced.
                if result.is_ok() {
                    if let Some(idx) = self.selected {
                        if self.stocks[idx].0.secid == secid {
                            self.reload_candles();
                        }
                    }
                }
            }
        }

        // Kick off an auto-sync for whichever stock is currently on screen
        // (one-shot per secid per session).
        self.try_start_auto_sync(ctx);

        // ── Up/Down arrow key to navigate stocks ──
        let arrow = ctx.input(|i| {
            if i.key_pressed(egui::Key::ArrowDown) {
                Some(1i32)
            } else if i.key_pressed(egui::Key::ArrowUp) {
                Some(-1i32)
            } else {
                None
            }
        });
        if let Some(dir) = arrow {
            let visible = self.visible_stock_indices();
            if !visible.is_empty() {
                let cur_pos = self
                    .selected
                    .and_then(|sel| visible.iter().position(|&i| i == sel));
                let next_pos = match cur_pos {
                    Some(pos) => (pos as i32 + dir).clamp(0, visible.len() as i32 - 1) as usize,
                    None => 0,
                };
                let next_idx = visible[next_pos];
                if self.selected != Some(next_idx) {
                    self.select_stock(next_idx);
                    // Keep the selected row visible as arrow keys page through.
                    self.scroll_to_selected = true;
                }
            }
        }

        // ── Left panel: stock list ──
        egui::SidePanel::left("stock_panel")
            .default_width(230.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.add_space(4.0);

                // Tab bar: 全部 | 自选 | one tab per filter bundle | +
                // Bundles are read from settings on every frame so edits to
                // settings.json are reflected without a restart.
                let settings = load_settings();
                let mut open_tab = None;
                let tab_text = |label: &str, active: bool| {
                    let text = egui::RichText::new(label);
                    if active {
                        text.strong()
                    } else {
                        text.color(egui::Color32::from_rgb(0x78, 0x7b, 0x86))
                    }
                };
                ui.horizontal_wrapped(|ui| {
                    for tab in [StockTab::All, StockTab::Favorites] {
                        let active = self.stock_tab == tab;
                        if ui.selectable_label(active, tab_text(tab.label(), active)).clicked()
                            && !active
                        {
                            open_tab = Some((tab, None));
                        }
                    }
                    ui.separator();
                    for bundle in &settings.data_filters {
                        let name = &bundle.name;
                        let active = self.stock_tab == StockTab::Filtered
                            && self.selected_filter_name.as_ref() == Some(name);
                        // A scan under way shows on the tab itself; how
                        // many stocks it matched is a hover away.
                        let (label, hint) =
                            match (self.filter_status.get(name), self.filtered.get(name)) {
                                (Some(FilterStatus::Running { matched, .. }), _) => {
                                    (format!("{name}…"), format!("筛选中，已匹配 {matched} 只"))
                                }
                                (_, Some(matches)) => {
                                    (name.clone(), format!("{} 只股票符合条件", matches.len()))
                                }
                                (_, None) => (name.clone(), "尚未筛选".to_string()),
                            };
                        let tab = ui
                            .selectable_label(active, tab_text(&label, active))
                            .on_hover_text(hint);
                        if tab.clicked() && !active {
                            open_tab = Some((StockTab::Filtered, Some(name.clone())));
                        }
                    }
                    if ui
                        .selectable_label(false, tab_text("+", false))
                        .on_hover_text("新建筛选")
                        .clicked()
                    {
                        self.open_filter_editor(None);
                    }
                });
                // The tab of a bundle that is gone (deleted, or removed from
                // settings.json by hand) gives way to the first one left, or
                // to 全部.
                let open_bundle_exists = settings
                    .data_filters
                    .iter()
                    .any(|b| Some(&b.name) == self.selected_filter_name.as_ref());
                if open_tab.is_none() && self.stock_tab == StockTab::Filtered && !open_bundle_exists {
                    open_tab = Some(match settings.data_filters.first() {
                        Some(first) => (StockTab::Filtered, Some(first.name.clone())),
                        None => (StockTab::All, None),
                    });
                }
                if let Some((tab, bundle)) = open_tab {
                    self.stock_tab = tab;
                    if let Some(name) = bundle {
                        self.focus_filter(&name);
                    }
                    // Jump back to whichever stock the user was last viewing
                    // on this tab (or the first visible one).
                    self.restore_selection_for_current_tab();
                }
                ui.separator();

                ui.horizontal(|ui| {
                    ui.label("搜索");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.search)
                            .hint_text("代码/名称")
                            .desired_width(f32::INFINITY),
                    );
                });
                ui.separator();

                // Sync button and progress. A filter bundle's tab has its own
                // button for its own matches, so only the progress shows there.
                let is_syncing = matches!(self.sync_status, SyncStatus::Running { .. });
                if self.stock_tab != StockTab::Filtered {
                    ui.horizontal(|ui| {
                        let btn_text = if is_syncing {
                            "同步中..."
                        } else if self.stock_tab == StockTab::Favorites {
                            "同步自选"
                        } else {
                            "同步数据"
                        };
                        let btn = ui.add_enabled(
                            !is_syncing,
                            egui::Button::new(egui::RichText::new(btn_text).size(12.0)),
                        );
                        if btn.clicked() {
                            // On the 自选 tab only those stocks are synced.
                            let favorites = (self.stock_tab == StockTab::Favorites
                                && !self.favorites.is_empty())
                            .then(|| self.favorites.clone());
                            self.run_sync(ctx, favorites);
                        }
                        if is_syncing {
                            ui.spinner();
                        }
                    });
                }
                match &self.sync_status {
                    SyncStatus::Running {
                        progress_pct,
                        completed,
                        total,
                        current_stock,
                    } => {
                        let bar_pct = *progress_pct / 100.0;
                        // stock-dl may first spend a while building its stock list.
                        let text = if *total == 0 {
                            "准备中…".to_string()
                        } else {
                            format!("{completed}/{total} ({progress_pct:.1}%)")
                        };
                        ui.add(egui::ProgressBar::new(bar_pct).text(text));
                        if !current_stock.is_empty() {
                            ui.label(
                                egui::RichText::new(current_stock)
                                    .size(11.0)
                                    .color(egui::Color32::from_rgb(0x90, 0xCA, 0xF9)),
                            );
                        }
                    }
                    SyncStatus::Done { success, failed } => {
                        let color = if *failed == 0 {
                            egui::Color32::from_rgb(0x26, 0xa6, 0x9a)
                        } else {
                            egui::Color32::from_rgb(0xFF, 0x98, 0x00)
                        };
                        ui.label(
                            egui::RichText::new(format!(
                                "同步完成: 成功={success}, 失败={failed}"
                            ))
                            .size(11.0)
                            .color(color),
                        );
                    }
                    SyncStatus::Error(msg) => {
                        ui.label(
                            egui::RichText::new(format!("同步错误: {msg}"))
                                .size(11.0)
                                .color(egui::Color32::from_rgb(0xef, 0x53, 0x50)),
                        );
                    }
                    SyncStatus::Idle => {}
                }
                if self.stock_tab != StockTab::Filtered
                    || !matches!(self.sync_status, SyncStatus::Idle)
                {
                    ui.separator();
                }

                // One-shot: consume the scroll request here so every row
                // knows whether it should call `scroll_to_me` on itself.
                let scroll_to_selected = std::mem::take(&mut self.scroll_to_selected);

                let open_bundle = settings
                    .data_filters
                    .iter()
                    .filter(|_| self.stock_tab == StockTab::Filtered)
                    .find(|b| Some(&b.name) == self.selected_filter_name.as_ref());
                match open_bundle {
                    Some(bundle) => {
                        self.show_filter_tab(ctx, ui, bundle, is_syncing, scroll_to_selected);
                    }
                    None => self.show_stock_list(ui, scroll_to_selected),
                }
            });

        self.show_filter_editor(ctx);

        // ── Central panel: chart ──
        egui::CentralPanel::default().show(ctx, |ui| {
            // Period selector bar + MA toggles
            ui.horizontal(|ui| {
                if let Some(idx) = self.selected {
                    let info = &self.stocks[idx].0;
                    ui.label(
                        egui::RichText::new(format!("{} {}", info.secid, info.name))
                            .size(15.0)
                            .strong(),
                    );
                    if self.auto_sync_secid.as_deref() == Some(info.secid.as_str()) {
                        ui.spinner();
                        ui.label(
                            egui::RichText::new("同步中")
                                .size(10.0)
                                .color(egui::Color32::from_rgb(0x90, 0xCA, 0xF9)),
                        );
                    }
                }
                ui.separator();

                let mut changed = false;
                for p in [Period::Daily, Period::Weekly] {
                    let btn = ui.selectable_label(self.period == p, p.label());
                    if btn.clicked() && self.period != p {
                        self.period = p;
                        changed = true;
                    }
                }
                if changed {
                    self.reload_candles();
                }

                // Switch for the structure markers, offered when the stock
                // on screen has something to show.
                if !self.overlay.is_empty() {
                    ui.separator();
                    ui.toggle_value(&mut self.show_overlay, "走势标识")
                        .on_hover_text("显示 / 隐藏筛选命中的走势标注");
                }

                ui.separator();

                // MA toggle buttons
                for (i, ma) in self.ma_lines.iter().enumerate() {
                    if i >= self.ma_visible.len() {
                        break;
                    }
                    let color = ma_color(ma.period);
                    let visible = self.ma_visible[i];
                    let label = format!("MA{}", ma.period);

                    let text = if visible {
                        egui::RichText::new(&label).size(12.0).color(color)
                    } else {
                        egui::RichText::new(&label)
                            .size(12.0)
                            .color(egui::Color32::from_rgb(0x55, 0x55, 0x55))
                            .strikethrough()
                    };

                    if ui.selectable_label(visible, text).clicked() {
                        self.ma_visible[i] = !self.ma_visible[i];
                    }
                }

                ui.separator();

                ui.separator();

                ui.label(
                    egui::RichText::new("Model")
                        .size(12.0)
                        .color(egui::Color32::from_rgb(0x9b, 0x9e, 0xa8)),
                );
                egui::ComboBox::from_id_salt("model_select")
                    .selected_text(&self.model)
                    .width(140.0)
                    .show_ui(ui, |ui| {
                        for &m in MODEL_OPTIONS {
                            ui.selectable_value(&mut self.model, m.to_string(), m);
                        }
                    });

                ui.checkbox(
                    &mut self.csv_as_chart,
                    egui::RichText::new("asChart").size(12.0),
                );

                let has_selection =
                    matches!(self.chart_state.selection, SelectionState::Selected { .. });
                let review_btn = ui.add_enabled(
                    has_selection,
                    egui::Button::new(egui::RichText::new("Review").size(12.0)),
                );
                if review_btn.clicked() && has_selection {
                    self.chart_state.selection_just_completed = true;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(
                            "scroll:zoom | drag:pan | shift+drag:select | dblclick:reset",
                        )
                        .size(10.0)
                        .color(egui::Color32::from_rgb(0x78, 0x7b, 0x86)),
                    );
                });
            });
            ui.separator();

            self.refresh_overlay();
            let title = self
                .selected
                .map(|i| self.stocks[i].0.name.clone())
                .unwrap_or_default();
            // Weekly charts span a wider price range per bar, so space the
            // percentage guide lines further apart to keep the chart readable.
            let price_level_step = match self.period {
                Period::Daily => 0.2,
                Period::Weekly => 0.3,
            };
            let hidden = MatchedShapes::default();
            draw_chart(
                ui,
                &self.candles,
                &self.ma_lines,
                &self.ma_visible,
                &mut self.chart_state,
                &title,
                price_level_step,
                if self.show_overlay { &self.overlay } else { &hidden },
            );
        });

        // Trigger trade-agent when selection just completed
        if self.chart_state.selection_just_completed {
            self.chart_state.selection_just_completed = false;
            self.run_trade_agent(ctx);
        }
    }
}

/// Strip a `sh`/`sz`/`bj` market prefix so the remaining code matches what
/// `stock-dl --codes` expects.
fn strip_secid_prefix(secid: &str) -> &str {
    if secid.len() > 2 {
        let prefix = &secid[..2];
        if matches!(prefix, "sh" | "sz" | "bj") {
            return &secid[2..];
        }
    }
    secid
}

/// A pseudo-terminal to use as a child's stderr: our end to read, and the
/// child's end for `Command::stderr`. stock-dl draws its progress bar only
/// when stderr is a terminal; into a pipe it prints nothing until it is done.
#[cfg(unix)]
fn stderr_pty() -> Option<(std::fs::File, std::process::Stdio)> {
    use std::os::fd::{FromRawFd, OwnedFd};
    let (mut ours, mut theirs) = (-1, -1);
    // Wide, so the bar's stock name is not cut off.
    let mut size = libc::winsize {
        ws_row: 24,
        ws_col: 200,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let opened = unsafe {
        libc::openpty(
            &mut ours,
            &mut theirs,
            std::ptr::null_mut(),
            std::ptr::null_mut::<libc::termios>(),
            &mut size,
        )
    } == 0;
    if !opened {
        return None;
    }
    // Keep both ends out of any other process the app starts meanwhile: a
    // stray copy of the child's end would keep ours from ever seeing the end.
    unsafe {
        libc::fcntl(ours, libc::F_SETFD, libc::FD_CLOEXEC);
        libc::fcntl(theirs, libc::F_SETFD, libc::FD_CLOEXEC);
    }
    let ours = unsafe { std::fs::File::from_raw_fd(ours) };
    let theirs = unsafe { OwnedFd::from_raw_fd(theirs) };
    Some((ours, theirs.into()))
}

#[cfg(not(unix))]
fn stderr_pty() -> Option<(std::fs::File, std::process::Stdio)> {
    None
}

/// Reads stock-dl's stderr until it closes. Each redraw of its progress bar
/// becomes a `SyncMessage::Progress`; everything else is returned, as the
/// error message should stock-dl fail.
fn read_dl_stderr(
    mut stderr: impl std::io::Read,
    tx: &mpsc::Sender<SyncMessage>,
    ctx: &egui::Context,
) -> String {
    let mut other = Vec::new();
    let mut shown: Option<(usize, String)> = None;
    let mut handle = |segment: &[u8]| {
        let text = strip_ansi(&String::from_utf8_lossy(segment));
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let Some((pct, completed, total, stock_label)) = parse_dl_bar_line(text) else {
            other.push(text.to_string());
            return;
        };
        // The bar is redrawn on a timer too; only pass on what changed.
        if shown.as_ref() != Some(&(completed, stock_label.clone())) {
            shown = Some((completed, stock_label.clone()));
            let _ = tx.send(SyncMessage::Progress {
                pct,
                completed,
                total,
                stock_label,
            });
            ctx.request_repaint();
        }
    };

    // Bar redraws end in `\r`, ordinary lines in `\n` (`\r\n` on a terminal).
    // Split the bytes, not text: a read may end inside a character.
    let mut pending = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = match stderr.read(&mut buf) {
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            // A pseudo-terminal reports EIO once the child's end is closed.
            Err(_) => 0,
        };
        if n == 0 {
            break;
        }
        pending.extend_from_slice(&buf[..n]);
        while let Some(i) = pending.iter().position(|&b| b == b'\r' || b == b'\n') {
            let segment: Vec<u8> = pending.drain(..=i).collect();
            handle(&segment[..i]);
        }
    }
    handle(&pending);
    other.join("\n")
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    let font_paths = [
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/System/Library/Fonts/Supplemental/Songti.ttc",
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
    ];

    for path in &font_paths {
        if let Ok(data) = std::fs::read(path) {
            fonts
                .font_data
                .insert("cjk".to_owned(), egui::FontData::from_owned(data));
            if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                family.push("cjk".to_owned());
            }
            if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                family.push("cjk".to_owned());
            }
            break;
        }
    }

    ctx.set_fonts(fonts);
}

/// The configured stock data directory, with a leading `~/` expanded.
fn settings_data_dir(settings: &AppSettings) -> PathBuf {
    let dir = &settings.dl.data_dir;
    match dir.strip_prefix("~/") {
        Some(rest) => {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(format!("{}/{}", home, rest))
        }
        None => PathBuf::from(dir),
    }
}

/// `--filter NAME`: runs one bundle over every stock without opening a window
/// and prints the matches, one `secid name` per line. NAME is looked up among
/// the saved bundles first, then among the presets, so a preset can be run
/// before it has ever been saved. Nothing is written to disk.
fn run_filter_cli(name: &str, data_dir: Option<PathBuf>) -> Result<(), String> {
    let settings = load_settings();
    let presets = preset_filters();
    let bundle = settings
        .data_filters
        .iter()
        .chain(&presets)
        .find(|f| f.name == name)
        .ok_or_else(|| {
            let names: Vec<&str> = settings
                .data_filters
                .iter()
                .chain(&presets)
                .map(|f| f.name.as_str())
                .collect();
            format!("没有名为 {} 的筛选 (可用: {})", name, names.join(", "))
        })?;
    let lines = parse_filter_lines(&bundle.filters)?;
    if lines.is_empty() {
        return Err("该筛选没有任何条件".to_string());
    }

    let data_dir = data_dir.unwrap_or_else(|| settings_data_dir(&settings));
    let stocks = load_stock_list(&data_dir);
    if stocks.is_empty() {
        return Err(format!("{} 下没有股票数据", data_dir.display()));
    }
    let mut matched = 0;
    for (info, dir) in &stocks {
        if evaluate_filters_on_stock(dir, &lines, &settings.ma, &bundle.periods) {
            println!("{} {}", info.secid, info.name);
            matched += 1;
        }
    }
    eprintln!("{}: {} / {} 只入选", bundle.name, matched, stocks.len());
    Ok(())
}

fn main() -> eframe::Result {
    // chart-app [DATA_DIR]                 open the chart window
    // chart-app --filter NAME [DATA_DIR]   run one filter bundle and print the matches
    // DATA_DIR overrides settings.dl.data_dir.
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(pos) = args.iter().position(|a| a == "--filter") {
        args.remove(pos);
        if pos >= args.len() {
            eprintln!("用法: chart-app --filter <筛选名称> [数据目录]");
            std::process::exit(2);
        }
        let name = args.remove(pos);
        if let Err(e) = run_filter_cli(&name, args.first().map(PathBuf::from)) {
            eprintln!("{}", e);
            std::process::exit(1);
        }
        return Ok(());
    }
    let data_dir_override = args.first().map(PathBuf::from);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 850.0])
            .with_title("Stock K-Line Chart"),
        ..Default::default()
    };

    eframe::run_native(
        "Stock K-Line Chart",
        options,
        Box::new(move |cc| Ok(Box::new(ChartApp::new(cc, data_dir_override)))),
    )
}
