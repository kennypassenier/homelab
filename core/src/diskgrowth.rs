//! feat-overview-12: disk-growth prediction, from Prometheus history.
//!
//! A filesystem's percent-used over a window of history fits a line (least
//! squares) and, separately, a robust line (Theil-Sen: the median of every
//! pairwise slope, which a single bad sample or a one-off cleanup cannot
//! move the way a mean can). Both are reported; the robust one is what a
//! warning is judged on, because a disk-growth curve is exactly the kind of
//! series that gets one enormous outlier (a log file filling the disk for an
//! hour, then rotated) that a plain least-squares fit takes far too
//! seriously.
//!
//! Pure math, no I/O: the caller reads the series from Prometheus and hands
//! it here as `(unix_seconds, percent_used)` pairs.

use serde::Serialize;

/// One filesystem's growth, fitted from its history.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GrowthFit {
    /// Percent used per day, by the robust (Theil-Sen) fit. Negative =
    /// shrinking.
    pub pct_per_day_robust: f64,
    /// The same, by plain least squares — shown beside the robust number so
    /// a large gap between them is visible rather than silently decided.
    pub pct_per_day_linear: f64,
    /// The most recent point's percent used, read back out of the series
    /// (not predicted) so the caller can show "62% now, …".
    pub pct_now: f64,
    /// Days until the robust fit crosses 100%, from the last point in the
    /// series. None when it is not growing (slope <= 0) or there are too
    /// few points to fit (fewer than 2 distinct times).
    pub days_to_full: Option<f64>,
}

/// Least-squares slope and intercept of `y = a + b*x`, in the series' own
/// units (seconds, percent). None when every `x` is the same (a vertical
/// fit is undefined) or the series is empty.
fn least_squares(points: &[(f64, f64)]) -> Option<(f64, f64)> {
    let n = points.len() as f64;
    if points.len() < 2 {
        return None;
    }
    let mean_x = points.iter().map(|(x, _)| x).sum::<f64>() / n;
    let mean_y = points.iter().map(|(_, y)| y).sum::<f64>() / n;
    let mut num = 0.0;
    let mut den = 0.0;
    for (x, y) in points {
        num += (x - mean_x) * (y - mean_y);
        den += (x - mean_x) * (x - mean_x);
    }
    if den == 0.0 {
        return None;
    }
    let b = num / den;
    let a = mean_y - b * mean_x;
    Some((a, b))
}

/// The Theil-Sen slope: the median of every pairwise slope `(y2-y1)/(x2-x1)`
/// over point pairs with distinct `x`. Robust to a single outlier point
/// (median, not mean) unlike [`least_squares`]. None when fewer than two
/// points have distinct `x`.
fn theil_sen_slope(points: &[(f64, f64)]) -> Option<f64> {
    let mut slopes: Vec<f64> = Vec::new();
    for i in 0..points.len() {
        for j in (i + 1)..points.len() {
            let (x1, y1) = points[i];
            let (x2, y2) = points[j];
            let dx = x2 - x1;
            if dx != 0.0 {
                slopes.push((y2 - y1) / dx);
            }
        }
    }
    if slopes.is_empty() {
        return None;
    }
    slopes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = slopes.len() / 2;
    Some(if slopes.len().is_multiple_of(2) {
        (slopes[mid - 1] + slopes[mid]) / 2.0
    } else {
        slopes[mid]
    })
}

/// Fit `points` (unix seconds, percent used 0..100, any order, duplicates
/// allowed) and predict when the robust line reaches 100%. `points` with
/// fewer than two distinct timestamps, or a non-finite value, fit nothing.
pub fn fit(points: &[(f64, f64)]) -> Option<GrowthFit> {
    let clean: Vec<(f64, f64)> = points
        .iter()
        .copied()
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .collect();
    if clean.len() < 2 {
        return None;
    }
    let mut sorted = clean.clone();
    sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let (_last_x, last_y) = *sorted.last()?;
    let (_, b_linear) = least_squares(&clean)?;
    let b_robust = theil_sen_slope(&clean)?;
    const DAY_S: f64 = 86_400.0;
    let days_to_full = if b_robust > 0.0 {
        Some(((100.0 - last_y) / b_robust / DAY_S).max(0.0))
    } else {
        None
    };
    Some(GrowthFit {
        pct_per_day_robust: b_robust * DAY_S,
        pct_per_day_linear: b_linear * DAY_S,
        pct_now: last_y,
        days_to_full,
    })
}

