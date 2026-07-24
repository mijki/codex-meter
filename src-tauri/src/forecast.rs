use chrono::{DateTime, Duration, Utc};

use crate::domain::{
    Accuracy, BurnRatePoint, ConsumptionTrajectoryPoint, ForecastConfidence, ForecastModel,
    ForecastQuality, ForecastRisk, QuotaForecast, QuotaObservation, TelemetryState,
    TelemetryStatus,
};

const MIN_FORECAST_COVERAGE_MINUTES: f64 = 15.0;
const DISCONTINUITY_DROP_TOLERANCE: f64 = 2.0;
const MAX_CONTINUITY_GAP_MINUTES: f64 = 120.0;
const EWMA_ALPHA: f64 = 0.35;

#[derive(Debug, Clone)]
struct ParsedObservation {
    at: DateTime<Utc>,
    used: f64,
}

pub struct ForecastInput {
    pub bucket_id: String,
    pub bucket_name: String,
    pub window_label: String,
    pub generated_at: DateTime<Utc>,
    pub resets_at: Option<DateTime<Utc>>,
    pub observations: Vec<QuotaObservation>,
}

pub fn calculate_forecast(input: ForecastInput) -> QuotaForecast {
    let mut parsed = input
        .observations
        .into_iter()
        .filter_map(|observation| {
            let at = DateTime::parse_from_rfc3339(&observation.observed_at)
                .ok()?
                .with_timezone(&Utc);
            (0.0..=100.0)
                .contains(&observation.used_percent)
                .then_some(ParsedObservation {
                    at,
                    used: observation.used_percent,
                })
        })
        .collect::<Vec<_>>();
    parsed.sort_by_key(|observation| observation.at);
    parsed.dedup_by_key(|observation| observation.at);

    let (observations, discontinuity_reason) = latest_continuous_segment(parsed);
    let latest = observations.last();
    let current_used = latest.map(|observation| observation.used);
    let remaining = current_used.map(|used| (100.0 - used).max(0.0));
    let observation_count = observations.len();
    let coverage_minutes = coverage_minutes(&observations);
    let intervals = interval_rates(&observations);
    let latest_interval = intervals.last().map(|(_, rate)| *rate);
    let rolling_30m = rolling_rate(&observations, 30.0);
    let rolling_1h = rolling_rate(&observations, 60.0);
    let rolling_3h = rolling_rate(&observations, 180.0);
    let rolling_6h = rolling_rate(&observations, 360.0);
    let complete_window = endpoint_rate(&observations);
    let ewma = ewma_rate(&intervals);
    let regression = ols_slope(&observations);
    let regularity = polling_regularity(&observations);
    let largest_gap = largest_gap_minutes(&observations);
    let stability = slope_stability(&intervals);
    let agreement = model_agreement([latest_interval, ewma, regression]);

    let (selected_model, selected_rate) = select_model(
        observation_count,
        coverage_minutes,
        regularity,
        stability,
        agreement,
        latest_interval,
        ewma,
        regression,
    );

    let reset_horizon_hours = input.resets_at.and_then(|reset| {
        let hours = (reset - input.generated_at).num_seconds() as f64 / 3600.0;
        (hours > 0.0).then_some(hours)
    });
    let safe_rate = match (remaining, reset_horizon_hours) {
        (Some(remaining), Some(hours)) => Some(remaining / hours),
        _ => None,
    };
    let pace_ratio = match (selected_rate, safe_rate) {
        (Some(rate), Some(safe)) if rate > 0.0 && safe > 0.0 => Some(rate / safe),
        _ => None,
    };

    let mut missing_signals = Vec::new();
    if observation_count < 3 {
        missing_signals.push("At least three compatible quota observations".to_string());
    }
    if coverage_minutes < MIN_FORECAST_COVERAGE_MINUTES {
        missing_signals.push("At least 15 minutes of compatible history".to_string());
    }
    if input.resets_at.is_none() {
        missing_signals.push("A reliable reset timestamp".to_string());
    } else if reset_horizon_hours.is_none() {
        missing_signals.push("A reset timestamp in the future".to_string());
    }
    if selected_rate.is_none() {
        missing_signals.push("A stable positive burn rate".to_string());
    }

    let confidence = forecast_confidence(
        observation_count,
        coverage_minutes,
        regularity,
        largest_gap,
        stability,
        agreement,
        selected_rate,
        discontinuity_reason.is_some(),
    );
    let invalidation_reason = if observation_count < 3 {
        Some("Fewer than three compatible observations are available.".to_string())
    } else if coverage_minutes < MIN_FORECAST_COVERAGE_MINUTES {
        Some("Compatible observations cover less than 15 minutes.".to_string())
    } else if reset_horizon_hours.is_none() {
        Some("The reset horizon is missing or has expired.".to_string())
    } else if selected_rate.is_none() {
        Some("No stable positive forecast rate is available.".to_string())
    } else {
        discontinuity_reason
    };

    let predicted_exhaustion_at = match (latest, remaining, selected_rate) {
        (Some(latest), Some(remaining), Some(rate)) if rate > 0.0 => {
            let seconds = (remaining / rate * 3600.0).round() as i64;
            Some((latest.at + Duration::seconds(seconds)).to_rfc3339())
        }
        _ => None,
    };
    let projected_usage_at_reset = match (current_used, selected_rate, reset_horizon_hours) {
        (Some(used), Some(rate), Some(hours)) if rate > 0.0 => Some(used + rate * hours),
        _ => None,
    };
    let exhaustion_before_reset = match (&predicted_exhaustion_at, input.resets_at) {
        (Some(predicted), Some(reset)) => DateTime::parse_from_rfc3339(predicted)
            .ok()
            .map(|value| value.with_timezone(&Utc) < reset),
        _ => None,
    };

    let status = if invalidation_reason.is_none() {
        TelemetryStatus {
            state: TelemetryState::Live,
            accuracy: Accuracy::Estimated,
            source: "Local deterministic forecast".to_string(),
            last_observed_at: latest.map(|observation| observation.at.to_rfc3339()),
            reason: None,
            required_integration: Some("Read-only App Server quota history".to_string()),
        }
    } else {
        TelemetryStatus {
            state: TelemetryState::Unavailable,
            accuracy: Accuracy::Unavailable,
            source: "Local deterministic forecast".to_string(),
            last_observed_at: latest.map(|observation| observation.at.to_rfc3339()),
            reason: invalidation_reason.clone(),
            required_integration: Some("Read-only App Server quota history".to_string()),
        }
    };

    QuotaForecast {
        bucket_id: input.bucket_id,
        bucket_name: input.bucket_name,
        window_label: input.window_label,
        generated_at: input.generated_at.to_rfc3339(),
        resets_at: input.resets_at.map(|reset| reset.to_rfc3339()),
        current_used_percent: current_used,
        remaining_percent: remaining,
        latest_interval_rate_pph: latest_interval,
        rolling_30m_rate_pph: rolling_30m,
        rolling_1h_rate_pph: rolling_1h,
        rolling_3h_rate_pph: rolling_3h,
        rolling_6h_rate_pph: rolling_6h,
        complete_window_rate_pph: complete_window,
        ewma_rate_pph: ewma,
        regression_rate_pph: regression,
        selected_rate_pph: selected_rate,
        safe_rate_pph: safe_rate,
        pace_ratio,
        risk: risk_from_pace(pace_ratio),
        predicted_exhaustion_at,
        projected_usage_at_reset,
        exhaustion_before_reset,
        quality: ForecastQuality {
            selected_model,
            confidence,
            observation_count,
            coverage_duration_minutes: coverage_minutes,
            polling_regularity: regularity,
            largest_gap_minutes: largest_gap,
            model_agreement: agreement,
            slope_stability: stability,
            invalidation_reason,
            missing_signals,
        },
        trajectory: trajectory_series(
            &observations,
            input.resets_at,
            selected_rate,
            confidence,
            stability,
        ),
        burn_rates: burn_rate_series(&observations, input.resets_at),
        status,
    }
}

