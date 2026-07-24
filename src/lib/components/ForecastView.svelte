<script lang="ts">
  import AccuracyBadge from './AccuracyBadge.svelte';
  import TelemetryState from './TelemetryState.svelte';
  import TimeSeriesChart from './TimeSeriesChart.svelte';
  import type { ChartMarker, ChartSeries } from './TimeSeriesChart.svelte';
  import type { Dashboard } from '../types';

  let { dashboard }: { dashboard: Dashboard } = $props();
  let selectedBucketId = $state('');
  let timeRange = $state<'window' | '6h' | '24h'>('window');

  $effect(() => {
    if (!selectedBucketId && dashboard.forecasts[0]) {
      selectedBucketId = dashboard.forecasts[0].bucketId;
    }
  });

  const selected = $derived(
    dashboard.forecasts.find((forecast) => forecast.bucketId === selectedBucketId) ??
      dashboard.forecasts[0],
  );
  const cutoff = $derived.by(() => {
    if (!selected || timeRange === 'window') return Number.NEGATIVE_INFINITY;
    const hours = timeRange === '6h' ? 6 : 24;
    return Date.now() - hours * 3_600_000;
  });
  const trajectory = $derived(
    selected?.trajectory.filter((point) => new Date(point.observedAt).getTime() >= cutoff) ?? [],
  );
  const burnRates = $derived(
    selected?.burnRates.filter((point) => new Date(point.observedAt).getTime() >= cutoff) ?? [],
  );

  const formatPercent = (value: number | null | undefined): string =>
    value === null || value === undefined ? '—' : `${value.toFixed(1)}%`;
  const formatRate = (value: number | null | undefined): string =>
    value === null || value === undefined ? '—' : `${value.toFixed(2)} pp/h`;
  const formatRatio = (value: number | null | undefined): string =>
    value === null || value === undefined ? '—' : `${value.toFixed(2)}×`;
  const formatTime = (value: string | null | undefined): string =>
    value ? new Date(value).toLocaleString() : 'Unavailable';
  const formatDuration = (minutes: number): string =>
    minutes >= 60 ? `${(minutes / 60).toFixed(1)} hours` : `${minutes.toFixed(0)} minutes`;

  const trajectorySeries = $derived<ChartSeries[]>([
    {
      label: 'Reported used',
      values: trajectory.map((point) => point.reportedUsedPercent),
      color: '#7ce0a4',
      accuracy: 'reported_exact',
    },
    {
      label: 'Rolling trend',
      values: trajectory.map((point) => point.rollingTrendPercent),
      color: '#76a9df',
      accuracy: 'derived_exact',
    },
    {
      label: 'Estimated forecast',
      values: trajectory.map((point) => point.forecastUsedPercent),
      color: '#e1a454',
      accuracy: 'estimated',
      dashed: true,
    },
    {
      label: 'Estimated lower range',
      values: trajectory.map((point) => point.confidenceLowPercent),
      color: '#a27a49',
      accuracy: 'estimated',
      dashed: true,
    },
    {
      label: 'Estimated upper range',
      values: trajectory.map((point) => point.confidenceHighPercent),
      color: '#a27a49',
      accuracy: 'estimated',
      dashed: true,
    },
  ]);
  const trajectoryMarkers = $derived<ChartMarker[]>(
    selected
      ? [
          { at: selected.generatedAt, label: 'Current', color: '#a8b6b0' },
          ...(selected.predictedExhaustionAt
            ? [
                {
                  at: selected.predictedExhaustionAt,
                  label: 'Estimated exhaustion',
                  color: '#e46767',
                },
              ]
            : []),
          ...(selected.resetsAt
            ? [{ at: selected.resetsAt, label: 'Reset', color: '#7b9e90' }]
            : []),
        ]
      : [],
  );
  const burnSeries = $derived<ChartSeries[]>([
    {
      label: 'Interval',
      values: burnRates.map((point) => point.intervalRatePph),
      color: '#8fd8aa',
      accuracy: 'derived_exact',
    },
    {
      label: '30 minute',
      values: burnRates.map((point) => point.rolling30mRatePph),
      color: '#7dbbe8',
      accuracy: 'derived_exact',
    },
    {
      label: '1 hour',
      values: burnRates.map((point) => point.rolling1hRatePph),
      color: '#9c94e8',
      accuracy: 'derived_exact',
    },
    {
      label: '3 hour',
      values: burnRates.map((point) => point.rolling3hRatePph),
      color: '#c58bdc',
      accuracy: 'derived_exact',
    },
    {
      label: '6 hour',
      values: burnRates.map((point) => point.rolling6hRatePph),
      color: '#d986ae',
      accuracy: 'derived_exact',
    },
    {
      label: 'EWMA',
      values: burnRates.map((point) => point.ewmaRatePph),
      color: '#e0a14f',
      accuracy: 'estimated',
      dashed: true,
    },
    {
      label: 'Safe rate',
      values: burnRates.map((point) => point.safeRatePph),
      color: '#cad6d1',
      accuracy: 'derived_exact',
      dashed: true,
    },
  ]);
  const accountDays = $derived(dashboard.accountUsage?.dailyUsageBuckets ?? []);
  const tokenSeries = $derived<ChartSeries[]>([
    {
      label: 'Reported daily total',
      values: accountDays.map((day) => day.tokens),
      color: '#7ce0a4',
      accuracy: 'reported_exact',
    },
    {
      label: '7-day rolling average',
      values: accountDays.map((_, index, values) => {
        const slice = values.slice(Math.max(0, index - 6), index + 1);
        return slice.reduce((sum, day) => sum + day.tokens, 0) / slice.length;
      }),
      color: '#76a9df',
      accuracy: 'derived_exact',
    },
  ]);
