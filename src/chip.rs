use crate::data::Candle;

pub const FALLBACK_TURNOVER_RATE: f64 = 0.03;
const MIN_TURNOVER_RATE: f64 = 0.001;
const MAX_TURNOVER_RATE: f64 = 0.30;
const TURNOVER_BASELINE_DAYS: usize = 40;
const VOLUME_RATIO_EXPONENT: f64 = 0.5;
const MIN_LOOKBACK: usize = 60;
const MAX_LOOKBACK: usize = 480;
const RESIDUAL_MASS_CUTOFF: f64 = 0.03;
const MIN_BINS: usize = 96;
const MAX_BINS: usize = 240;

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
    pub lookback_days: usize,
    pub avg_turnover_rate: f64,
}

pub struct ChipCache {
    pub ref_idx: usize,
    pub dist: ChipDistribution,
}

pub fn calculate_chip_distribution(candles: &[Candle], ref_idx: usize) -> ChipDistribution {
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
        lookback_days: 0,
        avg_turnover_rate: 0.0,
    };

    if candles.is_empty() || ref_idx >= candles.len() {
        return empty();
    }

    let ref_price = candles[ref_idx].close;
    let effective_turnovers = build_effective_turnovers(candles, ref_idx);
    let start = determine_start(candles, &effective_turnovers, ref_idx);
    let window = &candles[start..=ref_idx];

    let (price_min, price_max) = derive_price_range(window, ref_price);
    let price_range = price_max - price_min;
    let bin_count = choose_bin_count(window.len(), ref_price, price_range);
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

        distribute_new_chips(&mut chips, c, price_min, price_max, bin_width, new_chips);
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

    // Cost price boundaries (lowest / highest bins with chips)
    let cost_low = (0..bin_count)
        .find(|&b| chips[b] > 0.0)
        .map(|b| price_min + (b as f64 + 0.5) * bin_width)
        .unwrap_or(0.0);
    let cost_high = (0..bin_count)
        .rev()
        .find(|&b| chips[b] > 0.0)
        .map(|b| price_min + (b as f64 + 0.5) * bin_width)
        .unwrap_or(0.0);

    // CBW = (最高成本价 - 最低成本价) / 最低成本价 × 100%
    let cbw = if cost_low > 0.0 {
        (cost_high - cost_low) / cost_low * 100.0
    } else {
        0.0
    };

    // CKDP = (当前价 - 最低成本价) / (最高成本价 - 最低成本价) × 100%
    let spread = cost_high - cost_low;
    let ckdp = if spread > 0.0 {
        (ref_price - cost_low) / spread * 100.0
    } else {
        0.0
    };

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
/// volume baseline and use square-root scaling to keep persistent regime
/// shifts from immediately saturating the turnover cap.
fn build_effective_turnovers(candles: &[Candle], ref_idx: usize) -> Vec<f64> {
    let overall_median_volume = median(
        candles[..=ref_idx]
            .iter()
            .filter_map(|c| (c.volume > 0.0).then_some(c.volume))
            .collect(),
    )
    .unwrap_or(0.0);

    (0..=ref_idx)
        .map(|i| {
            let c = &candles[i];
            if c.volume <= 0.0 {
                0.0
            } else {
                let start = i.saturating_sub(TURNOVER_BASELINE_DAYS.saturating_sub(1));
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
                        .powf(VOLUME_RATIO_EXPONENT)
                        .mul_add(FALLBACK_TURNOVER_RATE, 0.0)
                        .clamp(MIN_TURNOVER_RATE, MAX_TURNOVER_RATE)
                } else {
                    FALLBACK_TURNOVER_RATE
                }
            }
        })
        .collect()
}