fn latest_continuous_segment(
    observations: Vec<ParsedObservation>,
) -> (Vec<ParsedObservation>, Option<String>) {
    if observations.len() < 2 {
        return (observations, None);
    }
    let mut segment_start = 0;
    let mut reason = None;
    for index in 1..observations.len() {
        let previous = &observations[index - 1];
        let current = &observations[index];
        let gap_minutes = (current.at - previous.at).num_seconds() as f64 / 60.0;
        if current.used + DISCONTINUITY_DROP_TOLERANCE < previous.used {
            segment_start = index;
            reason = Some("A usage drop started a new quota-window segment.".to_string());
        } else if gap_minutes > MAX_CONTINUITY_GAP_MINUTES {
            segment_start = index;
            reason = Some("A collection gap started a new analytical segment.".to_string());
        }
    }
    (observations[segment_start..].to_vec(), reason)
}

fn interval_rates(observations: &[ParsedObservation]) -> Vec<(DateTime<Utc>, f64)> {
    observations
        .windows(2)
        .filter_map(|pair| {
            let hours = (pair[1].at - pair[0].at).num_milliseconds() as f64 / 3_600_000.0;
            (hours > 0.0).then_some((pair[1].at, (pair[1].used - pair[0].used) / hours))
        })
        .collect()
}

