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

The shape: a leg up (a sharp rally, a string of small rebounds, or just one
hammer candle), then a sideways fight in which each pullback bottoms higher
than the last — the bottoms trace a rising, convex arc while the tops stay in a
band. It has no fixed period, so `find_arc_structures` works from swing pivots:

- Closes are reduced to a zigzag at each threshold in `ARC_SCALES` (4%–32%), and
  each pivot is moved to the true high/low of its swing (shadows count). Only
  the last `ARC_MAX_BARS` bars are searched.
- Every swing low is tried as the start of the rally. `arc_from` keeps the
  stretch from there to the latest bar when: nothing later undercuts the start;
  the 3–8 bottoms after the rally are mostly rising (one exception per three,
  never the latest); a parabola fitted through them is not concave and fits
  tightly; pullbacks retrace at least half of each advance; the tops stay in a
  band; the bottoms are spread out and the last one is recent; and price has
  neither fallen through the rising floor nor run far past the band.
- All tolerances (`ARC_*` constants) are fractions of the structure's own
  typical pullback, so the rules are scale-free. They were calibrated on three
  hand-drawn examples, kept as `arc_sketches()` in the tests — if you retune a
  constant, those tests say whether the reference shapes are still found.

| Column | Meaning |
|---|---|
| `arc_lows` | Bottoms (swing lows) made after the rally; at least 3 |
| `arc_len` | Bars from the rally's starting low to the latest bar |
| `arc_run` | The rally: starting low → first top, % |
| `arc_amp` | Typical pullback: median top-to-bottom swing, % |
| `arc_lift` | How far the bottoms have climbed: last vs first, % |
| `arc_bend` | Curvature of the arc, % of the lift: 0 = straight, 10–30 = clear arc, slightly negative = flattening |
| `arc_break` | Latest close vs the highest top of the sideways part, % (negative = still inside the range) |

A stock can hold several structures at once (different scales and starts), so
`arc_*` columns are bound like a variable: **within one line, on one timeframe,
every `arc_*` refers to the same structure, and the line holds if any one
structure satisfies it.** Conditions on one structure therefore go on a single
line joined with `and`; separate lines may each be met by a different
structure. With no structure the comparisons are unresolved (false). `arc_*`
columns take no `[N]` offset.

```
arc_lows >= 4 and arc_len >= 40 and arc_break <= 3
```

To scan both timeframes, leave the line unprefixed and set the bundle's
`periods` to `["daily", "weekly"]` (the editor keeps `periods` but cannot edit
it; new bundles are created daily-only).

### MA colors (`chart.rs:ma_color`)

Fixed color map: 5→yellow, 10→blue, 20→orange, 30→pink, 60→purple, 120→cyan, 240/250→green.
