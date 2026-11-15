# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Build
~/.cargo/bin/cargo build

# Run (optional path arg overrides settings data_dir)
~/.cargo/bin/cargo run -- [/path/to/stock-data]

# Test (all tests are in data.rs)
~/.cargo/bin/cargo test

# Run a single test by name
~/.cargo/bin/cargo test <test_name>
```

## Architecture

Three source files — no modules beyond that:

- **`src/data.rs`** — all data types, I/O, and business logic: settings/persistence, candle loading, MA calculation, arc-structure detection (`arc_*`), filter parsing and evaluation, stock-dl output parsing.
- **`src/chart.rs`** — stateless rendering of the K-line chart. Takes `&[Candle]`, `&[MaLine]`, and `&mut ChartState`; draws via `egui::Painter`. No data loading.
- **`src/main.rs`** — the `eframe::App` impl (`ChartApp`). Owns all UI state and drives background threads via `mpsc` channels for sync (`run_sync`), filter (`run_filter`), auto-sync (`try_start_auto_sync`), and trade-agent (`run_trade_agent`).

### Data flow

```
settings.json → AppSettings
stock data dir → load_stock_list → Vec<(StockInfo, PathBuf)>
stock dir/daily.csv → load_candles → (Vec<Candle>, Vec<MaLine>)
                     ↑ weekly candles are aggregated in-memory from daily
```

### Persistence (all under `~/.config/chart-app/`)

| File | Content |
|---|---|
| `settings.json` | `AppSettings`: data dir, binary paths, MA windows, named filter bundles |
| `favorites.json` | `Vec<String>` of secids |
| `filtered.json` | `HashMap<filter_name, Vec<String>>` of matched secids per filter |
| `last_selected.json` | `HashMap<tab_key, secid>` |
| `selected_filter.json` | Last-chosen filter name |

### External binaries

- **`stock-dl`** — downloads/updates stock CSV data. Called as `stock-dl -o <dir> download [-c N] [--codes ...]`.
- **`trade-agent`** — AI analysis of a selected range. Called as `trade-agent <csv> --date-range START:END [--model M] [--csv_as_chart true]`. Returns JSON `{match, confidence, explanation}`.

### Filter expressions

Filters live in `settings.json` under `data_filters` as `NamedFilter` objects, and
are created/edited in the app (筛选 tab → 编辑/新建, `FilterEditor` in `main.rs`):

```json
{
  "name": "my-filter",
  "periods": ["daily"],
  "filters": [
    "week: close > ma100",
    "day: close > ma120 and (ma60 > ma120 or ma120 > ma120[-1])",
    "price/price[-200] <= 2.0"
  ]
}
```

Each entry in `filters` is one line of the editor:

```
[day:|week:] <bool expr>
```

- **Lines are ANDed.** Blank lines and `#` / `//` lines are ignored.
- **Period prefix** (`day`/`daily`/`d`, `week`/`weekly`/`w`) pins that line to one
  timeframe. Without a prefix the line falls back to the bundle's `periods` and
  passes if it holds on any of them (`["daily"]` by default).
- **Boolean operators** `and` / `or` (case-insensitive) with parentheses; `and`
  binds tighter than `or`.
- Operand syntax: `column` or `column[N]` (N bars ago; sign ignored). Division via `/`.
- Supported columns: `close`/`price`, `open`, `high`, `low`, `volume`, `maN` (any period),
  and the `arc_*` columns below. Unknown columns are rejected at parse time.
- Comparison operators: `>`, `>=`, `<`, `<=`.
- MA periods not in the configured `ma` list are computed on-demand during filter evaluation.

Parsing lives in `data.rs`: `parse_filter_lines` → `parse_filter_line` →
`parse_filter_expr` (→ `FilterExpr` tree) → `parse_data_filter` for each leaf
comparison. `evaluate_filters_on_stock` reads each stock's CSV once (weekly is
aggregated from the daily candles in memory) and checks lines without `arc_*`
first, so arc detection only runs on stocks that survive the cheap lines.

### Arc structures (越振越强, `arc_*` columns)

Stripped of its candles the shape is two strokes: a steep rally, then a long
stretch that drifts gently upward — the fight after the rally keeps being
settled a little higher, and the lows of that stretch trace a rising arc.
`find_arc_structures` measures exactly that with three least-squares fits and
no notion of individual swings:

- **Rally** — every local high is tried as the rally's top. Walking back from
  it, the rally starts at the lowest low from which the climb is still one
  direct leg (a long base ending in a spike only counts the spike).
- **Centre** (震荡重心) — a line through the closes since the top. It must rise,
  but far less steeply than the rally, start in the upper part of the rally,
  and keep rising quarter by quarter; the closes must swing across it, with a
  scatter (`sigma`) that is moderate, small next to the rally, and similar in
  both halves.
- **Bottoms** — a parabola under the lows since the top, fitted by asymmetric
  least squares so it settles on their underside while a stray shadow cannot
  drag it down. It must rise and accelerate (bend upward — a straight line
  doesn't qualify), and not be undercut too deeply.

`arc_at` holds the checks. All tolerances (`ARC_*` constants) are fractions of
the rally's height or of `sigma`, so the rules are the same for any volatility
and timeframe. They were calibrated on three hand-drawn examples and one real
one (东航物流, 2023-11 → 2024-04), kept in the tests as `arc_sketches()` and
`ARC_EXAMPLE` — if you retune a constant, those tests say whether the reference
shapes are still found.

| Column | Meaning |
|---|---|
| `arc_len` | Bars from the rally's starting low to the latest bar |
| `arc_run` | The rally: starting low → top, % |
| `arc_peak` | Starting low → highest high reached since, % (how far the whole move has carried) |
| `arc_amp` | Typical swing since the top (3 sigma around the centre line), % |
| `arc_rise` | How far the centre line has risen since the top, % |
| `arc_lift` | How far the arc under the lows has risen since the top, % |
| `arc_bend` | Curvature of that arc, % of the lift: at least 10 (accelerating); 20–60 = clear arc |
| `arc_break` | Latest close vs the highest high since the rally's top, % (always ≤ 0) |

A stock can hold more than one structure (different rallies), so `arc_*`
columns are bound like a variable: **within one line, on one timeframe, every
`arc_*` refers to the same structure, and the line holds if any one structure
satisfies it.** Conditions on one structure therefore go on a single line
joined with `and`; separate lines may each be met by a different structure.
With no structure the comparisons are unresolved (false). `arc_*` columns take
no `[N]` offset.

```
arc_len >= 40 and arc_peak <= 100 and arc_break >= -10
```

In the Filtered tab the chart marks the structure that got the stock picked
(`chart.rs:draw_arc`): the rally in white from 起涨 to its top, the centre line
in blue, the arc under the lows in yellow, and a one-line summary top-right;
the 走势标识 toolbar button hides it. `ChartApp::refresh_arc_overlay` picks it
with `arc_to_draw` for the timeframe on screen, and selecting a stock there
switches the chart to the timeframe on which it qualifies
(`ChartApp::matched_period`).

To scan both timeframes, leave the line unprefixed and set the bundle's
`periods` to `["daily", "weekly"]` (the editor keeps `periods` but cannot edit
it; new bundles are created daily-only).

### MA colors (`chart.rs:ma_color`)

Fixed color map: 5→yellow, 10→blue, 20→orange, 30→pink, 60→purple, 120→cyan, 240/250→green.