fn coverage_minutes(observations: &[ParsedObservation]) -> f64 {
    match (observations.first(), observations.last()) {
        (Some(first), Some(last)) => (last.at - first.at).num_seconds().max(0) as f64 / 60.0,
        _ => 0.0,
    }
}

fn endpoint_rate(observations: &[ParsedObservation]) -> Option<f64> {
    let first = observations.first()?;
    let last = observations.last()?;
    let hours = (last.at - first.at).num_milliseconds() as f64 / 3_600_000.0;
    (hours > 0.0).then_some((last.used - first.used) / hours)
}

fn rolling_rate(observations: &[ParsedObservation], window_minutes: f64) -> Option<f64> {
    let last = observations.last()?;
    let start = observations.iter().rev().find(|observation| {
        (last.at - observation.at).num_seconds() as f64 / 60.0 >= window_minutes * 0.8
    })?;
    let hours = (last.at - start.at).num_milliseconds() as f64 / 3_600_000.0;
    (hours > 0.0).then_some((last.used - start.used) / hours)
}

fn ewma_rate(intervals: &[(DateTime<Utc>, f64)]) -> Option<f64> {
    let mut values = intervals.iter().map(|(_, rate)| *rate);
    let mut current = values.next()?;
    for rate in values {
        current = EWMA_ALPHA * rate + (1.0 - EWMA_ALPHA) * current;
    }
    current.is_finite().then_some(current)
}

fn ols_slope(observations: &[ParsedObservation]) -> Option<f64> {
    if observations.len() < 3 {
        return None;
    }
    let origin = observations.first()?.at;
    let points = observations
        .iter()
        .map(|observation| {
            (
                (observation.at - origin).num_milliseconds() as f64 / 3_600_000.0,
                observation.used,
            )
        })
        .collect::<Vec<_>>();
    let mean_x = points.iter().map(|(x, _)| x).sum::<f64>() / points.len() as f64;
    let mean_y = points.iter().map(|(_, y)| y).sum::<f64>() / points.len() as f64;
    let numerator = points
        .iter()
        .map(|(x, y)| (x - mean_x) * (y - mean_y))
        .sum::<f64>();
    let denominator = points
        .iter()
        .map(|(x, _)| (x - mean_x).powi(2))
        .sum::<f64>();
    (denominator > f64::EPSILON)
        .then_some(numerator / denominator)
        .filter(|value| value.is_finite())
}

fn polling_regularity(observations: &[ParsedObservation]) -> f64 {
    let gaps = observations
        .windows(2)
        .map(|pair| (pair[1].at - pair[0].at).num_seconds().max(0) as f64)
        .collect::<Vec<_>>();
    if gaps.len() < 2 {
        return 0.0;
    }
    let mean = gaps.iter().sum::<f64>() / gaps.len() as f64;
    if mean <= 0.0 {
        return 0.0;
    }
    let variance = gaps.iter().map(|gap| (gap - mean).powi(2)).sum::<f64>() / gaps.len() as f64;
    (1.0 - variance.sqrt() / mean).clamp(0.0, 1.0)
}

