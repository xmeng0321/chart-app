use crate::data::{Candle, ChipSettings};

pub struct ChipBin {
    pub price: f64,
    pub chips: f64,
}

#[allow(dead_code)]
pub struct ChipDistribution {
    pub bins: Vec<ChipBin>,
    pub max_chips: f64,
    pub ref_price: f64,
    pub bin_width: f64,
    pub cost_center: f64,
    pub cost_low: f64,
    pub cost_high: f64,
    pub cbw: f64,
    pub ckdp: f64,
    /// Fraction of total chip mass at or below the reference price (i.e. the
    /// "获利盘" ratio in CN charting tools). In [0, 1].
    pub profit_ratio: f64,
    pub lookback_days: usize,
    pub avg_turnover_rate: f64,
}

pub struct ChipCache {
    pub ref_idx: usize,
    pub dist: ChipDistribution,
}

pub fn calculate_chip_distribution(
    candles: &[Candle],
    ref_idx: usize,
    settings: &ChipSettings,
) -> ChipDistribution {
    let empty = || ChipDistribution {
        bins: Vec::new(),
        max_chips: 0.0,
        ref_price: 0.0,
        bin_width: 0.0,
        cost_center: 0.0,
        cost_low: 0.0,
        cost_high: 0.0,
        cbw: 0.0,
        ckdp: 0.0,
        profit_ratio: 0.0,
        lookback_days: 0,
        avg_turnover_rate: 0.0,
    };

    if candles.is_empty() || ref_idx >= candles.len() {
        return empty();
    }

    let ref_price = candles[ref_idx].close;
    let effective_turnovers = build_effective_turnovers(candles, ref_idx, settings);
    let start = determine_start(candles, &effective_turnovers, ref_idx, settings);
    let window = &candles[start..=ref_idx];

    let (price_min, price_max) = derive_price_range(window, ref_price);
    let price_range = price_max - price_min;
    let bin_count = choose_bin_count(window.len(), ref_price, price_range, settings);
    let bin_width = price_range / bin_count as f64;
    let mut chips = vec![0.0f64; bin_count];
    let mut turnover_sum = 0.0f64;
    let mut active_days = 0usize;

    for i in start..=ref_idx {
        let c = &candles[i];
        let turnover_rate = effective_turnovers[i];
        if c.volume <= 0.0 || turnover_rate <= 0.0 {
            continue;
        }

        active_days += 1;
        turnover_sum += turnover_rate;

        // Keep chips as normalized mass rather than raw volume to avoid
        // mixing the turnover ratio with a volume-sized inventory.
        for chip in chips.iter_mut() {
            *chip *= 1.0 - turnover_rate;
        }

        let new_chips = turnover_rate;

        distribute_new_chips(
            &mut chips, c, price_min, price_max, bin_width, new_chips, settings,
        );
    }

    let total_chips: f64 = chips.iter().sum();
    if total_chips > 0.0 {
        for chip in &mut chips {
            *chip /= total_chips;
        }
    }

    // Build result
    let max_chips = chips.iter().cloned().fold(0.0f64, f64::max);

    let cost_center = (0..bin_count)
        .map(|b| {
            let bin_price = price_min + (b as f64 + 0.5) * bin_width;
            bin_price * chips[b]
        })
        .sum::<f64>();

    // Cost-price band based on cumulative chip mass. Using extreme bins (any
    // bin with chips > 0) made CBW useless because the small `baseline_weight`
    // in distribute_new_chips spreads a sliver of mass across the entire
    // intraday range. Quantiles are the standard fix used by chip-distribution
    // tools.
    let q_low = settings.cost_quantile_low.clamp(0.0, 1.0);
    let q_high = settings.cost_quantile_high.clamp(q_low, 1.0);
    let (cost_low, cost_high) =
        cost_band_from_quantiles(&chips, price_min, bin_width, q_low, q_high);

    // CBW = (高成本价 - 低成本价) / 低成本价 × 100%
    let cbw = if cost_low > 0.0 {
        (cost_high - cost_low) / cost_low * 100.0
    } else {
        0.0
    };

    // CKDP = (当前价 - 低成本价) / (高成本价 - 低成本价) × 100%
    let spread = cost_high - cost_low;
    let ckdp = if spread > 0.0 {
        (ref_price - cost_low) / spread * 100.0
    } else {
        0.0
    };

    // 获利盘比例 = ∑ chips with bin_price ≤ ref_price. Chips already sum to 1
    // (when active_days > 0), so this is in [0, 1]. We attribute a partial
    // share to the bin straddling ref_price so the value moves smoothly as
    // ref_price drifts inside a bin.
    let profit_ratio = compute_profit_ratio(&chips, price_min, bin_width, ref_price);

    let bins = (0..bin_count)
        .map(|b| ChipBin {
            price: price_min + (b as f64 + 0.5) * bin_width,
            chips: chips[b],
        })
        .collect();

    ChipDistribution {
        bins,
        max_chips,
        ref_price,
        bin_width,
        cost_center,
        cost_low,
        cost_high,
        cbw,
        ckdp,
        profit_ratio,
        lookback_days: ref_idx - start + 1,
        avg_turnover_rate: if active_days > 0 {
            turnover_sum / active_days as f64
        } else {
            0.0
        },
    }
}

