use crate::chip::{calculate_chip_distribution, ChipCache};
use crate::data::{
    calculate_ma_cluster_score, Candle, ChipSettings, MaClusterSettings, MaLine, TradeAgentResult,
};
use eframe::egui::*;

// TradingView dark theme colors
const BG_COLOR: Color32 = Color32::from_rgb(0x13, 0x17, 0x22);
const GRID_COLOR: Color32 = Color32::from_rgb(0x2a, 0x2e, 0x39);
const BORDER_COLOR: Color32 = Color32::from_rgb(0x36, 0x3a, 0x45);
const TEXT_COLOR: Color32 = Color32::from_rgb(0xd1, 0xd4, 0xdc);
const TEXT_DIM: Color32 = Color32::from_rgb(0x78, 0x7b, 0x86);
const UP_COLOR: Color32 = Color32::from_rgb(0x26, 0xa6, 0x9a);
const DOWN_COLOR: Color32 = Color32::from_rgb(0xef, 0x53, 0x50);
const CROSSHAIR_COLOR: Color32 = Color32::from_rgb(0x9b, 0x9e, 0xa8);
const LABEL_BG: Color32 = Color32::from_rgb(0x36, 0x3a, 0x45);

const PRICE_AXIS_WIDTH: f32 = 85.0;
const TIME_AXIS_HEIGHT: f32 = 28.0;
const VOLUME_RATIO: f32 = 0.18;
const CHIP_PANEL_WIDTH: f32 = 120.0;
const MA_CLUSTER_PANEL_HEIGHT: f32 = 80.0;

#[derive(Clone, Copy, PartialEq)]
enum DragZone {
    None,
    Chart,
    PriceAxis,
    TimeAxis,
    Selection,
}

#[derive(Clone, PartialEq)]
pub enum SelectionState {
    Idle,
    Selecting { anchor: usize, current: usize },
    Selected { start: usize, end: usize },
}

pub enum AgentStatus {
    Idle,
    Running,
    Done(TradeAgentResult),
    Error(String),
}

pub struct ChartState {
    pub offset: f64,
    pub candles_in_view: f64,
    pub price_min: f64,
    pub price_max: f64,
    pub auto_price: bool,
    drag_zone: DragZone,
    pub selection: SelectionState,
    pub agent_status: AgentStatus,
    pub selection_just_completed: bool,
    pub show_chip_distribution: bool,
    pub show_ma_cluster_panel: bool,
    pub chip_cache: Option<ChipCache>,
    pub chip_settings: ChipSettings,
    pub ma_cluster_settings: MaClusterSettings,
}

impl ChartState {
    pub fn new(chip_settings: ChipSettings, ma_cluster_settings: MaClusterSettings) -> Self {
        Self {
            offset: 0.0,
            candles_in_view: 120.0,
            price_min: 0.0,
            price_max: 100.0,
            auto_price: true,
            drag_zone: DragZone::None,
            selection: SelectionState::Idle,
            agent_status: AgentStatus::Idle,
            selection_just_completed: false,
            show_chip_distribution: false,
            show_ma_cluster_panel: false,
            chip_cache: None,
            chip_settings,
            ma_cluster_settings,
        }
    }

    pub fn fit_to_data(&mut self, candles: &[Candle]) {
        if candles.is_empty() {
            return;
        }
        let n = candles.len() as f64;
        self.candles_in_view = n.min(120.0);
        self.offset = (n - self.candles_in_view).max(0.0);
        self.auto_price = true;
        self.selection = SelectionState::Idle;
        self.agent_status = AgentStatus::Idle;
        self.chip_cache = None;
    }
}

/// Return TradingView-style color for a given MA period
pub fn ma_color(period: usize) -> Color32 {
    match period {
        5 => Color32::from_rgb(0xFF, 0xEB, 0x3B),         // Yellow
        10 => Color32::from_rgb(0x29, 0x62, 0xFF),        // Blue
        20 => Color32::from_rgb(0xFF, 0x98, 0x00),        // Orange
        30 => Color32::from_rgb(0xE9, 0x1E, 0x63),        // Pink
        60 => Color32::from_rgb(0xAB, 0x47, 0xBC),        // Purple
        120 => Color32::from_rgb(0x00, 0xBC, 0xD4),       // Cyan
        240 | 250 => Color32::from_rgb(0x4C, 0xAF, 0x50), // Green
        _ => Color32::from_rgb(0xBB, 0xBB, 0xBB),         // Gray fallback
    }
}