fn largest_gap_minutes(observations: &[ParsedObservation]) -> f64 {
    observations
        .windows(2)
        .map(|pair| (pair[1].at - pair[0].at).num_seconds().max(0) as f64 / 60.0)
        .fold(0.0, f64::max)
}

fn slope_stability(intervals: &[(DateTime<Utc>, f64)]) -> Option<f64> {
    if intervals.len() < 2 {
        return None;
    }
    let rates = intervals.iter().map(|(_, rate)| *rate).collect::<Vec<_>>();
    let mean = rates.iter().sum::<f64>() / rates.len() as f64;
    let variance = rates.iter().map(|rate| (rate - mean).powi(2)).sum::<f64>() / rates.len() as f64;
    Some((1.0 - variance.sqrt() / mean.abs().max(0.1)).clamp(0.0, 1.0))
}

fn model_agreement(models: [Option<f64>; 3]) -> Option<f64> {
    let values = models
        .into_iter()
        .flatten()
        .filter(|value| value.is_finite() && *value > 0.0)
        .collect::<Vec<_>>();
    if values.len() < 2 {
        return None;
    }
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    Some((1.0 - (max - min) / mean.abs().max(0.1)).clamp(0.0, 1.0))
}

#[allow(clippy::too_many_arguments)]
fn select_model(
    count: usize,
    coverage_minutes: f64,
    regularity: f64,
    stability: Option<f64>,
    agreement: Option<f64>,
    latest: Option<f64>,
    ewma: Option<f64>,
    regression: Option<f64>,
) -> (ForecastModel, Option<f64>) {
    if count < 3 || coverage_minutes < MIN_FORECAST_COVERAGE_MINUTES {
        return (ForecastModel::Unavailable, None);
    }
    let positive = |value: Option<f64>| value.filter(|rate| rate.is_finite() && *rate > 0.0);
    if count >= 5
        && regularity >= 0.55
        && stability.unwrap_or(0.0) >= 0.35
        && agreement.unwrap_or(0.0) >= 0.45
    {
        if let Some(rate) = positive(regression) {
            return (ForecastModel::OrdinaryLeastSquares, Some(rate));
        }
    }
    if count >= 4 {
        if let Some(rate) = positive(ewma) {
            return (
                ForecastModel::ExponentiallyWeightedMovingAverage,
                Some(rate),
            );
        }
    }
    if let Some(rate) = positive(latest) {
        return (ForecastModel::RecentRate, Some(rate));
    }
    (ForecastModel::Unavailable, None)
}

#[allow(clippy::too_many_arguments)]
fn forecast_confidence(
    count: usize,
    coverage_minutes: f64,
    regularity: f64,
    largest_gap_minutes: f64,
    stability: Option<f64>,
    agreement: Option<f64>,
    selected_rate: Option<f64>,
    discontinuity: bool,
) -> ForecastConfidence {
    if count < 3 || selected_rate.is_none() {
        return ForecastConfidence::Unavailable;
    }
    if coverage_minutes < 15.0 {
        return ForecastConfidence::Preliminary;
    }
    let mut level = if coverage_minutes < 60.0 {
        1
    } else if coverage_minutes < 180.0 {
        2
    } else {
        3
    };
    if regularity < 0.5
        || largest_gap_minutes > 60.0
        || stability.unwrap_or(0.0) < 0.35
        || agreement.unwrap_or(0.0) < 0.45
        || discontinuity
    {
        level = (level - 1).max(1);
    }
    match level {
        3 => ForecastConfidence::High,
        2 => ForecastConfidence::Medium,
        _ => ForecastConfidence::Low,
    }
}