/// Estimate a per-day turnover rate from volume. The data source doesn't carry
/// a real turnover ratio, so we re-anchor each day against a rolling median
/// volume baseline and use square-root scaling (configurable via
/// `volume_ratio_exponent`) to keep persistent regime shifts from immediately
/// saturating the turnover cap.
fn build_effective_turnovers(
    candles: &[Candle],
    ref_idx: usize,
    settings: &ChipSettings,
) -> Vec<f64> {
    let overall_median_volume = median(
        candles[..=ref_idx]
            .iter()
            .filter_map(|c| (c.volume > 0.0).then_some(c.volume))
            .collect(),
    )
    .unwrap_or(0.0);

    let baseline_window = settings.turnover_baseline_days.max(1);
    let exponent = settings.volume_ratio_exponent;
    let fallback = settings.fallback_turnover_rate;
    let min_rate = settings.min_turnover_rate;
    let max_rate = settings.max_turnover_rate.max(min_rate);

    (0..=ref_idx)
        .map(|i| {
            let c = &candles[i];
            if c.volume <= 0.0 {
                0.0
            } else {
                let start = i.saturating_sub(baseline_window.saturating_sub(1));
                let baseline_volume = median(
                    candles[start..=i]
                        .iter()
                        .filter_map(|c| (c.volume > 0.0).then_some(c.volume))
                        .collect(),
                )
                .filter(|value| *value > 0.0)
                .unwrap_or(overall_median_volume);

                if baseline_volume > 0.0 {
                    (c.volume / baseline_volume)
                        .powf(exponent)
                        .mul_add(fallback, 0.0)
                        .clamp(min_rate, max_rate)
                } else {
                    fallback
                }
            }
        })
        .collect()
}

fn determine_start(
    candles: &[Candle],
    effective_turnovers: &[f64],
    ref_idx: usize,
    settings: &ChipSettings,
) -> usize {
    let mut start = ref_idx;
    let mut residual_mass = 1.0;
    let mut trading_days = 0usize;
    let max_lookback = settings.max_lookback.max(1);
    let min_lookback = settings.min_lookback.min(max_lookback);
    let cutoff = settings.residual_mass_cutoff;

    for i in (0..=ref_idx).rev() {
        if ref_idx - i + 1 > max_lookback {
            return i + 1;
        }

        start = i;
        let c = &candles[i];
        let turnover_rate = effective_turnovers[i];
        if c.volume <= 0.0 || turnover_rate <= 0.0 {
            continue;
        }

        trading_days += 1;
        residual_mass *= 1.0 - turnover_rate;

        if trading_days >= min_lookback && residual_mass <= cutoff {
            break;
        }
    }

    start
}

