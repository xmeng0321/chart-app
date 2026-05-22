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

Filters live in `settings.json` under `data_filters` as `NamedFilter` objects:

```json
{
  "name": "my-filter",
  "periods": ["daily"],
  "filters": ["ma250 >= ma250[-1]", "price/price[-200] <= 2.0"]
}
```

- `periods`: `["daily"]` (default), `["weekly"]`, or `["daily", "weekly"]` (OR logic — stock passes if all conditions hold on any listed period).
- Operand syntax: `column` or `column[N]` (N bars ago; sign ignored). Division via `/`.
- Supported columns: `close`/`price`, `open`, `high`, `low`, `volume`, `maN` (any period).
- Operators: `>`, `>=`, `<`, `<=`.
- MA periods not in the configured `ma` list are computed on-demand during filter evaluation.

### MA colors (`chart.rs:ma_color`)

Fixed color map: 5→yellow, 10→blue, 20→orange, 30→pink, 60→purple, 120→cyan, 240/250→green.