fn risk_from_pace(pace_ratio: Option<f64>) -> ForecastRisk {
    match pace_ratio {
        Some(value) if value > 1.5 => ForecastRisk::ExhaustionLikely,
        Some(value) if value >= 1.0 => ForecastRisk::AtRisk,
        Some(value) if value >= 0.75 => ForecastRisk::Watch,
        Some(_) => ForecastRisk::Healthy,
        None => ForecastRisk::Unavailable,
    }
}

fn burn_rate_series(
    observations: &[ParsedObservation],
    reset: Option<DateTime<Utc>>,
) -> Vec<BurnRatePoint> {
    observations
        .iter()
        .enumerate()
        .map(|(index, observation)| {
            let slice = &observations[..=index];
            let intervals = interval_rates(slice);
            let remaining = (100.0 - observation.used).max(0.0);
            let safe_rate = reset.and_then(|reset| {
                let hours = (reset - observation.at).num_seconds() as f64 / 3600.0;
                (hours > 0.0).then_some(remaining / hours)
            });
            BurnRatePoint {
                observed_at: observation.at.to_rfc3339(),
                interval_rate_pph: intervals.last().map(|(_, rate)| *rate),
                rolling_30m_rate_pph: rolling_rate(slice, 30.0),
                rolling_1h_rate_pph: rolling_rate(slice, 60.0),
                rolling_3h_rate_pph: rolling_rate(slice, 180.0),
                rolling_6h_rate_pph: rolling_rate(slice, 360.0),
                ewma_rate_pph: ewma_rate(&intervals),
                safe_rate_pph: safe_rate,
            }
        })
        .collect()
}

fn trajectory_series(
    observations: &[ParsedObservation],
    reset: Option<DateTime<Utc>>,
    selected_rate: Option<f64>,
    confidence: ForecastConfidence,
    stability: Option<f64>,
) -> Vec<ConsumptionTrajectoryPoint> {
    let mut result = observations
        .iter()
        .enumerate()
        .map(|(index, observation)| ConsumptionTrajectoryPoint {
            observed_at: observation.at.to_rfc3339(),
            reported_used_percent: Some(observation.used),
            rolling_trend_percent: ols_prediction(&observations[..=index], observation.at),
            forecast_used_percent: None,
            confidence_low_percent: None,
            confidence_high_percent: None,
        })
        .collect::<Vec<_>>();
    let (Some(latest), Some(reset), Some(rate)) = (observations.last(), reset, selected_rate)
    else {
        return result;
    };
    if rate <= 0.0 || reset <= latest.at {
        return result;
    }
    let uncertainty = match confidence {
        ForecastConfidence::High => 0.15,
        ForecastConfidence::Medium => 0.3,
        ForecastConfidence::Low | ForecastConfidence::Preliminary => 0.5,
        ForecastConfidence::Unavailable => return result,
    } * (2.0 - stability.unwrap_or(0.5));
    let horizon_seconds = (reset - latest.at).num_seconds();
    for step in 0..=12_i64 {
        let at = latest.at + Duration::seconds(horizon_seconds * step / 12);
        let hours = (at - latest.at).num_seconds() as f64 / 3600.0;
        let projected = latest.used + rate * hours;
        let spread = rate.abs() * hours * uncertainty;
        result.push(ConsumptionTrajectoryPoint {
            observed_at: at.to_rfc3339(),
            reported_used_percent: None,
            rolling_trend_percent: None,
            forecast_used_percent: Some(projected),
            confidence_low_percent: Some((projected - spread).max(0.0)),
            confidence_high_percent: Some(projected + spread),
        });
    }
    result
}