fn distribute_new_chips(
    chips: &mut [f64],
    candle: &Candle,
    price_min: f64,
    price_max: f64,
    bin_width: f64,
    new_chips: f64,
    settings: &ChipSettings,
) {
    let day_low = candle.low.min(candle.high);
    let day_high = candle.low.max(candle.high);

    // Single-price成交日直接落到一个价位 bin。
    if (day_high - day_low).abs() < f64::EPSILON {
        let trade_price = candle.close.clamp(price_min, price_max);
        let bin = price_to_bin(trade_price, price_min, bin_width, chips.len());
        chips[bin] += new_chips;
        return;
    }

    let lo_bin = price_to_bin(day_low, price_min, bin_width, chips.len());
    let hi_bin = price_to_bin(day_high, price_min, bin_width, chips.len());
    let range = (day_high - day_low).max(bin_width);
    let body_low = candle.open.min(candle.close).clamp(day_low, day_high);
    let body_high = candle.open.max(candle.close).clamp(day_low, day_high);
    let open = candle.open.clamp(day_low, day_high);
    let close = candle.close.clamp(day_low, day_high);
    let amount_anchor = infer_average_trade_price(candle, day_low, day_high);
    let price_anchor = amount_anchor
        .unwrap_or_else(|| ((day_low + day_high + close * 2.0) / 4.0).clamp(day_low, day_high));
    let directional_bias = ((close - open) / range).abs().clamp(0.0, 1.0);
    let body_fade = (range * 0.25).max(bin_width);
    let open_span = (range * 0.45).max(bin_width);
    let close_span = (range * 0.30).max(bin_width);
    let price_anchor_span = if amount_anchor.is_some() {
        (range * 0.16).max(bin_width)
    } else {
        (range * 0.22).max(bin_width)
    };
    let wick_focus = if close >= open { body_low } else { body_high };
    let wick_span = (range * 0.20).max(bin_width);
    let baseline_weight = if amount_anchor.is_some() { 0.04 } else { 0.06 };
    let open_weight = if amount_anchor.is_some() {
        0.10 - 0.03 * directional_bias
    } else {
        0.12 - 0.04 * directional_bias
    };
    let close_weight = if amount_anchor.is_some() {
        0.18 + 0.08 * directional_bias
    } else {
        0.24 + 0.12 * directional_bias
    };
    let price_anchor_weight =
        if amount_anchor.is_some() { 0.34 } else { 0.22 } * settings.anchor_strength.max(0.0);
    let body_region_weight = if amount_anchor.is_some() { 0.24 } else { 0.26 };
    let wick_weight = if amount_anchor.is_some() { 0.08 } else { 0.10 };

    let mut weights = Vec::with_capacity(hi_bin.saturating_sub(lo_bin) + 1);
    let mut weight_sum = 0.0;

    for b in lo_bin..=hi_bin {
        let bin_price = price_min + (b as f64 + 0.5) * bin_width;
        let w = baseline_weight
            + open_weight * triangular_weight(bin_price, open, open_span)
            + close_weight * triangular_weight(bin_price, close, close_span)
            + price_anchor_weight * triangular_weight(bin_price, price_anchor, price_anchor_span)
            + body_region_weight * body_weight(bin_price, body_low, body_high, body_fade)
            + wick_weight * triangular_weight(bin_price, wick_focus, wick_span);
        weights.push(w);
        weight_sum += w;
    }

    if weight_sum <= 0.0 {
        return;
    }

    for (j, b) in (lo_bin..=hi_bin).enumerate() {
        chips[b] += new_chips * weights[j] / weight_sum;
    }
}

