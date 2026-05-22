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
    filtered: Vec<String>,
    filter_status: FilterStatus,
    filter_receiver: Option<mpsc::Receiver<FilterMessage>>,
    /// Name of the currently picked filter bundle from `settings.data_filters`.
    /// Persisted across launches so the dropdown reopens to the same choice.
    selected_filter_name: Option<String>,
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
        let data_dir = data_dir.unwrap_or_else(|| {
            let dir = settings.dl.data_dir.clone();
            // Expand ~ to home directory
            if dir.starts_with("~/") {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                PathBuf::from(format!("{}/{}", home, &dir[2..]))
            } else {
                PathBuf::from(dir)
            }
        });

        let stocks = load_stock_list(&data_dir);
        let favorites = load_favorites();
        let last_selected_by_tab = load_last_selected();
        // Restore the previously chosen filter name, falling back to the first
        // available bundle so the dropdown always has a valid selection when
        // any filters are configured.
        let selected_filter_name = load_selected_filter()
            .filter(|n| settings.data_filters.iter().any(|f| &f.name == n))
            .or_else(|| settings.data_filters.first().map(|f| f.name.clone()));
        let filtered = load_filtered(selected_filter_name.as_deref());
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
            filter_status: FilterStatus::Idle,
            filter_receiver: None,
            selected_filter_name,
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

    fn reload_filtered_for_selected_filter(&mut self) {
        self.filtered = load_filtered(self.selected_filter_name.as_deref());
        if self.stock_tab == StockTab::Filtered {
            self.restore_selection_for_current_tab();
        }
    }

    fn select_stock(&mut self, idx: usize) {
        self.selected = Some(idx);
        // Remember per-tab so future tab switches / restarts can restore it.
        if let Some((info, _)) = self.stocks.get(idx) {
            self.last_selected_by_tab
                .insert(self.stock_tab.key().to_string(), info.secid.clone());
            save_last_selected(&self.last_selected_by_tab);
        }
        self.reload_candles();
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
            .get(self.stock_tab.key())
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

    /// Build the list of visible stock indices (matching current search + tab filters).
    fn visible_stock_indices(&self) -> Vec<usize> {
        let search_lower = self.search.to_lowercase();
        self.stocks
            .iter()
            .enumerate()
            .filter(|(_, (info, _))| {
                if !search_lower.is_empty()
                    && !info.code.to_lowercase().contains(&search_lower)
                    && !info.name.contains(&self.search)
                    && !info.secid.to_lowercase().contains(&search_lower)
                {
                    return false;
                }
                match self.stock_tab {
                    StockTab::Favorites => self.favorites.contains(&info.secid),
                    StockTab::Filtered => self.filtered.contains(&info.secid),
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

    fn run_sync(&mut self, ctx: &egui::Context) {
        let settings = load_settings();
        let ctx = ctx.clone();

        // When on Favorites/Filtered tab, only sync those stocks
        let symbols: Option<Vec<String>> = match self.stock_tab {
            StockTab::Favorites if !self.favorites.is_empty() => {
                Some(self.favorites.clone())
            }
            StockTab::Filtered if !self.filtered.is_empty() => {
                Some(self.filtered.clone())
            }
            _ => None,
        };

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
            cmd.stderr(std::process::Stdio::piped());

            eprintln!("[stock-dl] Running: {:?}", cmd);

            let mut child = match cmd.spawn() {
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

            // Read stdout line-by-line for progress
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
            let stderr_output = child
                .stderr
                .take()
                .map(|stderr| std::io::read_to_string(stderr).unwrap_or_default())
                .unwrap_or_default();

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
        let settings = load_settings();
        let dir = &settings.dl.data_dir;
        if dir.starts_with("~/") {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(format!("{}/{}", home, &dir[2..]))
        } else {
            PathBuf::from(dir)
        }
    }

    fn run_filter(&mut self, ctx: &egui::Context) {
        let settings = load_settings();

        // Resolve which named filter bundle to run. Falls back to the first
        // entry if the persisted selection no longer exists.
        let bundle = match self
            .selected_filter_name
            .as_ref()
            .and_then(|n| settings.data_filters.iter().find(|f| &f.name == n))
            .or_else(|| settings.data_filters.first())
        {
            Some(b) => b,
            None => {
                self.filter_status =
                    FilterStatus::Error("未配置任何筛选 (settings.json: data_filters)".to_string());
                return;
            }
        };

        let filter_name = bundle.name.clone();
        let filter_periods = bundle.periods.clone();

        let filters: Vec<DataFilter> = match bundle
            .filters
            .iter()
            .map(|s| parse_data_filter(s))
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(f) => f,
            Err(e) => {
                self.filter_status = FilterStatus::Error(format!("过滤条件解析失败: {}", e));
                return;
            }
        };

        let ma_windows = settings.ma.clone();
        let data_dir = self.resolve_data_dir();
        let ctx = ctx.clone();

        self.filter_status = FilterStatus::Running {
            progress_pct: 0.0,
            completed: 0,
            total: 0,
            matched: 0,
            current_stock: String::new(),
        };

        let (tx, rx) = mpsc::channel();
        self.filter_receiver = Some(rx);

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

            matched_secids.sort();
            save_filtered(Some(&filter_name), &matched_secids);

            let matched = matched_secids.len();
            let _ = tx.send(FilterMessage::Finished(Ok((matched, total))));
            ctx.request_repaint();
        });
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

        // Poll filter progress
        let mut filter_done = false;
        if let Some(ref rx) = self.filter_receiver {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    FilterMessage::Progress {
                        completed,
                        total,
                        matched,
                        stock_label,
                    } => {
                        self.filter_status = FilterStatus::Running {
                            progress_pct: if total > 0 {
                                (completed as f32 / total as f32) * 100.0
                            } else {
                                0.0
                            },
                            completed,
                            total,
                            matched,
                            current_stock: stock_label,
                        };
                    }
                    FilterMessage::Finished(result) => {
                        match result {
                            Ok((matched, total)) => {
                                self.filter_status =
                                    FilterStatus::Done { matched, total };
                                self.filtered = load_filtered(self.selected_filter_name.as_deref());
                            }
                            Err(e) => {
                                self.filter_status = FilterStatus::Error(e);
                            }
                        }
                        filter_done = true;
                    }
                }
            }
        }
        if filter_done {
            self.filter_receiver = None;
            if self.stock_tab == StockTab::Filtered {
                self.restore_selection_for_current_tab();
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
            .default_width(210.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.add_space(4.0);

                // Tab bar: 全部 | 自选 | 筛选
                ui.horizontal(|ui| {
                    for tab in [StockTab::All, StockTab::Favorites, StockTab::Filtered] {
                        let active = self.stock_tab == tab;
                        let text = if active {
                            egui::RichText::new(tab.label()).strong()
                        } else {
                            egui::RichText::new(tab.label())
                                .color(egui::Color32::from_rgb(0x78, 0x7b, 0x86))
                        };
                        if ui.selectable_label(active, text).clicked() && self.stock_tab != tab {
                            self.stock_tab = tab;
                            // Jump back to whichever stock the user was last
                            // viewing on this tab (or the first visible one).
                            self.restore_selection_for_current_tab();
                        }
                    }
                });
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

                // Sync button and progress
                let is_syncing = matches!(self.sync_status, SyncStatus::Running { .. });
                ui.horizontal(|ui| {
                    let btn_text = if is_syncing {
                        "同步中..."
                    } else {
                        match self.stock_tab {
                            StockTab::Favorites => "同步自选",
                            StockTab::Filtered => "同步筛选",
                            StockTab::All => "同步数据",
                        }
                    };
                    let btn = ui.add_enabled(
                        !is_syncing,
                        egui::Button::new(egui::RichText::new(btn_text).size(12.0)),
                    );
                    if btn.clicked() {
                        self.run_sync(ctx);
                    }
                    if is_syncing {
                        ui.spinner();
                    }
                });
                match &self.sync_status {
                    SyncStatus::Running {
                        progress_pct,
                        completed,
                        total,
                        current_stock,
                    } => {
                        let bar_pct = *progress_pct / 100.0;
                        ui.add(
                            egui::ProgressBar::new(bar_pct)
                                .text(format!("{completed}/{total} ({progress_pct:.1}%)")),
                        );
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
                ui.separator();

                // Filter button and progress (only on Filtered tab)
                if self.stock_tab == StockTab::Filtered {
                    let is_filtering =
                        matches!(self.filter_status, FilterStatus::Running { .. });

                    // Filter-bundle picker. Reads names from settings on every
                    // frame so edits to settings.json are reflected without a
                    // restart.
                    let available_filters: Vec<String> = load_settings()
                        .data_filters
                        .iter()
                        .map(|f| f.name.clone())
                        .collect();
                    if !available_filters.is_empty() {
                        // Repair stale selection (e.g. user removed the active
                        // bundle from settings).
                        if !self
                            .selected_filter_name
                            .as_ref()
                            .map(|n| available_filters.contains(n))
                            .unwrap_or(false)
                        {
                            self.selected_filter_name = Some(available_filters[0].clone());
                            if let Some(name) = &self.selected_filter_name {
                                save_selected_filter(name);
                            }
                        }
                        let prev_filter = self.selected_filter_name.clone();
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("筛选:").size(12.0));
                            let current = self
                                .selected_filter_name
                                .clone()
                                .unwrap_or_else(|| available_filters[0].clone());
                            ui.add_enabled_ui(!is_filtering, |ui| {
                                egui::ComboBox::from_id_salt("filter-bundle-select")
                                    .selected_text(egui::RichText::new(&current).size(12.0))
                                    .show_ui(ui, |ui| {
                                        for name in &available_filters {
                                            ui.selectable_value(
                                                &mut self.selected_filter_name,
                                                Some(name.clone()),
                                                name.as_str(),
                                            );
                                        }
                                    });
                            });
                            // Show which timeframe(s) the active filter runs on.
                            let period_label = {
                                let settings = load_settings();
                                settings
                                    .data_filters
                                    .iter()
                                    .find(|f| Some(&f.name) == self.selected_filter_name.as_ref())
                                    .map(|f| {
                                        let has_daily = f.periods.contains(&Period::Daily) || f.periods.is_empty();
                                        let has_weekly = f.periods.contains(&Period::Weekly);
                                        match (has_daily, has_weekly) {
                                            (true, true) => "日+周",
                                            (false, true) => "周",
                                            _ => "日",
                                        }
                                    })
                                    .unwrap_or("日")
                            };
                            ui.label(
                                egui::RichText::new(format!("[{period_label}]"))
                                    .size(11.0)
                                    .color(egui::Color32::from_rgb(0x90, 0xCA, 0xF9)),
                            );
                        });
                        if self.selected_filter_name != prev_filter {
                            if let Some(name) = &self.selected_filter_name {
                                save_selected_filter(name);
                            }
                            self.reload_filtered_for_selected_filter();
                        }
                    }

                    ui.horizontal(|ui| {
                        let btn_text = if is_filtering {
                            "筛选中..."
                        } else {
                            "重新筛选"
                        };
                        let btn = ui.add_enabled(
                            !is_filtering,
                            egui::Button::new(
                                egui::RichText::new(btn_text).size(12.0),
                            ),
                        );
                        if btn.clicked() {
                            self.run_filter(ctx);
                        }
                        if is_filtering {
                            ui.spinner();
                        }
                    });
                    match &self.filter_status {
                        FilterStatus::Running {
                            progress_pct,
                            completed,
                            total,
                            matched,
                            current_stock,
                        } => {
                            let bar_pct = *progress_pct / 100.0;
                            ui.add(
                                egui::ProgressBar::new(bar_pct).text(format!(
                                    "{completed}/{total} 匹配:{matched} ({progress_pct:.1}%)"
                                )),
                            );
                            if !current_stock.is_empty() {
                                ui.label(
                                    egui::RichText::new(current_stock)
                                        .size(11.0)
                                        .color(egui::Color32::from_rgb(
                                            0x90, 0xCA, 0xF9,
                                        )),
                                );
                            }
                        }
                        FilterStatus::Done { matched, total } => {
                            ui.label(
                                egui::RichText::new(format!(
                                    "筛选完成: {matched}/{total} 只股票符合条件"
                                ))
                                .size(11.0)
                                .color(egui::Color32::from_rgb(0x26, 0xa6, 0x9a)),
                            );
                        }
                        FilterStatus::Error(msg) => {
                            ui.label(
                                egui::RichText::new(format!("筛选错误: {msg}"))
                                    .size(11.0)
                                    .color(egui::Color32::from_rgb(0xef, 0x53, 0x50)),
                            );
                        }
                        FilterStatus::Idle => {}
                    }
                    ui.separator();
                }

                // One-shot: consume the scroll request here so every row
                // knows whether it should call `scroll_to_me` on itself.
                let scroll_to_selected = std::mem::take(&mut self.scroll_to_selected);

                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    let search_lower = self.search.to_lowercase();
                    let mut click_idx = None;
                    let mut fav_toggle = None;

                    for (i, (info, _)) in self.stocks.iter().enumerate() {
                        // Search filter
                        if !search_lower.is_empty()
                            && !info.code.to_lowercase().contains(&search_lower)
                            && !info.name.contains(&self.search)
                            && !info.secid.to_lowercase().contains(&search_lower)
                        {
                            continue;
                        }

                        // Tab filter
                        let is_fav = self.favorites.contains(&info.secid);
                        match self.stock_tab {
                            StockTab::Favorites => {
                                if !is_fav {
                                    continue;
                                }
                            }
                            StockTab::Filtered => {
                                if !self.filtered.contains(&info.secid) {
                                    continue;
                                }
                            }
                            StockTab::All => {}
                        }

                        let selected = self.selected == Some(i);
                        let star = if is_fav { "★" } else { "☆" };
                        let star_color = if is_fav {
                            egui::Color32::from_rgb(0xFF, 0xD7, 0x00)
                        } else {
                            egui::Color32::from_rgb(0x55, 0x55, 0x55)
                        };

                        ui.horizontal(|ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new(star).color(star_color).size(14.0),
                                    )
                                    .frame(false),
                                )
                                .clicked()
                            {
                                fav_toggle = Some((info.secid.clone(), is_fav));
                            }
                            let label = format!("{} {}", info.secid, info.name);
                            let resp = ui.selectable_label(selected, &label);
                            if resp.clicked() {
                                click_idx = Some(i);
                            }
                            if selected && scroll_to_selected {
                                resp.scroll_to_me(Some(egui::Align::Center));
                            }
                        });
                    }

                    if let Some(i) = click_idx {
                        self.select_stock(i);
                    }

                    if let Some((secid, was_fav)) = fav_toggle {
                        if was_fav {
                            self.favorites.retain(|s| s != &secid);
                        } else {
                            self.favorites.push(secid);
                            self.favorites.sort();
                        }
                        save_favorites(&self.favorites);
                    }
                });
            });

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
            draw_chart(
                ui,
                &self.candles,
                &self.ma_lines,
                &self.ma_visible,
                &mut self.chart_state,
                &title,
                price_level_step,
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

fn main() -> eframe::Result {
    // CLI arg overrides settings.dl.data_dir
    let data_dir_override = std::env::args().nth(1).map(PathBuf::from);

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