fn ols_prediction(observations: &[ParsedObservation], at: DateTime<Utc>) -> Option<f64> {
    let slope = ols_slope(observations)?;
    let origin = observations.first()?;
    let hours = (at - origin.at).num_seconds() as f64 / 3600.0;
    Some(origin.used + slope * hours)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observations(values: &[(i64, f64)]) -> Vec<QuotaObservation> {
        let origin = DateTime::parse_from_rfc3339("2026-07-24T08:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        values
            .iter()
            .map(|(minutes, used)| QuotaObservation {
                observed_at: (origin + Duration::minutes(*minutes)).to_rfc3339(),
                used_percent: *used,
                accuracy: Accuracy::ReportedExact,
            })
            .collect()
    }

    fn input(values: &[(i64, f64)]) -> ForecastInput {
        ForecastInput {
            bucket_id: "quota:app-server:codex:primary".to_string(),
            bucket_name: "Codex".to_string(),
            window_label: "5 hour window".to_string(),
            generated_at: DateTime::parse_from_rfc3339("2026-07-24T10:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            resets_at: Some(
                DateTime::parse_from_rfc3339("2026-07-24T13:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            ),
            observations: observations(values),
        }
    }

    #[test]
    fn regular_intervals_produce_rates_safe_pace_and_exhaustion() {
        let forecast = calculate_forecast(input(&[
            (0, 20.0),
            (30, 25.0),
            (60, 30.0),
            (90, 35.0),
            (120, 40.0),
        ]));
        assert_eq!(forecast.latest_interval_rate_pph, Some(10.0));
        assert_eq!(forecast.rolling_1h_rate_pph, Some(10.0));
        assert_eq!(forecast.safe_rate_pph, Some(20.0));
        assert_eq!(forecast.pace_ratio, Some(0.5));
        assert_eq!(forecast.risk, ForecastRisk::Healthy);
        assert!(forecast.predicted_exhaustion_at.is_some());
        assert_eq!(forecast.quality.confidence, ForecastConfidence::Medium);
    }

    #[test]
    fn irregular_intervals_use_elapsed_time_and_regression() {
        let forecast = calculate_forecast(input(&[
            (0, 10.0),
            (10, 12.0),
            (35, 17.0),
            (80, 26.0),
            (120, 34.0),
        ]));
        assert!((forecast.latest_interval_rate_pph.unwrap() - 12.0).abs() < 0.001);
        assert!((forecast.regression_rate_pph.unwrap() - 12.0).abs() < 0.001);
        assert!(forecast.quality.polling_regularity < 1.0);
    }

    #[test]
    fn insufficient_zero_and_negative_rates_do_not_fabricate_dates() {
        let insufficient = calculate_forecast(input(&[(0, 20.0), (5, 21.0)]));
        assert_eq!(
            insufficient.quality.confidence,
            ForecastConfidence::Unavailable
        );
        assert!(insufficient.predicted_exhaustion_at.is_none());

        let zero = calculate_forecast(input(&[(0, 20.0), (30, 20.0), (60, 20.0)]));
        assert!(zero.selected_rate_pph.is_none());
        assert!(zero.predicted_exhaustion_at.is_none());

        let negative = calculate_forecast(input(&[(0, 30.0), (30, 29.5), (60, 29.0)]));
        assert!(negative.selected_rate_pph.is_none());
        assert!(negative.predicted_exhaustion_at.is_none());
    }

    #[test]
    fn reset_drop_and_long_gap_start_new_segments() {
        let reset = calculate_forecast(input(&[
            (0, 80.0),
            (30, 85.0),
            (60, 5.0),
            (90, 10.0),
            (120, 15.0),
        ]));
        assert_eq!(reset.quality.observation_count, 3);
        assert!(reset
            .quality
            .invalidation_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("usage drop")));

        let gap = calculate_forecast(input(&[
            (0, 10.0),
            (30, 12.0),
            (200, 20.0),
            (230, 22.0),
            (260, 24.0),
        ]));
        assert_eq!(gap.quality.observation_count, 3);
        assert!(gap
            .quality
            .invalidation_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("collection gap")));
    }

    #[test]
    fn pace_classification_uses_documented_boundaries() {
        assert_eq!(risk_from_pace(Some(0.74)), ForecastRisk::Healthy);
        assert_eq!(risk_from_pace(Some(0.75)), ForecastRisk::Watch);
        assert_eq!(risk_from_pace(Some(1.0)), ForecastRisk::AtRisk);
        assert_eq!(risk_from_pace(Some(1.51)), ForecastRisk::ExhaustionLikely);
        assert_eq!(risk_from_pace(None), ForecastRisk::Unavailable);
    }
}