fn derive_price_range(window: &[Candle], ref_price: f64) -> (f64, f64) {
    let mut price_min = f64::INFINITY;
    let mut price_max = f64::NEG_INFINITY;

    for c in window {
        price_min = price_min.min(c.low.min(c.high));
        price_max = price_max.max(c.low.max(c.high));
    }

    if !price_min.is_finite() || !price_max.is_finite() {
        let pad = ref_price.abs().max(1.0) * 0.005;
        return (ref_price - pad, ref_price + pad);
    }

    if price_max <= price_min {
        let pad = ref_price.abs().max(1.0) * 0.005;
        price_min -= pad;
        price_max += pad;
    }

    let span = price_max - price_min;
    let pad = (span * 0.02).max(ref_price.abs().max(1.0) * 0.002);

    (price_min - pad, price_max + pad)
}

fn choose_bin_count(
    window_len: usize,
    ref_price: f64,
    price_range: f64,
    settings: &ChipSettings,
) -> usize {
    let bins_from_window = (window_len as f64 * 1.4).round() as usize;
    let relative_range = price_range / ref_price.abs().max(1.0);
    let bins_from_range = if relative_range < 0.08 {
        96
    } else if relative_range < 0.18 {
        128
    } else if relative_range < 0.35 {
        160
    } else {
        192
    };

    let min_bins = settings.min_bins.max(1);
    let max_bins = settings.max_bins.max(min_bins);
    bins_from_window
        .max(bins_from_range)
        .clamp(min_bins, max_bins)
}

/// Walk the cumulative chip mass and return the prices at which it first
/// crosses `q_low` and `q_high`. When all chips are zero the band collapses to
/// (0, 0), matching the previous "no chips → no cost line" behaviour.
fn cost_band_from_quantiles(
    chips: &[f64],
    price_min: f64,
    bin_width: f64,
    q_low: f64,
    q_high: f64,
) -> (f64, f64) {
    let total: f64 = chips.iter().sum();
    if total <= 0.0 {
        return (0.0, 0.0);
    }

    let target_low = q_low * total;
    let target_high = q_high * total;
    let mut cumulative = 0.0;
    let mut cost_low: Option<f64> = None;
    let mut cost_high: Option<f64> = None;

    for (b, mass) in chips.iter().enumerate() {
        let price = price_min + (b as f64 + 0.5) * bin_width;
        cumulative += mass;
        if cost_low.is_none() && cumulative >= target_low {
            cost_low = Some(price);
        }
        if cumulative >= target_high {
            cost_high = Some(price);
            break;
        }
    }

    // Fall back to the last non-empty bin for the upper edge if we never
    // crossed the high quantile (numerical edge case when mass is concentrated
    // in the very last bin).
    let last_price = chips
        .iter()
        .enumerate()
        .rev()
        .find(|(_, m)| **m > 0.0)
        .map(|(b, _)| price_min + (b as f64 + 0.5) * bin_width)
        .unwrap_or(0.0);

    (cost_low.unwrap_or(0.0), cost_high.unwrap_or(last_price))
}

fn compute_profit_ratio(chips: &[f64], price_min: f64, bin_width: f64, ref_price: f64) -> f64 {
    let total: f64 = chips.iter().sum();
    if total <= 0.0 || bin_width <= 0.0 {
        return 0.0;
    }

    let mut acc = 0.0;
    for (b, mass) in chips.iter().enumerate() {
        let bin_low = price_min + b as f64 * bin_width;
        let bin_high = bin_low + bin_width;
        if bin_high <= ref_price {
            acc += mass;
        } else if bin_low >= ref_price {
            break;
        } else {
            // Linear partial credit for the bin straddling ref_price.
            let frac = ((ref_price - bin_low) / bin_width).clamp(0.0, 1.0);
            acc += mass * frac;
            break;
        }
    }
    (acc / total).clamp(0.0, 1.0)
}

fn triangular_weight(price: f64, center: f64, span: f64) -> f64 {
    if span <= 0.0 {
        return 0.0;
    }

    (1.0 - ((price - center).abs() / span)).max(0.0)
}

fn body_weight(price: f64, body_low: f64, body_high: f64, fade: f64) -> f64 {
    if price >= body_low && price <= body_high {
        return 1.0;
    }

    if fade <= 0.0 {
        return 0.0;
    }

    let dist = if price < body_low {
        body_low - price
    } else {
        price - body_high
    };

    (1.0 - dist / fade).max(0.0)
}