/// `price_level_step` is the fractional gap between consecutive horizontal
/// price guide lines — e.g. 0.2 for 20 %-per-line on daily charts, 0.3 on
/// weekly charts.
pub fn draw_chart(
    ui: &mut Ui,
    candles: &[Candle],
    ma_lines: &[MaLine],
    ma_visible: &[bool],
    state: &mut ChartState,
    title: &str,
    price_level_step: f64,
) {
    if candles.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(RichText::new("暂无数据").size(20.0).color(TEXT_DIM));
        });
        return;
    }

    let available = ui.available_size();
    let (response, painter) =
        ui.allocate_painter(available, Sense::click_and_drag() | Sense::hover());

    let full_rect = response.rect;
    let chip_w = if state.show_chip_distribution {
        CHIP_PANEL_WIDTH
    } else {
        0.0
    };
    let cluster_h = if state.show_ma_cluster_panel {
        MA_CLUSTER_PANEL_HEIGHT
    } else {
        0.0
    };

    // Layout regions. When the cluster subplot is visible, the main chart
    // shrinks vertically to make room for it above the time axis.
    let main_area_bottom = full_rect.max.y - TIME_AXIS_HEIGHT;
    let chart_rect = Rect::from_min_max(
        full_rect.min,
        pos2(
            full_rect.max.x - PRICE_AXIS_WIDTH - chip_w,
            main_area_bottom - cluster_h,
        ),
    );
    let cluster_panel_rect = if cluster_h > 0.0 {
        Some(Rect::from_min_max(
            pos2(chart_rect.min.x, chart_rect.max.y),
            pos2(chart_rect.max.x, main_area_bottom),
        ))
    } else {
        None
    };
    let chip_rect = if state.show_chip_distribution {
        Some(Rect::from_min_max(
            pos2(chart_rect.max.x, chart_rect.min.y),
            pos2(chart_rect.max.x + chip_w, chart_rect.max.y),
        ))
    } else {
        None
    };
    let price_axis_rect = Rect::from_min_max(
        pos2(chart_rect.max.x + chip_w, chart_rect.min.y),
        pos2(full_rect.max.x, chart_rect.max.y),
    );
    let time_axis_rect = Rect::from_min_max(
        pos2(chart_rect.min.x, main_area_bottom),
        pos2(chart_rect.max.x, full_rect.max.y),
    );

    // Handle input
    handle_input(
        &response,
        ui,
        state,
        &chart_rect,
        &price_axis_rect,
        &time_axis_rect,
        candles,
    );

    // Auto-fit price
    if state.auto_price {
        auto_fit_price(state, candles, ma_lines, ma_visible);
    }

    // Background
    painter.rect_filled(full_rect, 0.0, BG_COLOR);

    // Clip to chart area for candle/volume/ma drawing
    let clipped = painter.with_clip_rect(chart_rect);

    // Grid
    draw_grid(&clipped, &chart_rect, state, candles);

    // Percentage price levels anchored to the global lowest low.
    draw_price_levels(&clipped, candles, state, &chart_rect, price_level_step);

    // Volume (behind candles)
    draw_volume(&clipped, candles, state, &chart_rect);

    // Selection overlay (behind candles)
    draw_selection(&clipped, state, &chart_rect);

    // MA lines (behind candles, above volume)
    draw_ma_lines(&clipped, candles, ma_lines, ma_visible, state, &chart_rect);

    // Candles
    draw_candles(&clipped, candles, state, &chart_rect);

    // Current price line
    draw_current_price_line(&clipped, candles, state, &chart_rect);

    // Chip distribution
    if let Some(cr) = chip_rect {
        draw_chip_distribution(&painter, candles, state, &cr);
    }

    // Axes (outside clip)
    draw_price_axis(&painter, &price_axis_rect, state);
    draw_time_axis(&painter, &time_axis_rect, candles, state);

    // Crosshair & OHLCV tooltip
    let hovered_idx = if let Some(pos) = response.hover_pos() {
        if chart_rect.contains(pos) {
            draw_crosshair(
                &painter,
                pos,
                &chart_rect,
                &price_axis_rect,
                &time_axis_rect,
                candles,
                state,
            );
            let idx = x_to_idx(pos.x, state, &chart_rect).round() as usize;
            if idx < candles.len() {
                Some(idx)
            } else {
                None
            }
        } else if let Some(cp) = cluster_panel_rect {
            if cp.contains(pos) {
                let idx = x_to_idx(pos.x, state, &cp).round() as usize;
                if idx < candles.len() {
                    Some(idx)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    // MA cluster subplot
    if let Some(cp) = cluster_panel_rect {
        draw_ma_cluster_panel(
            &painter,
            ma_lines,
            state,
            &cp,
            &state.ma_cluster_settings,
            candles.len(),
            hovered_idx,
        );
    }

    // Border between chart and axes (extends through the cluster panel when
    // the subplot is visible).
    let right_border_bottom = pos2(chart_rect.max.x, main_area_bottom);
    painter.line_segment(
        [chart_rect.right_top(), right_border_bottom],
        Stroke::new(1.0, BORDER_COLOR),
    );
    painter.line_segment(
        [
            pos2(chart_rect.min.x, main_area_bottom),
            right_border_bottom,
        ],
        Stroke::new(1.0, BORDER_COLOR),
    );
    if let Some(cp) = cluster_panel_rect {
        // Separator between main chart and cluster subplot.
        painter.line_segment(
            [pos2(cp.min.x, cp.min.y), pos2(cp.max.x, cp.min.y)],
            Stroke::new(1.0, BORDER_COLOR),
        );
    }

    // Title (top-left)
    painter.text(
        pos2(chart_rect.min.x + 10.0, chart_rect.min.y + 8.0),
        Align2::LEFT_TOP,
        title,
        FontId::proportional(15.0),
        TEXT_COLOR,
    );

    // OHLCV info for hovered candle
    if let Some(idx) = hovered_idx {
        draw_ohlcv_label(
            &painter,
            &candles[idx],
            chart_rect.min.x + 10.0,
            chart_rect.min.y + 28.0,
        );
        // MA values for hovered candle
        draw_ma_legend(
            &painter,
            ma_lines,
            ma_visible,
            idx,
            chart_rect.min.x + 10.0,
            chart_rect.min.y + 46.0,
        );
        draw_ma_cluster_legend(
            &painter,
            ma_lines,
            idx,
            &state.ma_cluster_settings,
            chart_rect.min.x + 10.0,
            chart_rect.min.y + 62.0,
        );
    } else {
        // Show MA legend for last visible candle
        let last_vis = ((state.offset + state.candles_in_view).ceil() as usize)
            .min(candles.len())
            .saturating_sub(1);
        draw_ma_legend(
            &painter,
            ma_lines,
            ma_visible,
            last_vis,
            chart_rect.min.x + 10.0,
            chart_rect.min.y + 28.0,
        );
        draw_ma_cluster_legend(
            &painter,
            ma_lines,
            last_vis,
            &state.ma_cluster_settings,
            chart_rect.min.x + 10.0,
            chart_rect.min.y + 44.0,
        );
    }

    // Agent result overlay (above everything)
    draw_agent_result(&painter, state, &chart_rect);
}

// ─── Input Handling ───────────────────────────────────────────

fn zoom_time(state: &mut ChartState, rect: &Rect, pos: Pos2, n: f64, factor: f64) {
    let cursor_frac = (pos.x - rect.min.x) as f64 / rect.width() as f64;
    let cursor_idx = state.offset + cursor_frac * state.candles_in_view;
    let new_count = (state.candles_in_view * factor).clamp(10.0, n.max(10.0));
    state.candles_in_view = new_count;
    state.offset = cursor_idx - cursor_frac * new_count;
    state.offset = state.offset.clamp(-new_count * 0.5, n - 1.0);
}

fn zoom_price(state: &mut ChartState, rect: &Rect, pos: Pos2, factor: f64) {
    let cursor_frac = (pos.y - rect.min.y) as f64 / rect.height() as f64;
    let cursor_price = state.price_max - cursor_frac * (state.price_max - state.price_min);
    let new_range = (state.price_max - state.price_min) * factor;
    state.price_max = cursor_price + cursor_frac * new_range;
    state.price_min = state.price_max - new_range;
    state.auto_price = false;
}

fn handle_input(
    response: &Response,
    ui: &Ui,
    state: &mut ChartState,
    chart_rect: &Rect,
    price_axis_rect: &Rect,
    time_axis_rect: &Rect,
    candles: &[Candle],
) {
    let n = candles.len() as f64;
    if n == 0.0 {
        return;
    }

    let hover_pos = response.hover_pos();
    let in_chart = hover_pos.map_or(false, |p| chart_rect.contains(p));
    let in_price_axis = hover_pos.map_or(false, |p| price_axis_rect.contains(p));
    let in_time_axis = hover_pos.map_or(false, |p| time_axis_rect.contains(p));

    // Keep repainting while hovered so we catch all scroll/zoom events
    if in_chart || in_price_axis || in_time_axis {
        ui.ctx().request_repaint();
    }

    // ── Scroll / pinch zoom (chart area) ──
    if in_chart {
        let pos = hover_pos.unwrap();
        let cmd = ui.input(|i| i.modifiers.command || i.modifiers.ctrl);

        // Trackpad pinch-to-zoom
        let pinch = ui.input(|i| i.zoom_delta());
        if (pinch - 1.0).abs() > 0.001 {
            let factor = 1.0 / pinch as f64;
            if cmd {
                zoom_price(state, chart_rect, pos, factor);
            } else {
                zoom_time(state, chart_rect, pos, n, factor);
            }
        }

        // Mouse wheel / trackpad scroll → zoom
        let scroll_y = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll_y.abs() > 0.5 {
            let factor = (1.0 - scroll_y as f64 * 0.003).clamp(0.75, 1.35);
            if cmd {
                zoom_price(state, chart_rect, pos, factor);
            } else {
                zoom_time(state, chart_rect, pos, n, factor);
            }
        }

        // Horizontal scroll → pan time
        let scroll_x = ui.input(|i| i.smooth_scroll_delta.x);
        if scroll_x.abs() > 0.5 {
            let d = (scroll_x as f64 / chart_rect.width() as f64) * state.candles_in_view;
            state.offset -= d;
            state.offset = state.offset.clamp(-state.candles_in_view * 0.5, n - 1.0);
        }

        // Keyboard +/- to zoom time
        let (plus, minus) = ui.input(|i| {
            let p = i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals);
            let m = i.key_pressed(Key::Minus);
            (p, m)
        });
        if plus {
            let center = chart_rect.center();
            zoom_time(state, chart_rect, center, n, 0.8);
        }
        if minus {
            let center = chart_rect.center();
            zoom_time(state, chart_rect, center, n, 1.25);
        }
    }

    // ── Scroll on price axis → zoom price ──
    if in_price_axis {
        let scroll_y = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll_y.abs() > 0.5 {
            let factor = (1.0 - scroll_y as f64 * 0.003).clamp(0.75, 1.35);
            let center = chart_rect.center();
            zoom_price(state, chart_rect, center, factor);
        }
    }

    // ── Scroll on time axis → zoom time ──
    if in_time_axis {
        let scroll_y = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll_y.abs() > 0.5 {
            let factor = (1.0 - scroll_y as f64 * 0.003).clamp(0.75, 1.35);
            let center = chart_rect.center();
            zoom_time(state, chart_rect, center, n, factor);
        }
    }

    // ── Escape key → clear selection ──
    if ui.input(|i| i.key_pressed(Key::Escape)) {
        state.selection = SelectionState::Idle;
        state.agent_status = AgentStatus::Idle;
    }

    // ── Drag handling: determine zone on drag start ──
    if response.drag_started() {
        if let Some(pos) = response.interact_pointer_pos() {
            let shift = ui.input(|i| i.modifiers.shift);
            if shift && chart_rect.contains(pos) {
                state.drag_zone = DragZone::Selection;
                let idx = x_to_idx(pos.x, state, chart_rect)
                    .round()
                    .clamp(0.0, n - 1.0) as usize;
                state.selection = SelectionState::Selecting {
                    anchor: idx,
                    current: idx,
                };
                state.agent_status = AgentStatus::Idle;
            } else if price_axis_rect.contains(pos) {
                state.drag_zone = DragZone::PriceAxis;
            } else if time_axis_rect.contains(pos) {
                state.drag_zone = DragZone::TimeAxis;
            } else {
                state.drag_zone = DragZone::Chart;
            }
        }
    }

    if response.dragged() {
        let delta = response.drag_delta();
        match state.drag_zone {
            DragZone::Selection => {
                if let Some(pos) = response.interact_pointer_pos() {
                    let idx = x_to_idx(pos.x, state, chart_rect)
                        .round()
                        .clamp(0.0, n - 1.0) as usize;
                    if let SelectionState::Selecting { anchor, .. } = state.selection {
                        state.selection = SelectionState::Selecting {
                            anchor,
                            current: idx,
                        };
                    }
                }
            }
            DragZone::PriceAxis => {
                // Drag up → zoom in (smaller range), drag down → zoom out
                let factor = (1.0 + delta.y as f64 * 0.005).clamp(0.92, 1.08);
                let center = chart_rect.center();
                zoom_price(state, chart_rect, center, factor);
            }
            DragZone::TimeAxis => {
                // Drag left → zoom in (fewer candles), drag right → zoom out
                let factor = (1.0 + delta.x as f64 * 0.005).clamp(0.92, 1.08);
                let center = chart_rect.center();
                zoom_time(state, chart_rect, center, n, factor);
            }
            DragZone::Chart => {
                // Horizontal → pan time
                let d_idx = -(delta.x as f64 / chart_rect.width() as f64) * state.candles_in_view;
                state.offset += d_idx;
                state.offset = state.offset.clamp(-state.candles_in_view * 0.5, n - 1.0);
                // Vertical → pan price (keep zoomed range, shift up/down)
                if !state.auto_price {
                    let price_range = state.price_max - state.price_min;
                    let price_per_px = price_range / chart_rect.height() as f64;
                    let shift = delta.y as f64 * price_per_px;
                    state.price_max += shift;
                    state.price_min += shift;
                }
            }
            DragZone::None => {}
        }
    }

    if response.drag_stopped() {
        if state.drag_zone == DragZone::Selection {
            if let SelectionState::Selecting { anchor, current } = state.selection {
                let start = anchor.min(current);
                let end = anchor.max(current);
                if end > start {
                    state.selection = SelectionState::Selected { start, end };
                } else {
                    state.selection = SelectionState::Idle;
                }
            }
        }
        state.drag_zone = DragZone::None;
    }

    // Double-click → reset
    if response.double_clicked() {
        state.fit_to_data(candles);
    }
}

// ─── Price auto-fit ───────────────────────────────────────────

fn auto_fit_price(
    state: &mut ChartState,
    candles: &[Candle],
    ma_lines: &[MaLine],
    ma_visible: &[bool],
) {
    let start = (state.offset.floor() as usize).min(candles.len().saturating_sub(1));
    let end = ((state.offset + state.candles_in_view).ceil() as usize).min(candles.len());
    if start >= end {
        return;
    }

    let mut lo = f64::MAX;
    let mut hi = f64::MIN;
    for c in &candles[start..end] {
        lo = lo.min(c.low);
        hi = hi.max(c.high);
    }

    // Include visible MA values in price range
    for (i, ma) in ma_lines.iter().enumerate() {
        if i >= ma_visible.len() || !ma_visible[i] {
            continue;
        }
        for j in start..end.min(ma.values.len()) {
            let v = ma.values[j];
            if !v.is_nan() {
                lo = lo.min(v);
                hi = hi.max(v);
            }
        }
    }

    let pad = (hi - lo) * 0.08;
    state.price_min = lo - pad;
    state.price_max = hi + pad;
}

// ─── Coordinate mapping ──────────────────────────────────────

fn idx_to_x(idx: f64, state: &ChartState, rect: &Rect) -> f32 {
    let frac = (idx - state.offset) / state.candles_in_view;
    rect.min.x + frac as f32 * rect.width()
}

fn price_to_y(price: f64, state: &ChartState, rect: &Rect) -> f32 {
    let range = state.price_max - state.price_min;
    if range.abs() < 1e-12 {
        return rect.center().y;
    }
    let frac = (state.price_max - price) / range;
    rect.min.y + frac as f32 * rect.height()
}

fn x_to_idx(x: f32, state: &ChartState, rect: &Rect) -> f64 {
    let frac = (x - rect.min.x) as f64 / rect.width() as f64;
    state.offset + frac * state.candles_in_view
}

fn y_to_price(y: f32, state: &ChartState, rect: &Rect) -> f64 {
    let frac = (y - rect.min.y) as f64 / rect.height() as f64;
    state.price_max - frac * (state.price_max - state.price_min)
}

// ─── Drawing ─────────────────────────────────────────────────

fn visible_range(state: &ChartState, n: usize) -> (usize, usize) {
    let start = (state.offset.floor() as i64).max(0) as usize;
    let end = ((state.offset + state.candles_in_view).ceil() as usize + 1).min(n);
    (start, end)
}

fn draw_candles(painter: &Painter, candles: &[Candle], state: &ChartState, rect: &Rect) {
    let candle_px = rect.width() as f64 / state.candles_in_view;
    let body_w = (candle_px * 0.7).max(1.0) as f32;
    let (start, end) = visible_range(state, candles.len());

    for i in start..end {
        let c = &candles[i];
        let cx = idx_to_x(i as f64 + 0.5, state, rect);
        let is_up = c.close >= c.open;
        let color = if is_up { UP_COLOR } else { DOWN_COLOR };

        // Wick
        let wick_top = price_to_y(c.high, state, rect);
        let wick_bot = price_to_y(c.low, state, rect);
        let wick_w = if body_w > 4.0 { 1.5 } else { 1.0 };
        painter.line_segment(
            [pos2(cx, wick_top), pos2(cx, wick_bot)],
            Stroke::new(wick_w, color),
        );

        // Body
        let body_top = price_to_y(c.open.max(c.close), state, rect);
        let body_bot = price_to_y(c.open.min(c.close), state, rect);
        let bh = (body_bot - body_top).max(1.0);
        let body_rect = Rect::from_min_size(pos2(cx - body_w / 2.0, body_top), vec2(body_w, bh));
        painter.rect_filled(body_rect, 0.0, color);
    }
}

fn draw_ma_lines(
    painter: &Painter,
    candles: &[Candle],
    ma_lines: &[MaLine],
    ma_visible: &[bool],
    state: &ChartState,
    rect: &Rect,
) {
    let (start, end) = visible_range(state, candles.len());

    for (i, ma) in ma_lines.iter().enumerate() {
        if i >= ma_visible.len() || !ma_visible[i] {
            continue;
        }

        let color = ma_color(ma.period);
        let stroke = Stroke::new(1.5, color);

        // Collect segments (break at NaN gaps)
        let mut segment: Vec<Pos2> = Vec::new();

        for j in start..end.min(ma.values.len()) {
            let v = ma.values[j];
            if v.is_nan() {
                // Flush current segment
                if segment.len() >= 2 {
                    painter.add(Shape::line(segment.clone(), stroke));
                }
                segment.clear();
            } else {
                let x = idx_to_x(j as f64 + 0.5, state, rect);
                let y = price_to_y(v, state, rect);
                segment.push(pos2(x, y));
            }
        }

        // Flush remaining
        if segment.len() >= 2 {
            painter.add(Shape::line(segment, stroke));
        }
    }
}

fn draw_ma_cluster_legend(
    painter: &Painter,
    ma_lines: &[MaLine],
    candle_idx: usize,
    settings: &MaClusterSettings,
    x: f32,
    y: f32,
) {
    let font = FontId::proportional(11.0);
    let score = calculate_ma_cluster_score(ma_lines, candle_idx, settings);

    let (score_text, score_color) = match score {
        Some(s) => {
            let c = if s.score >= 70.0 {
                Color32::from_rgb(0x4C, 0xAF, 0x50) // green
            } else if s.score >= 40.0 {
                Color32::from_rgb(0xFF, 0xC1, 0x07) // amber
            } else {
                Color32::from_rgb(0xEF, 0x53, 0x50) // red
            };
            (format!("簇:{:.1}", s.score), c)
        }
        None => ("簇:--".to_string(), TEXT_DIM),
    };

    let mut cx = x;
    let g1 = painter.layout_no_wrap(score_text, font.clone(), score_color);
    let w1 = g1.size().x;
    painter.galley(pos2(cx, y), g1, score_color);
    cx += w1 + 14.0;

    let bull_text = match score {
        Some(s) => format!("多头:{:.0}%", s.bull * 100.0),
        None => "多头:--".to_string(),
    };
    let g2 = painter.layout_no_wrap(bull_text, font.clone(), TEXT_COLOR);
    let w2 = g2.size().x;
    painter.galley(pos2(cx, y), g2, TEXT_COLOR);
    cx += w2 + 14.0;

    let amp_text = match score {
        Some(s) => format!("幅度:{:.1}%", s.amp_pct),
        None => "幅度:--".to_string(),
    };
    let g3 = painter.layout_no_wrap(amp_text, font, TEXT_COLOR);
    painter.galley(pos2(cx, y), g3, TEXT_COLOR);
}

fn draw_ma_cluster_panel(
    painter: &Painter,
    ma_lines: &[MaLine],
    state: &ChartState,
    rect: &Rect,
    settings: &MaClusterSettings,
    n_candles: usize,
    hovered_idx: Option<usize>,
) {
    painter.rect_filled(*rect, 0.0, BG_COLOR);
    let clipped = painter.with_clip_rect(*rect);

    let y_for_score = |s: f64| -> f32 {
        let t = (s.clamp(0.0, 100.0) / 100.0) as f32;
        rect.max.y - t * rect.height()
    };

    let amber = Color32::from_rgb(0xFF, 0xC1, 0x07);
    let green = Color32::from_rgb(0x4C, 0xAF, 0x50);
    let red = Color32::from_rgb(0xEF, 0x53, 0x50);

    // Threshold dashed lines at 40 (amber) and 70 (green).
    draw_dashed_line_h(
        &clipped,
        rect.min.x,
        rect.max.x,
        y_for_score(40.0),
        amber.linear_multiply(0.35),
        0.5,
    );
    draw_dashed_line_h(
        &clipped,
        rect.min.x,
        rect.max.x,
        y_for_score(70.0),
        green.linear_multiply(0.35),
        0.5,
    );

    // Score polyline across visible range. Break segments where the score is
    // undefined (e.g. before MA240 is ready).
    let (start, end) = visible_range(state, n_candles);
    let stroke = Stroke::new(1.2, amber);
    let mut segment: Vec<Pos2> = Vec::new();
    let flush = |seg: &mut Vec<Pos2>, painter: &Painter| {
        if seg.len() >= 2 {
            painter.add(Shape::line(seg.clone(), stroke));
        }
        seg.clear();
    };
    for j in start..end {
        match calculate_ma_cluster_score(ma_lines, j, settings) {
            Some(s) => {
                let x = idx_to_x(j as f64 + 0.5, state, rect);
                let y = y_for_score(s.score);
                segment.push(pos2(x, y));
            }
            None => flush(&mut segment, &clipped),
        }
    }
    flush(&mut segment, &clipped);

    // Right-edge scale labels.
    let label_font = FontId::proportional(10.0);
    for score in [0.0, 50.0, 100.0] {
        painter.text(
            pos2(rect.max.x - 2.0, y_for_score(score)),
            Align2::RIGHT_CENTER,
            format!("{:.0}", score),
            label_font.clone(),
            TEXT_DIM,
        );
    }

    // Panel title.
    painter.text(
        pos2(rect.min.x + 6.0, rect.min.y + 4.0),
        Align2::LEFT_TOP,
        "簇分",
        FontId::proportional(11.0),
        TEXT_DIM,
    );

    // Hovered-candle crosshair + value readout.
    if let Some(idx) = hovered_idx {
        let x = idx_to_x(idx as f64 + 0.5, state, rect);
        draw_dashed_line_v(&clipped, x, rect.min.y, rect.max.y, CROSSHAIR_COLOR, 0.5);
        if let Some(s) = calculate_ma_cluster_score(ma_lines, idx, settings) {
            let color = if s.score >= 70.0 {
                green
            } else if s.score >= 40.0 {
                amber
            } else {
                red
            };
            painter.text(
                pos2(rect.min.x + 40.0, rect.min.y + 4.0),
                Align2::LEFT_TOP,
                format!("{:.1}", s.score),
                FontId::proportional(11.0),
                color,
            );
        }
    }
}

fn draw_ma_legend(
    painter: &Painter,
    ma_lines: &[MaLine],
    ma_visible: &[bool],
    candle_idx: usize,
    x: f32,
    y: f32,
) {
    let mut cx = x;
    for (i, ma) in ma_lines.iter().enumerate() {
        if i >= ma_visible.len() || !ma_visible[i] {
            continue;
        }
        let color = ma_color(ma.period);
        let val = if candle_idx < ma.values.len() {
            ma.values[candle_idx]
        } else {
            f64::NAN
        };
        let text = if val.is_nan() {
            format!("MA{}:--", ma.period)
        } else {
            format!("MA{}:{:.2}", ma.period, val)
        };
        let galley = painter.layout_no_wrap(text, FontId::proportional(11.0), color);
        let w = galley.size().x;
        painter.galley(pos2(cx, y), galley, color);
        cx += w + 14.0;
    }
}

fn draw_volume(painter: &Painter, candles: &[Candle], state: &ChartState, rect: &Rect) {
    let (start, end) = visible_range(state, candles.len());
    if start >= end {
        return;
    }

    let max_vol = candles[start..end]
        .iter()
        .map(|c| c.volume)
        .fold(0.0f64, f64::max);
    if max_vol <= 0.0 {
        return;
    }

    let vol_h = rect.height() * VOLUME_RATIO;
    let candle_px = rect.width() as f64 / state.candles_in_view;
    let body_w = (candle_px * 0.7).max(1.0) as f32;

    for i in start..end {
        let c = &candles[i];
        let cx = idx_to_x(i as f64 + 0.5, state, rect);
        let is_up = c.close >= c.open;
        let color = if is_up {
            Color32::from_rgba_unmultiplied(0x26, 0xa6, 0x9a, 0x40)
        } else {
            Color32::from_rgba_unmultiplied(0xef, 0x53, 0x50, 0x40)
        };

        let h = (c.volume / max_vol) as f32 * vol_h;
        let vol_rect =
            Rect::from_min_size(pos2(cx - body_w / 2.0, rect.max.y - h), vec2(body_w, h));
        painter.rect_filled(vol_rect, 0.0, color);
    }
}

fn draw_grid(painter: &Painter, rect: &Rect, state: &ChartState, candles: &[Candle]) {
    let price_range = state.price_max - state.price_min;
    if price_range <= 0.0 {
        return;
    }

    // Horizontal (price)
    let step = nice_step(price_range, 6);
    let mut price = (state.price_min / step).ceil() * step;
    while price <= state.price_max {
        let y = price_to_y(price, state, rect);
        if y > rect.min.y && y < rect.max.y {
            painter.line_segment(
                [pos2(rect.min.x, y), pos2(rect.max.x, y)],
                Stroke::new(0.5, GRID_COLOR),
            );
        }
        price += step;
    }

    // Vertical (time)
    let time_step = nice_step(state.candles_in_view, 8).max(1.0);
    let first = ((state.offset / time_step).ceil() * time_step) as i64;
    let last = (state.offset + state.candles_in_view) as i64;
    let mut idx = first;
    while idx <= last {
        if idx >= 0 && (idx as usize) < candles.len() {
            let x = idx_to_x(idx as f64 + 0.5, state, rect);
            if x > rect.min.x && x < rect.max.x {
                painter.line_segment(
                    [pos2(x, rect.min.y), pos2(x, rect.max.y)],
                    Stroke::new(0.5, GRID_COLOR),
                );
            }
        }
        idx += time_step as i64;
    }
}

fn draw_current_price_line(painter: &Painter, candles: &[Candle], state: &ChartState, rect: &Rect) {
    if let Some(last) = candles.last() {
        let y = price_to_y(last.close, state, rect);
        if y > rect.min.y && y < rect.max.y {
            let color = if last.close >= last.open {
                UP_COLOR
            } else {
                DOWN_COLOR
            };
            draw_dashed_line_h(painter, rect.min.x, rect.max.x, y, color, 0.8);
        }
    }
}

fn draw_price_axis(painter: &Painter, rect: &Rect, state: &ChartState) {
    let range = state.price_max - state.price_min;
    if range <= 0.0 {
        return;
    }
    painter.rect_filled(*rect, 0.0, BG_COLOR);

    let step = nice_step(range, 6);
    let mut price = (state.price_min / step).ceil() * step;
    while price <= state.price_max {
        let frac = (state.price_max - price) / range;
        let y = rect.min.y + frac as f32 * rect.height();
        if y > rect.min.y + 8.0 && y < rect.max.y - 8.0 {
            painter.text(
                pos2(rect.min.x + 6.0, y),
                Align2::LEFT_CENTER,
                format_price(price),
                FontId::proportional(11.0),
                TEXT_DIM,
            );
        }
        price += step;
    }
}

fn draw_time_axis(painter: &Painter, rect: &Rect, candles: &[Candle], state: &ChartState) {
    painter.rect_filled(*rect, 0.0, BG_COLOR);

    let time_step = nice_step(state.candles_in_view, 8).max(1.0);
    let first = ((state.offset / time_step).ceil() * time_step) as i64;
    let last = (state.offset + state.candles_in_view) as i64;
    let mut idx = first;
    while idx <= last {
        if idx >= 0 && (idx as usize) < candles.len() {
            let frac = (idx as f64 + 0.5 - state.offset) / state.candles_in_view;
            let x = rect.min.x + frac as f32 * rect.width();
            if x > rect.min.x + 30.0 && x < rect.max.x - 30.0 {
                let ts = &candles[idx as usize].timestamp;
                let label = if ts.len() > 10 { &ts[..10] } else { ts };
                painter.text(
                    pos2(x, rect.center().y),
                    Align2::CENTER_CENTER,
                    label,
                    FontId::proportional(10.0),
                    TEXT_DIM,
                );
            }
        }
        idx += time_step as i64;
    }
}

// ─── Crosshair ───────────────────────────────────────────────

fn draw_crosshair(
    painter: &Painter,
    pos: Pos2,
    chart_rect: &Rect,
    price_rect: &Rect,
    time_rect: &Rect,
    candles: &[Candle],
    state: &ChartState,
) {
    let raw = x_to_idx(pos.x, state, chart_rect);
    let snapped = raw.round().clamp(0.0, candles.len() as f64 - 1.0) as usize;
    let snap_x = idx_to_x(snapped as f64 + 0.5, state, chart_rect);

    // Vertical dashed line
    draw_dashed_line_v(
        painter,
        snap_x,
        chart_rect.min.y,
        chart_rect.max.y,
        CROSSHAIR_COLOR,
        0.5,
    );

    // Horizontal dashed line
    draw_dashed_line_h(
        painter,
        chart_rect.min.x,
        chart_rect.max.x,
        pos.y,
        CROSSHAIR_COLOR,
        0.5,
    );

    // Price label on axis
    let price = y_to_price(pos.y, state, chart_rect);
    let price_text = format_price(price);
    let tw = price_text.len() as f32 * 7.5 + 10.0;
    let lbl = Rect::from_min_size(
        pos2(price_rect.min.x, pos.y - 10.0),
        vec2(tw.min(PRICE_AXIS_WIDTH), 20.0),
    );
    painter.rect_filled(lbl, 3.0, LABEL_BG);
    painter.text(
        lbl.center(),
        Align2::CENTER_CENTER,
        &price_text,
        FontId::proportional(11.0),
        TEXT_COLOR,
    );

    // Time label on axis
    if snapped < candles.len() {
        let ts = &candles[snapped].timestamp;
        let tw2 = ts.len() as f32 * 7.0 + 12.0;
        let lbl2 = Rect::from_min_size(
            pos2(snap_x - tw2 / 2.0, time_rect.min.y + 1.0),
            vec2(tw2, TIME_AXIS_HEIGHT - 2.0),
        );
        painter.rect_filled(lbl2, 3.0, LABEL_BG);
        painter.text(
            lbl2.center(),
            Align2::CENTER_CENTER,
            ts,
            FontId::proportional(10.0),
            TEXT_COLOR,
        );
    }
}

fn draw_ohlcv_label(painter: &Painter, c: &Candle, x: f32, y: f32) {
    let is_up = c.close >= c.open;
    let color = if is_up { UP_COLOR } else { DOWN_COLOR };
    let text = format!(
        "O {}  H {}  L {}  C {}  V {}",
        format_price(c.open),
        format_price(c.high),
        format_price(c.low),
        format_price(c.close),
        format_volume(c.volume),
    );
    painter.text(
        pos2(x, y),
        Align2::LEFT_TOP,
        &text,
        FontId::proportional(12.0),
        color,
    );
}

fn format_volume(v: f64) -> String {
    if v >= 1.0e8 {
        format!("{:.2}亿", v / 1.0e8)
    } else if v >= 1.0e4 {
        format!("{:.2}万", v / 1.0e4)
    } else {
        format!("{:.0}", v)
    }
}

// ─── Selection & Agent Result ─────────────────────────────────

fn draw_selection(painter: &Painter, state: &ChartState, rect: &Rect) {
    let (start, end) = match &state.selection {
        SelectionState::Selecting { anchor, current } => {
            let s = (*anchor).min(*current);
            let e = (*anchor).max(*current);
            (s, e)
        }
        SelectionState::Selected { start, end } => (*start, *end),
        SelectionState::Idle => return,
    };

    let x0 = idx_to_x(start as f64, state, rect);
    let x1 = idx_to_x(end as f64 + 1.0, state, rect);
    let sel_rect = Rect::from_min_max(pos2(x0, rect.min.y), pos2(x1, rect.max.y));

    // Fill color based on agent result
    let fill = match &state.agent_status {
        AgentStatus::Done(r) => {
            if r.r#match {
                Color32::from_rgba_unmultiplied(0x26, 0xa6, 0x9a, 0x25)
            } else {
                Color32::from_rgba_unmultiplied(0xef, 0x53, 0x50, 0x25)
            }
        }
        _ => Color32::from_rgba_unmultiplied(0x29, 0x62, 0xFF, 0x20),
    };

    painter.rect_filled(sel_rect, 0.0, fill);

    // Boundary lines
    let line_color = Color32::from_rgba_unmultiplied(0x29, 0x62, 0xFF, 0x80);
    painter.line_segment(
        [pos2(x0, rect.min.y), pos2(x0, rect.max.y)],
        Stroke::new(1.0, line_color),
    );
    painter.line_segment(
        [pos2(x1, rect.min.y), pos2(x1, rect.max.y)],
        Stroke::new(1.0, line_color),
    );

    // Show selected candle count
    let count = end - start + 1;
    let count_text = format!("{}", count);
    let mid_x = (x0 + x1) / 2.0;
    let label_y = rect.max.y - 30.0;
    let galley = painter.layout_no_wrap(
        count_text,
        FontId::proportional(11.0),
        Color32::from_rgb(0xd1, 0xd4, 0xdc),
    );
    let tw = galley.size().x + 10.0;
    let th = galley.size().y + 4.0;
    let label_rect = Rect::from_min_size(pos2(mid_x - tw / 2.0, label_y - th / 2.0), vec2(tw, th));
    painter.rect_filled(
        label_rect,
        3.0,
        Color32::from_rgba_unmultiplied(0x29, 0x62, 0xFF, 0xA0),
    );
    painter.galley(
        pos2(label_rect.min.x + 5.0, label_rect.min.y + 2.0),
        galley,
        Color32::TRANSPARENT,
    );
}

fn draw_agent_result(painter: &Painter, state: &ChartState, rect: &Rect) {
    let (start, end) = match &state.selection {
        SelectionState::Selected { start, end } => (*start, *end),
        _ => return,
    };

    let x0 = idx_to_x(start as f64, state, rect);
    let x1 = idx_to_x(end as f64 + 1.0, state, rect);
    let panel_x = (x0 + x1) / 2.0;

    match &state.agent_status {
        AgentStatus::Running => {
            let text = "Analyzing...";
            let galley = painter.layout_no_wrap(
                text.to_string(),
                FontId::proportional(13.0),
                Color32::from_rgb(0x90, 0xCA, 0xF9),
            );
            let tw = galley.size().x + 20.0;
            let th = galley.size().y + 12.0;
            let bg = Rect::from_min_size(pos2(panel_x - tw / 2.0, rect.min.y + 60.0), vec2(tw, th));
            painter.rect_filled(bg, 6.0, Color32::from_rgb(0x1e, 0x22, 0x2d));
            painter.rect_stroke(
                bg,
                6.0,
                Stroke::new(1.0, Color32::from_rgb(0x36, 0x3a, 0x45)),
            );
            painter.galley(
                pos2(bg.min.x + 10.0, bg.min.y + 6.0),
                galley,
                Color32::TRANSPARENT,
            );
        }
        AgentStatus::Done(result) => {
            let accent = if result.r#match { UP_COLOR } else { DOWN_COLOR };

            let items = [
                format!("match: {}", result.r#match),
                format!("confidence: {:.2}", result.confidence),
            ];

            let panel_w: f32 = 240.0;
            let line_h: f32 = 18.0;
            let panel_h = items.len() as f32 * line_h + 16.0;

            let px = (panel_x - panel_w / 2.0).clamp(rect.min.x + 2.0, rect.max.x - panel_w - 2.0);
            let py = rect.min.y + 60.0;
            let bg = Rect::from_min_size(pos2(px, py), vec2(panel_w, panel_h));

            painter.rect_filled(bg, 6.0, Color32::from_rgb(0x1e, 0x22, 0x2d));
            painter.rect_stroke(
                bg,
                6.0,
                Stroke::new(1.0, Color32::from_rgb(0x36, 0x3a, 0x45)),
            );

            // Accent left bar
            let accent_rect = Rect::from_min_size(bg.min, vec2(3.0, panel_h));
            painter.rect_filled(
                accent_rect,
                Rounding {
                    nw: 6.0,
                    sw: 6.0,
                    ne: 0.0,
                    se: 0.0,
                },
                accent,
            );

            let tx = bg.min.x + 12.0;
            let mut ty = bg.min.y + 8.0;

            for item in &items {
                painter.text(
                    pos2(tx, ty),
                    Align2::LEFT_TOP,
                    item,
                    FontId::proportional(12.0),
                    accent,
                );
                ty += line_h;
            }
        }
        AgentStatus::Error(msg) => {
            let err_text = if msg.len() > 60 {
                format!("Error: {}...", &msg[..57])
            } else {
                format!("Error: {}", msg)
            };
            let galley = painter.layout_no_wrap(err_text, FontId::proportional(12.0), DOWN_COLOR);
            let tw = galley.size().x + 20.0;
            let th = galley.size().y + 12.0;
            let bg = Rect::from_min_size(pos2(panel_x - tw / 2.0, rect.min.y + 60.0), vec2(tw, th));
            painter.rect_filled(bg, 6.0, Color32::from_rgb(0x2d, 0x1e, 0x1e));
            painter.rect_stroke(bg, 6.0, Stroke::new(1.0, DOWN_COLOR));
            painter.galley(
                pos2(bg.min.x + 10.0, bg.min.y + 6.0),
                galley,
                Color32::TRANSPARENT,
            );
        }
        AgentStatus::Idle => {}
    }
}

// ─── Chip Distribution ───────────────────────────────────────

fn draw_chip_distribution(
    painter: &Painter,
    candles: &[Candle],
    state: &mut ChartState,
    rect: &Rect,
) {
    let (_, end) = visible_range(state, candles.len());
    let ref_idx = end.saturating_sub(1);
    if ref_idx >= candles.len() {
        return;
    }

    // Check cache
    let needs_recalc = match &state.chip_cache {
        Some(cache) => cache.ref_idx != ref_idx,
        None => true,
    };

    if needs_recalc {
        let dist = calculate_chip_distribution(candles, ref_idx, &state.chip_settings);
        state.chip_cache = Some(ChipCache { ref_idx, dist });
    }

    let dist = match &state.chip_cache {
        Some(cache) => &cache.dist,
        None => return,
    };

    // Background
    painter.rect_filled(*rect, 0.0, BG_COLOR);
    let clipped = painter.with_clip_rect(*rect);

    if dist.max_chips <= 0.0 || dist.bins.is_empty() || dist.bin_width <= 0.0 {
        painter.line_segment(
            [rect.left_top(), rect.left_bottom()],
            Stroke::new(1.0, BORDER_COLOR),
        );
        return;
    }

    // Draw histogram bars
    let max_w = rect.width() - 4.0;

    for bin in &dist.bins {
        if bin.chips <= 0.0 {
            continue;
        }

        let top_price = bin.price + dist.bin_width / 2.0;
        let bottom_price = bin.price - dist.bin_width / 2.0;
        let y0 = price_to_y(top_price, state, rect);
        let y1 = price_to_y(bottom_price, state, rect);
        let unclipped_top = y0.min(y1);
        let unclipped_bottom = y0.max(y1);
        if unclipped_bottom < rect.min.y || unclipped_top > rect.max.y {
            continue;
        }
        let top = unclipped_top.max(rect.min.y);
        let bottom = unclipped_bottom.min(rect.max.y);

        let bar_w = (bin.chips / dist.max_chips) as f32 * max_w;

        let color = if bin.price <= dist.ref_price {
            Color32::from_rgba_unmultiplied(0xef, 0x53, 0x50, 0xB0) // profit (red)
        } else {
            Color32::from_rgba_unmultiplied(0x29, 0x62, 0xFF, 0xB0) // loss (blue)
        };

        let bar_rect = Rect::from_min_size(
            pos2(rect.max.x - bar_w - 2.0, top),
            vec2(bar_w, (bottom - top).max(1.0)),
        );
        clipped.rect_filled(bar_rect, 0.0, color);
    }

    // Cost center dashed line + 重心 label that travels with it.
    if dist.cost_center > 0.0 {
        let cc_y = price_to_y(dist.cost_center, state, rect);
        if cc_y >= rect.min.y && cc_y <= rect.max.y {
            let cc_color = Color32::from_rgb(0xFF, 0xD7, 0x00); // gold
            draw_dashed_line_h(&clipped, rect.min.x, rect.max.x, cc_y, cc_color, 1.0);
            clipped.text(
                pos2(rect.min.x + 4.0, cc_y - 2.0),
                Align2::LEFT_BOTTOM,
                format!("重心 {:.2}", dist.cost_center),
                FontId::proportional(9.5),
                cc_color,
            );
        }
    }

    // Indicator stack — one metric per line, right-aligned to the chip
    // column's right edge. Order top-down: ASR / CBW / CKDP.
    let indicator_color = Color32::from_rgb(0xFF, 0xD7, 0x00);
    let indicator_font = FontId::proportional(10.0);
    let line_h = 13.0;
    let top_pad = 4.0;
    let right_pad = 4.0;
    let indicator_lines = [
        format!("ASR {:.0}%", dist.profit_ratio * 100.0),
        format!("CBW {:.1}%", dist.cbw),
        format!("CKDP {:.1}", dist.ckdp),
    ];
    for (i, text) in indicator_lines.iter().enumerate() {
        painter.text(
            pos2(
                rect.max.x - right_pad,
                rect.min.y + top_pad + i as f32 * line_h,
            ),
            Align2::RIGHT_TOP,
            text,
            indicator_font.clone(),
            indicator_color,
        );
    }

    // Average turnover note, pushed below the indicator stack so they don't
    // overlap.
    if dist.avg_turnover_rate > 0.0 {
        let stack_height = top_pad + indicator_lines.len() as f32 * line_h + 4.0;
        let note = format!(
            "按成交量估算换手\n平均{:.1}%",
            dist.avg_turnover_rate * 100.0
        );
        let note_rect = Rect::from_min_size(
            pos2(rect.min.x + 6.0, rect.min.y + stack_height),
            vec2((rect.width() - 12.0).max(0.0), 32.0),
        );
        painter.rect_filled(
            note_rect,
            4.0,
            Color32::from_rgba_unmultiplied(0x1e, 0x22, 0x2d, 0xD8),
        );
        painter.text(
            note_rect.center(),
            Align2::CENTER_CENTER,
            note,
            FontId::proportional(9.5),
            TEXT_DIM,
        );
    }

    // Left border
    painter.line_segment(
        [rect.left_top(), rect.left_bottom()],
        Stroke::new(1.0, BORDER_COLOR),
    );
}

// ─── Price Level Lines ────────────────────────────────────────

/// Draw horizontal price level lines at every `step` fraction above the
/// lowest low of the visible range (e.g. 0.2 → every 20 %, 0.3 → every 30 %).
/// The base (0 %) is anchored to `min(low)` across the visible candles so the
/// lines stay stable as the user scrolls or zooms.
fn draw_price_levels(
    painter: &Painter,
    candles: &[Candle],
    state: &ChartState,
    rect: &Rect,
    step: f64,
) {
    if candles.is_empty() || step <= 0.0 {
        return;
    }

    // Lowest low among the currently visible candles
    let (start, end) = visible_range(state, candles.len());
    if start >= end {
        return;
    }
    let base = candles[start..end]
        .iter()
        .map(|c| c.low)
        .fold(f64::MAX, f64::min);
    if base <= 0.0 {
        return;
    }

    // Amber, kept subtle so it reads clearly without dominating the chart
    let line_color = Color32::from_rgba_unmultiplied(0xFF, 0xC1, 0x07, 0x48);
    let label_color = Color32::from_rgba_unmultiplied(0xFF, 0xC1, 0x07, 0xA8);

    // Skip to the first level that could be visible (at or just below price_min)
    let n_start: usize = if state.price_min > base {
        ((state.price_min / base - 1.0) / step).floor() as usize
    } else {
        0
    };

    let mut prev_label_y = f32::NEG_INFINITY;
    let mut prev_line_y = f32::NEG_INFINITY;

    let mut n = n_start;
    loop {
        let price = base * (1.0 + n as f64 * step);

        // Stop when above the visible price range (with a small margin)
        if price > state.price_max || n > 200 {
            break;
        }

        let y = price_to_y(price, state, rect);

        // Skip lines that are too close together (< 12 px) to avoid solid fill
        // from densely-packed levels when base is very small (forward-adjusted data)
        if (y - prev_line_y).abs() < 12.0 {
            n += 1;
            continue;
        }
        prev_line_y = y;

        // Draw the dashed level line across the full chart width
        draw_dashed_line_h(painter, rect.min.x, rect.max.x, y, line_color, 0.8);

        // Percentage label on the right edge.
        // Suppress when the previous label is fewer than 14 px away to avoid overlap.
        if (y - prev_label_y).abs() >= 14.0 {
            let label = if n == 0 {
                "底".to_string()
            } else {
                format!("+{}%", (n as f64 * step * 100.0).round() as i64)
            };
            painter.text(
                pos2(rect.max.x - 6.0, y - 3.0),
                Align2::RIGHT_BOTTOM,
                &label,
                FontId::proportional(9.5),
                label_color,
            );
            prev_label_y = y;
        }

        n += 1;
    }
}

// ─── Helpers ─────────────────────────────────────────────────

fn draw_dashed_line_h(painter: &Painter, x0: f32, x1: f32, y: f32, color: Color32, width: f32) {
    let dash = 5.0;
    let gap = 3.0;
    let mut x = x0;
    while x < x1 {
        let end = (x + dash).min(x1);
        painter.line_segment([pos2(x, y), pos2(end, y)], Stroke::new(width, color));
        x = end + gap;
    }
}

fn draw_dashed_line_v(painter: &Painter, x: f32, y0: f32, y1: f32, color: Color32, width: f32) {
    let dash = 5.0;
    let gap = 3.0;
    let mut y = y0;
    while y < y1 {
        let end = (y + dash).min(y1);
        painter.line_segment([pos2(x, y), pos2(x, end)], Stroke::new(width, color));
        y = end + gap;
    }
}

fn nice_step(range: f64, target_steps: usize) -> f64 {
    if range <= 0.0 {
        return 1.0;
    }
    let rough = range / target_steps as f64;
    let mag = 10.0f64.powf(rough.log10().floor());
    let norm = rough / mag;
    let nice = if norm <= 1.0 {
        1.0
    } else if norm <= 2.0 {
        2.0
    } else if norm <= 5.0 {
        5.0
    } else {
        10.0
    };
    nice * mag
}

fn format_price(price: f64) -> String {
    format!("{:.2}", price)
}