/// feat-overview-12: whether a fit is close enough to full to warn about,
/// within `within_days` (the caller's chosen N; [`DEFAULT_WARN_DAYS`] when
/// nothing else is configured).
pub fn is_warning(fit: &GrowthFit, within_days: f64) -> bool {
    fit.days_to_full.is_some_and(|d| d <= within_days)
}

/// The default warning window, days (Kenny's "within N days" with no N
/// named yet): two weeks gives time to act without crying wolf over a
/// filesystem that fills in six months.
pub const DEFAULT_WARN_DAYS: f64 = 14.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steady_growth_predicts_the_day_it_crosses_100() {
        // 1% a day, starting at 50%, for 10 days.
        let points: Vec<(f64, f64)> = (0..10)
            .map(|d| (d as f64 * 86_400.0, 50.0 + d as f64))
            .collect();
        let fit = fit(&points).expect("fits");
        assert!((fit.pct_per_day_robust - 1.0).abs() < 1e-6);
        assert!((fit.pct_now - 59.0).abs() < 1e-9);
        // At day 9 (last point) it is at 59%; 41 points to go at 1%/day.
        let days = fit.days_to_full.expect("growing, so it predicts");
        assert!((days - 41.0).abs() < 1e-6, "got {days}");
    }

    #[test]
    fn shrinking_disk_predicts_nothing() {
        let points: Vec<(f64, f64)> = (0..10)
            .map(|d| (d as f64 * 86_400.0, 80.0 - d as f64))
            .collect();
        let fit = fit(&points).expect("fits");
        assert!(fit.pct_per_day_robust < 0.0);
        assert!(fit.days_to_full.is_none());
    }

    #[test]
    fn flat_disk_predicts_nothing() {
        let points: Vec<(f64, f64)> = (0..5).map(|d| (d as f64 * 86_400.0, 42.0)).collect();
        let fit = fit(&points).expect("fits");
        assert_eq!(fit.pct_per_day_robust, 0.0);
        assert!(fit.days_to_full.is_none());
    }

    #[test]
    fn one_outlier_does_not_move_the_robust_fit_much() {
        // Steady 0.5%/day, but one point spikes to 95% (a one-off log
        // filling the disk, then cleaned up) — the robust fit should stay
        // close to 0.5, where the linear one is dragged upward.
        let mut points: Vec<(f64, f64)> = (0..20)
            .map(|d| (d as f64 * 86_400.0, 30.0 + d as f64 * 0.5))
            .collect();
        points[10].1 = 95.0;
        let fit = fit(&points).expect("fits");
        assert!(
            (fit.pct_per_day_robust - 0.5).abs() < 0.3,
            "robust slope {} should stay near 0.5",
            fit.pct_per_day_robust
        );
        assert!(
            fit.pct_per_day_linear > fit.pct_per_day_robust,
            "the single spike should drag the linear fit up more than the robust one"
        );
    }

    #[test]
    fn too_few_points_fits_nothing() {
        assert!(fit(&[]).is_none());
        assert!(fit(&[(1.0, 2.0)]).is_none());
    }

    #[test]
    fn same_timestamp_twice_fits_nothing() {
        assert!(fit(&[(1.0, 2.0), (1.0, 3.0)]).is_none());
    }

    #[test]
    fn warning_respects_the_chosen_window() {
        let f = GrowthFit {
            pct_per_day_robust: 1.0,
            pct_per_day_linear: 1.0,
            pct_now: 90.0,
            days_to_full: Some(10.0),
        };
        assert!(is_warning(&f, 14.0));
        assert!(is_warning(&f, 10.0));
        assert!(!is_warning(&f, 9.9));
        let not_growing = GrowthFit {
            days_to_full: None,
            ..f
        };
        assert!(!is_warning(&not_growing, 1000.0));
    }
}