fn median(mut values: Vec<f64>) -> Option<f64> {
    if values.is_empty() {
        return None;
    }

    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mid = values.len() / 2;

    if values.len().is_multiple_of(2) {
        Some((values[mid - 1] + values[mid]) / 2.0)
    } else {
        Some(values[mid])
    }
}

fn infer_average_trade_price(candle: &Candle, day_low: f64, day_high: f64) -> Option<f64> {
    let amount = candle.amount?;
    if amount <= 0.0 || candle.volume <= 0.0 {
        return None;
    }

    let typical_price = ((candle.open + candle.close + day_low + day_high) / 4.0)
        .abs()
        .max(1e-6);
    let tolerance = ((day_high - day_low) * 0.05).max(typical_price * 0.01);

    [1.0, 10.0, 100.0, 1000.0, 10000.0]
        .into_iter()
        .filter_map(|scale| {
            let avg_price = amount / candle.volume / scale;
            if !avg_price.is_finite() || avg_price <= 0.0 {
                return None;
            }

            if avg_price < day_low - tolerance || avg_price > day_high + tolerance {
                return None;
            }

            Some((
                ((avg_price / typical_price).ln()).abs(),
                avg_price.clamp(day_low, day_high),
            ))
        })
        .min_by(|lhs, rhs| lhs.0.total_cmp(&rhs.0))
        .map(|(_, avg_price)| avg_price)
}

fn price_to_bin(price: f64, price_min: f64, bin_width: f64, num_bins: usize) -> usize {
    if bin_width <= 0.0 || num_bins == 0 {
        return 0;
    }

    (((price - price_min) / bin_width).floor() as isize).clamp(0, num_bins as isize - 1) as usize
}

#[cfg(test)]
mod tests {
    use super::calculate_chip_distribution;
    use crate::data::{Candle, ChipSettings};

    fn settings() -> ChipSettings {
        ChipSettings::default()
    }

    fn candle(open: f64, close: f64, high: f64, low: f64, volume: f64) -> Candle {
        candle_with_amount(open, close, high, low, volume, None)
    }

    fn candle_with_amount(
        open: f64,
        close: f64,
        high: f64,
        low: f64,
        volume: f64,
        amount: Option<f64>,
    ) -> Candle {
        Candle {
            timestamp: "2026-01-01".to_string(),
            open,
            close,
            high,
            low,
            volume,
            amount,
        }
    }

    #[test]
    fn single_price_candle_still_adds_chips() {
        let candles = vec![candle(10.0, 10.0, 10.0, 10.0, 100.0)];
        let dist = calculate_chip_distribution(&candles, 0, &settings());

        let nonzero_bins = dist.bins.iter().filter(|bin| bin.chips > 0.0).count();
        let total_chips: f64 = dist.bins.iter().map(|bin| bin.chips).sum();

        assert_eq!(nonzero_bins, 1);
        assert!((total_chips - 1.0).abs() < 1e-6);
        assert!(dist.max_chips > 0.0);
    }

    #[test]
    fn average_turnover_rate_matches_volume_scaling() {
        // Equal volumes → every day uses the fallback rate, so the average
        // reported back should equal that fallback.
        let candles = vec![
            candle(10.0, 11.0, 11.0, 10.0, 100.0),
            candle(11.0, 12.0, 12.0, 11.0, 100.0),
        ];
        let s = settings();
        let dist = calculate_chip_distribution(&candles, 1, &s);

        assert!((dist.avg_turnover_rate - s.fallback_turnover_rate).abs() < 1e-6);
    }

