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

- **`src/data.rs`** — all data types, I/O, and business logic: settings/persistence, candle loading, MA calculation, filter parsing and evaluation, stock-dl output parsing.
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
- Supported columns: `close`/`price`, `open`, `high`, `low`, `volume`, `maN` (any period).
  Unknown columns are rejected at parse time.
- Comparison operators: `>`, `>=`, `<`, `<=`.
- MA periods not in the configured `ma` list are computed on-demand during filter evaluation.

Parsing lives in `data.rs`: `parse_filter_lines` → `parse_filter_line` →
`parse_filter_expr` (→ `FilterExpr` tree) → `parse_data_filter` for each leaf
comparison. `evaluate_filters_on_stock` loads candles at most once per timeframe.

### MA colors (`chart.rs:ma_color`)

Fixed color map: 5→yellow, 10→blue, 20→orange, 30→pink, 60→purple, 120→cyan, 240/250→green.