fn determine_start(candles: &[Candle], effective_turnovers: &[f64], ref_idx: usize) -> usize {
    let mut start = ref_idx;
    let mut residual_mass = 1.0;
    let mut trading_days = 0usize;

    for i in (0..=ref_idx).rev() {
        if ref_idx - i + 1 > MAX_LOOKBACK {
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

        if trading_days >= MIN_LOOKBACK && residual_mass <= RESIDUAL_MASS_CUTOFF {
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
    let price_anchor_weight = if amount_anchor.is_some() { 0.34 } else { 0.22 };
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

fn choose_bin_count(window_len: usize, ref_price: f64, price_range: f64) -> usize {
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

    bins_from_window
        .max(bins_from_range)
        .clamp(MIN_BINS, MAX_BINS)
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
    use super::{calculate_chip_distribution, FALLBACK_TURNOVER_RATE};
    use crate::data::Candle;

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
        let dist = calculate_chip_distribution(&candles, 0);

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
        let dist = calculate_chip_distribution(&candles, 1);

        assert!((dist.avg_turnover_rate - FALLBACK_TURNOVER_RATE).abs() < 1e-6);
    }

    #[test]
    fn high_volume_day_raises_turnover_estimate() {
        // A one-off spike should still raise the inferred turnover above the
        // fallback baseline even without true float-share data.
        let candles = vec![
            candle(10.0, 11.0, 11.0, 10.0, 100.0),
            candle(11.0, 12.0, 12.0, 11.0, 100.0),
            candle(11.0, 12.0, 12.0, 11.0, 100.0),
            candle(11.0, 12.0, 12.0, 11.0, 1000.0),
        ];
        let dist = calculate_chip_distribution(&candles, candles.len() - 1);

        assert!(dist.avg_turnover_rate > FALLBACK_TURNOVER_RATE);
    }

    #[test]
    fn chip_mass_is_normalized_after_multiple_days() {
        let candles = vec![
            candle(10.0, 11.0, 11.5, 9.8, 100.0),
            candle(11.0, 10.5, 11.2, 10.0, 100.0),
            candle(10.5, 10.8, 11.0, 10.2, 100.0),
        ];
        let dist = calculate_chip_distribution(&candles, 2);
        let total_chips: f64 = dist.bins.iter().map(|bin| bin.chips).sum();

        assert!((total_chips - 1.0).abs() < 1e-6);
    }

    #[test]
    fn bullish_candle_biases_distribution_toward_close() {
        let candles = vec![candle(10.0, 14.0, 15.0, 9.0, 100.0)];
        let dist = calculate_chip_distribution(&candles, 0);
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

        let dist = calculate_chip_distribution(&candles, candles.len() - 1);

        assert!(dist.avg_turnover_rate < FALLBACK_TURNOVER_RATE * 1.5);
    }

    #[test]
    fn amount_anchor_moves_cost_center_toward_average_trade_price() {
        let baseline = calculate_chip_distribution(&[candle(10.0, 14.0, 15.0, 9.0, 100.0)], 0);
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
        let dist = calculate_chip_distribution(&candles, 2);
        let expected: f64 = dist.bins.iter().map(|b| b.price * b.chips).sum();
        assert!((dist.cost_center - expected).abs() < 1e-9);
        // Cost center should be within the price range of the candles
        assert!(dist.cost_center >= 9.5 && dist.cost_center <= 12.0);
    }

    #[test]
    fn cbw_reflects_chip_price_spread() {
        let candles = vec![candle(10.0, 14.0, 15.0, 9.0, 100.0)];
        let dist = calculate_chip_distribution(&candles, 0);
        assert!(dist.cbw > 0.0);
        let expected = (dist.cost_high - dist.cost_low) / dist.cost_low * 100.0;
        assert!((dist.cbw - expected).abs() < 1e-6);
    }

    #[test]
    fn ckdp_reflects_relative_price_position() {
        // close=14, range roughly 9..15 → CKDP should be well above 50
        let candles = vec![candle(10.0, 14.0, 15.0, 9.0, 100.0)];
        let dist = calculate_chip_distribution(&candles, 0);
        let expected = (dist.ref_price - dist.cost_low) / (dist.cost_high - dist.cost_low) * 100.0;
        assert!((dist.ckdp - expected).abs() < 1e-6);
        assert!(dist.ckdp > 50.0);
    }

    #[test]
    fn bin_count_scales_with_longer_window() {
        let candles = (0..240)
            .map(|i| candle(10.0, 10.0 + i as f64 * 0.02, 15.0, 8.5, 100.0))
            .collect::<Vec<_>>();
        let dist = calculate_chip_distribution(&candles, candles.len() - 1);

        assert!(dist.bins.len() > 150);
    }
}