    #[test]
    fn high_volume_day_raises_turnover_estimate() {
        let candles = vec![
            candle(10.0, 11.0, 11.0, 10.0, 100.0),
            candle(11.0, 12.0, 12.0, 11.0, 100.0),
            candle(11.0, 12.0, 12.0, 11.0, 100.0),
            candle(11.0, 12.0, 12.0, 11.0, 1000.0),
        ];
        let s = settings();
        let dist = calculate_chip_distribution(&candles, candles.len() - 1, &s);

        assert!(dist.avg_turnover_rate > s.fallback_turnover_rate);
    }

    #[test]
    fn chip_mass_is_normalized_after_multiple_days() {
        let candles = vec![
            candle(10.0, 11.0, 11.5, 9.8, 100.0),
            candle(11.0, 10.5, 11.2, 10.0, 100.0),
            candle(10.5, 10.8, 11.0, 10.2, 100.0),
        ];
        let dist = calculate_chip_distribution(&candles, 2, &settings());
        let total_chips: f64 = dist.bins.iter().map(|bin| bin.chips).sum();

        assert!((total_chips - 1.0).abs() < 1e-6);
    }

    #[test]
    fn bullish_candle_biases_distribution_toward_close() {
        let candles = vec![candle(10.0, 14.0, 15.0, 9.0, 100.0)];
        let dist = calculate_chip_distribution(&candles, 0, &settings());
        let open_bin = dist
            .bins
            .iter()
            .min_by(|a, b| {
                (a.price - 10.0)
                    .abs()
                    .partial_cmp(&(b.price - 10.0).abs())
                    .unwrap()
            })
            .unwrap();
        let close_bin = dist
            .bins
            .iter()
            .min_by(|a, b| {
                (a.price - 14.0)
                    .abs()
                    .partial_cmp(&(b.price - 14.0).abs())
                    .unwrap()
            })
            .unwrap();

        assert!(close_bin.chips > open_bin.chips);
    }

    #[test]
    fn sustained_volume_regime_shift_reanchors_turnover_baseline() {
        let mut candles = (0..120)
            .map(|_| candle(10.0, 10.5, 10.8, 9.8, 100.0))
            .collect::<Vec<_>>();
        candles.extend((0..80).map(|_| candle(10.0, 10.5, 10.8, 9.8, 1000.0)));

        let s = settings();
        let dist = calculate_chip_distribution(&candles, candles.len() - 1, &s);

        assert!(dist.avg_turnover_rate < s.fallback_turnover_rate * 1.5);
    }

    #[test]
    fn amount_anchor_moves_cost_center_toward_average_trade_price() {
        let s = settings();
        let baseline = calculate_chip_distribution(&[candle(10.0, 14.0, 15.0, 9.0, 100.0)], 0, &s);
        let anchored = calculate_chip_distribution(
            &[candle_with_amount(
                10.0,
                14.0,
                15.0,
                9.0,
                100.0,
                Some(102_000.0),
            )],
            0,
            &s,
        );
        let target_avg_price = 10.2;

        assert!(
            (anchored.cost_center - target_avg_price).abs()
                < (baseline.cost_center - target_avg_price).abs()
        );
    }

    #[test]
    fn cost_center_is_weighted_average_of_chips() {
        let candles = vec![
            candle(10.0, 11.0, 11.5, 9.8, 100.0),
            candle(11.0, 10.5, 11.2, 10.0, 100.0),
            candle(10.5, 10.8, 11.0, 10.2, 100.0),
        ];
        let dist = calculate_chip_distribution(&candles, 2, &settings());
        let expected: f64 = dist.bins.iter().map(|b| b.price * b.chips).sum();
        assert!((dist.cost_center - expected).abs() < 1e-9);
        assert!(dist.cost_center >= 9.5 && dist.cost_center <= 12.0);
    }

    #[test]
    fn cbw_reflects_chip_price_spread() {
        let candles = vec![candle(10.0, 14.0, 15.0, 9.0, 100.0)];
        let dist = calculate_chip_distribution(&candles, 0, &settings());
        assert!(dist.cbw > 0.0);
        let expected = (dist.cost_high - dist.cost_low) / dist.cost_low * 100.0;
        assert!((dist.cbw - expected).abs() < 1e-6);
    }