</script>

<section class="view-stack forecast-view" aria-labelledby="forecast-title">
  <div class="section-heading forecast-controls">
    <div>
      <p class="eyebrow">Local deterministic analytics</p>
      <h2 id="forecast-title">Quota outlook</h2>
    </div>
    <div class="control-row">
      <label>
        <span>Quota bucket</span>
        <select bind:value={selectedBucketId} aria-label="Quota bucket">
          {#each dashboard.forecasts as forecast (forecast.bucketId)}
            <option value={forecast.bucketId}>{forecast.bucketName} · {forecast.windowLabel}</option
            >
          {/each}
        </select>
      </label>
      <label>
        <span>Time range</span>
        <select bind:value={timeRange} aria-label="Forecast time range">
          <option value="window">Current window</option>
          <option value="6h">Last 6 hours</option>
          <option value="24h">Last 24 hours</option>
        </select>
      </label>
      <label>
        <span>Granularity</span>
        <select aria-label="Chart granularity">
          <option>Observed samples</option>
        </select>
      </label>
    </div>
  </div>

  {#if selected}
    <article class="forecast-outlook risk-{selected.risk}">
      <div class="forecast-outlook-head">
        <div>
          <p class="eyebrow">{selected.bucketName} · {selected.windowLabel}</p>
          <h2>{selected.risk.replaceAll('_', ' ')}</h2>
          <p>This is a locally derived pace assessment. Quota capacity units are not exposed.</p>
        </div>
        <div class="badge-stack">
          <AccuracyBadge accuracy="reported_exact" />
          <AccuracyBadge accuracy="derived_exact" />
          <AccuracyBadge accuracy="estimated" />
        </div>
      </div>
      <div class="outlook-grid">
        <div>
          <span>Current usage</span><strong>{formatPercent(selected.currentUsedPercent)}</strong>
        </div>
        <div><span>Remaining</span><strong>{formatPercent(selected.remainingPercent)}</strong></div>
        <div><span>Reset</span><strong>{formatTime(selected.resetsAt)}</strong></div>
        <div><span>Current pace</span><strong>{formatRate(selected.selectedRatePph)}</strong></div>
        <div><span>Safe pace</span><strong>{formatRate(selected.safeRatePph)}</strong></div>
        <div><span>Pace ratio</span><strong>{formatRatio(selected.paceRatio)}</strong></div>
        <div>
          <span>Estimated quota exhaustion</span>
          <strong>{formatTime(selected.predictedExhaustionAt)}</strong>
        </div>
        <div>
          <span>Projected usage at reset</span>
          <strong>{formatPercent(selected.projectedUsageAtReset)}</strong>
        </div>
      </div>
    </article>

    <article class="panel chart-panel">
      <TimeSeriesChart
        title="Consumption trajectory"
        description="Reported history, derived rolling trend, and estimated forecast are intentionally distinct."
        timestamps={trajectory.map((point) => point.observedAt)}
        series={trajectorySeries}
        markers={trajectoryMarkers}
        thresholds={[50, 75, 90, 100]}
      />
    </article>

    <article class="panel chart-panel">
      <TimeSeriesChart
        title="Burn rate"
        description="Percentage points per hour. Irregular polling uses actual elapsed time."
        timestamps={burnRates.map((point) => point.observedAt)}
        series={burnSeries}
        markers={selected.resetsAt
          ? [{ at: selected.resetsAt, label: 'Reset', color: '#7b9e90' }]
          : []}
        unit=" pp/h"
      />
    </article>

    <div class="overview-grid">
      <article class="panel">
        <div class="panel-head">
          <div>
            <p class="eyebrow">Forecast quality</p>
            <h2>{selected.quality.confidence} confidence</h2>
          </div>
          <AccuracyBadge accuracy="estimated" />
        </div>
        <dl class="facts">
          <div>
            <dt>Selected model</dt>
            <dd>{selected.quality.selectedModel.replaceAll('_', ' ')}</dd>
          </div>
          <div>
            <dt>Observations</dt>
            <dd>{selected.quality.observationCount}</dd>
          </div>
          <div>
            <dt>Coverage</dt>
            <dd>{formatDuration(selected.quality.coverageDurationMinutes)}</dd>
          </div>
          <div>
            <dt>Polling regularity</dt>
            <dd>{formatPercent(selected.quality.pollingRegularity * 100)}</dd>
          </div>
          <div>
            <dt>Largest gap</dt>
            <dd>{selected.quality.largestGapMinutes.toFixed(0)} min</dd>
          </div>
          <div>
            <dt>Model agreement</dt>
            <dd>
              {formatPercent(
                selected.quality.modelAgreement === null
                  ? null
                  : selected.quality.modelAgreement * 100,
              )}
            </dd>
          </div>
          <div>
            <dt>Slope stability</dt>
            <dd>
              {formatPercent(
                selected.quality.slopeStability === null
                  ? null
                  : selected.quality.slopeStability * 100,
              )}
            </dd>
          </div>
        </dl>
        {#if selected.quality.invalidationReason}
          <p class="analysis-note">{selected.quality.invalidationReason}</p>
        {/if}
        {#if selected.quality.missingSignals.length}
          <ul class="missing-list">
            {#each selected.quality.missingSignals as signal (signal)}<li>{signal}</li>{/each}
          </ul>
        {/if}
      </article>
      <article class="panel">
        <p class="eyebrow">Rate comparison</p>
        <h2>Models and rolling windows</h2>
        <dl class="facts">
          <div>
            <dt>Latest interval</dt>
            <dd>{formatRate(selected.latestIntervalRatePph)}</dd>
          </div>
          <div>
            <dt>30 minute</dt>
            <dd>{formatRate(selected.rolling30mRatePph)}</dd>
          </div>
          <div>
            <dt>1 hour</dt>
            <dd>{formatRate(selected.rolling1hRatePph)}</dd>
          </div>
          <div>
            <dt>3 hour</dt>
            <dd>{formatRate(selected.rolling3hRatePph)}</dd>
          </div>
          <div>
            <dt>6 hour</dt>
            <dd>{formatRate(selected.rolling6hRatePph)}</dd>
          </div>
          <div>
            <dt>Complete segment</dt>
            <dd>{formatRate(selected.completeWindowRatePph)}</dd>
          </div>
          <div>
            <dt>EWMA estimate</dt>
            <dd>{formatRate(selected.ewmaRatePph)}</dd>
          </div>
          <div>
            <dt>OLS estimate</dt>
            <dd>{formatRate(selected.regressionRatePph)}</dd>
          </div>
        </dl>
      </article>
    </div>
  {:else}
    <article class="panel">
      <TelemetryState status={dashboard.forecastStatus} />
    </article>
  {/if}

  <article class="panel chart-panel">
    {#if accountDays.length}
      <TimeSeriesChart
        title="Token activity"
        description="Exact account daily totals and a derived seven-day rolling average. Quota and tokens are not treated as equivalent units."
        timestamps={accountDays.map((day) => new Date(`${day.startDate}T12:00:00Z`).toISOString())}
        series={tokenSeries}
        unit=" tokens"
      />
    {:else}
      <div class="panel-head">
        <div>
          <p class="eyebrow">Account activity</p>
          <h2>Token activity</h2>
        </div>
      </div>
      <TelemetryState status={dashboard.accountUsageStatus} compact />
    {/if}
  </article>
</section>
