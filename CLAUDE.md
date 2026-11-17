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

# Run one filter bundle without opening a window; prints `secid name` per match
# (NAME = a saved bundle or a preset; nothing is written to disk)
~/.cargo/bin/cargo run --release -- --filter 蓄势待发 [/path/to/stock-data]
```

## Architecture

Three source files — no modules beyond that:

- **`src/data.rs`** — all data types, I/O, and business logic: settings/persistence, candle loading, MA calculation, arc- and pit-structure detection (`arc_*`, `pit_*`), filter parsing and evaluation (incl. the window columns and the preset bundles), stock-dl output parsing.
- **`src/chart.rs`** — stateless rendering of the K-line chart. Takes `&[Candle]`, `&[MaLine]`, and `&mut ChartState`; draws via `egui::Painter`. No data loading.
- **`src/main.rs`** — the `eframe::App` impl (`ChartApp`). Owns all UI state and drives background threads via `mpsc` channels for sync (`run_sync`), filter (`run_filter`), auto-sync (`try_start_auto_sync`), and trade-agent (`run_trade_agent`). Also the headless `--filter` mode (`run_filter_cli`).

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
| `last_selected.json` | `HashMap<key, secid>`: `all`, `favorites`, and one `filtered:<bundle>` per filter bundle |
| `selected_filter.json` | Name of the filter bundle in focus |

### External binaries

- **`stock-dl`** — downloads/updates stock CSV data. Called as `stock-dl -o <dir> download [-c N] [--codes ...]`.
- **`trade-agent`** — AI analysis of a selected range. Called as `trade-agent <csv> --date-range START:END [--model M] [--csv_as_chart true]`. Returns JSON `{match, confidence, explanation}`.

### Filter tabs

The left panel's tab bar is 全部 | 自选 | one tab per filter bundle | + (new
bundle). A bundle's tab (`ChartApp::show_filter_tab`) holds its own buttons —
筛选 / 重新筛选, 同步 (stock-dl for just its matches), 编辑 — its own status
line, and its own list of matches. `StockTab::Filtered` is the kind of all those
tabs; which one is open is `selected_filter_name`, the bundle in focus.

- State is per bundle, keyed by name: `filtered` (the cached matches, mirroring
  `filtered.json`; no entry = not run since its conditions last changed),
  `filter_status`, and `filter_receivers` while a scan runs. Scans are
  independent, so several bundles can run at once (a running one shows `…` on
  its tab); a worker sends its matches back and the UI thread writes
  `filtered.json`.
- On a bundle's tab, arrow keys walk its list and the chart marks the
  structures that bundle matched. Every tab remembers its own last-viewed stock.
- Renaming, editing or deleting a bundle carries its state along
  (`rename_filter_state`, `delete_filter_bundle`); changing its conditions drops
  its matches and any scan still running for the old ones. Saving a bundle
  opens its tab.

### Filter expressions

Filters live in `settings.json` under `data_filters` as `NamedFilter` objects, and
are created/edited in the app (the + tab, or 编辑 on a bundle's tab; `FilterEditor` in `main.rs`):

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
  the window columns and the `arc_*` / `pit_*` columns below. Unknown columns are rejected at parse time.
- Window columns measure the N bars ending at the row read (so they take `[N]` offsets
  too, and count weeks on a `week:` line). Unresolved until N bars of history exist.

  | Column | Meaning |
  |---|---|
  | `hhvN` / `llvN` | Highest high / lowest low |
  | `posN` | Where the close sits between `llvN` (0) and `hhvN` (100) |
  | `atrN` | Average true range, % of the previous close |
  | `vmaN` | Average volume |
- Comparison operators: `>`, `>=`, `<`, `<=`.
- MA periods not in the configured `ma` list are computed on-demand during filter evaluation.

Parsing lives in `data.rs`: `parse_filter_lines` → `parse_filter_line` →
`parse_filter_expr` (→ `FilterExpr` tree) → `parse_data_filter` for each leaf
comparison. `evaluate_filters_on_stock` reads each stock's CSV once (weekly is
aggregated from the daily candles in memory) and checks lines without `arc_*` /
`pit_*` first, so structure detection only runs on stocks that survive the
cheap lines.

### Preset bundles (蓄势待发 / 放量突破 / 下破变脸)

`preset_filters()` in `data.rs` holds three ready-made bundles, offered as buttons in
the editor's 新建 dialog and runnable by name with `--filter`. They are ordinary
filter lines, so their thresholds are tuned in the editor. A preset also carries its
`periods`: 下破变脸 is one unprefixed `pit_*` line scanned on both timeframes (see
"Pit structures" below), plus `day: close > ma100`. It finds a shape and promises
nothing: a rough check of its daily picks (every 5th session, 2019–2026, 14.9k
picks, a median of 22 a day) showed about the market's rate of +20%-in-20-bars
moves (14.6% vs 13.7%) and no dependable edge in 20-bar return (+0.8 points
pooled, but +0.04 averaged by date, t ≈ 0.2; 53% beat the day's median stock).
The MA line halves the picks (the shape alone: 28.7k, 43 a day) without changing
that picture.

蓄势待发 and 放量突破 are lines over the window columns.
They come from a study of what every A-share looked like before a burst (+30%
within 20 sessions) or a sustained run (+50% within 60), 2018–2026 — the report
with the method, the hit rates per year and the caveats is `docs/prelaunch-study.html`.
The shape, in four strokes:

1. **Long-term up** — weekly close above a rising 30-week MA; MA60 rising and above MA120.
2. **Has rallied, and is active** — 40%+ above the 120-bar low, a 120-bar high within
   the last 60 bars, `atr60 >= 4`.
3. **Holding near the top while volume cools** — `pos60 >= 60`, `vma60 < vma250`.
4. **Volume returning, price not yet moved** — `vma5 >= vma60`, 5-bar change within ±8%.

蓄势待发 is that setup (about 13 stocks a day; roughly 2.4× the market's rate of
+30%-in-20-days moves, 2× for sustained runs, above the base rate in every year).
放量突破 is the confirmation: the first +5% candle on 1.5× volume to a 60-bar high —
a higher burst rate but many false breakouts and no edge in average return.

The tests build a synthetic series with that shape (`setup_bars`) and break one
property at a time; if a preset line changes, they say whether the shape is still
found and the look-alikes still rejected.

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

On a filter bundle's tab the chart marks the structure that got the stock picked
(`chart.rs:draw_arc`): the rally in white from 起涨 to its top, the centre line
in blue, the arc under the lows in yellow, and a one-line summary top-right;
the 走势标识 toolbar button hides it. `ChartApp::refresh_overlay` picks it with
`shapes_to_draw` (`arc_to_draw` + `pit_to_draw`) for the timeframe on screen,
and selecting a stock there switches the chart to the timeframe on which it
qualifies (`ChartApp::matched_period`).

A bundle's list is what its last scan found, while the marks are judged on the
candles as they are now — and selecting a stock auto-syncs it. So a listed stock
may no longer meet its bundle (e.g. today's bar took it past `pit_pos <= 100`).
`refresh_overlay` checks that with `evaluate_filters_on_stock`; when the stock
has left the bundle, `shapes_to_draw` falls back to the structure it was most
likely picked for (`allow_unmet`: the one meeting the most lines, else the
longest), `MatchedShapes::stale` is set, and the chart adds an orange note under
the summary. With no structure left at all the note stands alone.

To scan both timeframes, leave the line unprefixed and set the bundle's
`periods` to `["daily", "weekly"]` (the editor keeps `periods` but cannot edit
it; new bundles are created daily-only unless started from a preset that scans
both).

### Pit structures (下破变脸, `pit_*` columns)

Three strokes: a long flat stretch (震荡横盘), a drop through its floor (下破),
and a climb that takes the floor back (变脸 — 企稳走强, or 慢慢震荡向上).
`find_pit_structures` reads it back from the latest bar:

- **Pit** — every low that nothing has undercut since is tried as the pit low.
- **Box** — the stretch before the drop into that low whose closes stay in one
  band: its floor is their lowest, its top their highest. It must be low for
  its length (height ≤ 0.85 · `sigma` · √bars, where `sigma` is the typical
  bar-to-bar move of its closes — a random walk covers about 1.6), level and
  not arched (a parabola through the closes; a rally given back whole is a
  mountain, not a box), swung across (bottom third ↔ top third at least three
  times), with a floor that has held before. From its end to the pit low the
  closes stay under the floor (a retest that fails at the floor is allowed),
  the low goes below the floor by at least 0.3 of the box's height and 3
  `sigma`, and the drop takes at most a third of the box's length. Of all the
  stretches that qualify, the tightest is the box (`pit_box`).
- **Turn** — a least-squares line through the closes since the low. The
  structure is in force when at least one bar closed under the floor, the
  latest close is back at or above the floor, the line has climbed at least
  half the pit's depth, the close keeps at least half of the rebound, and the
  low is no further back than half the box's length (`pit_at`).

Box and pit are settled by the bars up to the low; a later bar only re-judges
the turn. So a structure appears when the floor is taken back, and drops out
when the close falls back under it, the rebound fades, it goes stale, or a new
low undercuts the pit.

No tolerance (`PIT_*` constants) is a bar count or a percentage: lengths are
shares of the box's length, heights are shares of its height or multiples of
`sigma`. The same rules therefore pick a six-week shelf and a two-year range,
on daily and weekly charts, in quiet and wild stocks — the tests rescale one
series in price and in time to check exactly that. The constants were
calibrated on contact sheets of real matches; the reference shapes are
`pit_closes` (synthetic, with look-alikes that each break one property) and
`PIT_EXAMPLE` (歌尔股份, 2026-07 → 2026-11). The prototype was checked against
the Rust port structure by structure over every stock on both timeframes; if
you retune a constant, the tests say whether the shapes are still found and
the look-alikes still rejected.

| Column | Meaning |
|---|---|
| `pit_box` | Bars in the box |
| `pit_amp` | Height of the box: floor → top, % (both are closes) |
| `pit_drop` | How far the pit low went below the floor, % |
| `pit_under` | Bars since the box that closed below its floor |
| `pit_len` | Bars since the pit low |
| `pit_up` | The rebound: pit low → latest close, % |
| `pit_pos` | Latest close within the box: 0 = floor, 100 = top, above 100 once it has left through the top |

`pit_*` columns are bound exactly like `arc_*`: within one line, on one
timeframe, they all refer to the same pit structure, the line holds if any one
structure satisfies it, and they take no `[N]` offset. A line may read both
families; each is bound on its own.

```
pit_box >= 30 and pit_amp <= 40 and pit_len <= 40 and pit_pos <= 100
day: close > ma100
```

Those two lines are the 下破变脸 preset: a box of at least 30 bars no taller
than 40%, the pit low within the last 40 bars, the close still inside the box —
and above its 100-day average (an ordinary filter line: change the `ma100`, or
delete it).

On the chart (`chart.rs:draw_pit`) the box is a blue rectangle with its floor
carried on as a dashed line, the pit low is marked 坑底, the turn line is
yellow, and a one-line summary sits top-right.

### MA colors (`chart.rs:ma_color`)

Fixed color map: 5→yellow, 10→blue, 20→orange, 30→pink, 60→purple, 120→cyan, 240/250→green.