    #[test]
    fn ckdp_reflects_relative_price_position() {
        // close=14, range roughly 9..15 → CKDP should be well above 50
        let candles = vec![candle(10.0, 14.0, 15.0, 9.0, 100.0)];
        let dist = calculate_chip_distribution(&candles, 0, &settings());
        let expected = (dist.ref_price - dist.cost_low) / (dist.cost_high - dist.cost_low) * 100.0;
        assert!((dist.ckdp - expected).abs() < 1e-6);
        assert!(dist.ckdp > 50.0);
    }

    #[test]
    fn bin_count_scales_with_longer_window() {
        let candles = (0..240)
            .map(|i| candle(10.0, 10.0 + i as f64 * 0.02, 15.0, 8.5, 100.0))
            .collect::<Vec<_>>();
        let dist = calculate_chip_distribution(&candles, candles.len() - 1, &settings());

        assert!(dist.bins.len() > 150);
    }

    /// With baseline_weight spreading a thin layer of chips across the entire
    /// intraday range, the legacy "any non-zero bin" cost band would put
    /// `cost_low`/`cost_high` essentially at price_min/price_max — making CBW
    /// huge regardless of where mass actually sits. Quantile-based bands
    /// should be much tighter.
    #[test]
    fn quantile_cost_band_is_tighter_than_full_range() {
        let candles = vec![candle(10.0, 14.0, 15.0, 9.0, 100.0)];
        let dist = calculate_chip_distribution(&candles, 0, &settings());

        let bin_min = dist.bins.first().map(|b| b.price).unwrap();
        let bin_max = dist.bins.last().map(|b| b.price).unwrap();
        let full_span = bin_max - bin_min;
        let band_span = dist.cost_high - dist.cost_low;

        assert!(band_span > 0.0);
        assert!(band_span < full_span * 0.95);
    }

    #[test]
    fn profit_ratio_reflects_chips_below_ref_price() {
        // close=14 is near the top of the range, so most chip mass should sit
        // below it → high profit ratio.
        let bullish =
            calculate_chip_distribution(&[candle(10.0, 14.0, 15.0, 9.0, 100.0)], 0, &settings());
        assert!(bullish.profit_ratio > 0.5);
        assert!(bullish.profit_ratio <= 1.0);

        // Conversely, close near the bottom → low profit ratio.
        let bearish =
            calculate_chip_distribution(&[candle(14.0, 10.0, 15.0, 9.0, 100.0)], 0, &settings());
        assert!(bearish.profit_ratio < 0.5);
        assert!(bearish.profit_ratio >= 0.0);
    }

    #[test]
    fn anchor_strength_zero_relaxes_amount_anchor_pull() {
        let strong = ChipSettings::default();
        let weak = ChipSettings {
            anchor_strength: 0.0,
            ..ChipSettings::default()
        };

        // amount → avg trade price ≈ 10.2 (close to day_low). With strength=1
        // we should land closer to that anchor than with strength=0.
        let candles = [candle_with_amount(
            10.0,
            14.0,
            15.0,
            9.0,
            100.0,
            Some(102_000.0),
        )];

        let strong_dist = calculate_chip_distribution(&candles, 0, &strong);
        let weak_dist = calculate_chip_distribution(&candles, 0, &weak);

        let target = 10.2;
        assert!((strong_dist.cost_center - target).abs() < (weak_dist.cost_center - target).abs());
    }

    #[test]
    fn lookback_respects_max_setting() {
        // 50 dummy candles + max_lookback=10 should clip lookback_days to 10.
        let candles = (0..50)
            .map(|_| candle(10.0, 10.5, 10.8, 9.8, 100.0))
            .collect::<Vec<_>>();
        let s = ChipSettings {
            max_lookback: 10,
            min_lookback: 1,
            ..ChipSettings::default()
        };
        let dist = calculate_chip_distribution(&candles, candles.len() - 1, &s);
        assert!(dist.lookback_days <= 10);
    }
}
