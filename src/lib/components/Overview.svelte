<script lang="ts">
  import AccuracyBadge from './AccuracyBadge.svelte';
  import QuotaAlertCard from './QuotaAlertCard.svelte';
  import QuotaCard from './QuotaCard.svelte';
  import TelemetryState from './TelemetryState.svelte';
  import TimeSeriesChart from './TimeSeriesChart.svelte';
  import type { ChartMarker, ChartSeries } from './TimeSeriesChart.svelte';
  import type { Dashboard, QuotaAlert } from '../types';

  let {
    dashboard,
    onnavigate,
    ondismiss,
    onenterdemo,
    onrefresh,
    refreshdisabled,
  }: {
    dashboard: Dashboard;
    onnavigate: (view: 'Alerts' | 'Forecast' | 'Settings' | 'Turns' | 'Usage Burn') => void;
    ondismiss: (alert: QuotaAlert) => void;
    onenterdemo: () => void;
    onrefresh: () => void;
    refreshdisabled: boolean;
  } = $props();

  const formatNumber = (value: number): string =>
    new Intl.NumberFormat(undefined, { notation: value > 999_999 ? 'compact' : 'standard' }).format(
      value,
    );
  const formatDuration = (duration: number | null): string => {
    if (duration === null) return 'Unavailable';
    const minutes = Math.round(duration / 60_000);
    return minutes > 59 ? `${Math.floor(minutes / 60)}h ${minutes % 60}m` : `${minutes}m`;
  };
  const detailSources = $derived(
    dashboard.channels.filter((channel) =>
      ['lifecycle-hooks', 'opentelemetry'].includes(channel.id),
    ),
  );
  const detailedDisabled = $derived(
    !dashboard.demoMode &&
      detailSources.length > 0 &&
      detailSources.every((source) => !source.enabled),
  );
  const prominentAlerts = $derived(
    dashboard.alerts.active.filter((alert) =>
      ['warning', 'critical', 'exhausted'].includes(alert.severity),
    ),
  );
  const priorityForecast = $derived(dashboard.forecasts[0]);
  const trajectory = $derived(priorityForecast?.trajectory ?? []);
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
  ]);
  const trajectoryMarkers = $derived<ChartMarker[]>(
    priorityForecast
      ? [
          { at: priorityForecast.generatedAt, label: 'Current', color: '#a8b6b0' },
          ...(priorityForecast.predictedExhaustionAt
            ? [
                {
                  at: priorityForecast.predictedExhaustionAt,
                  label: 'Estimated exhaustion',
                  color: '#e46767',
                },
              ]
            : []),
          ...(priorityForecast.resetsAt
            ? [{ at: priorityForecast.resetsAt, label: 'Reset', color: '#7b9e90' }]
            : []),
        ]
      : [],
  );
  const formatPercent = (value: number | null | undefined): string =>
    value === null || value === undefined ? '—' : `${value.toFixed(1)}%`;
  const formatRate = (value: number | null | undefined): string =>
    value === null || value === undefined ? '—' : `${value.toFixed(2)} pp/h`;
  const formatRatio = (value: number | null | undefined): string =>
    value === null || value === undefined ? '—' : `${value.toFixed(2)}×`;
  const formatTime = (value: string | null | undefined): string =>
    value ? new Date(value).toLocaleString() : 'Unavailable';
  const accountDays = $derived(dashboard.accountUsage?.dailyUsageBuckets ?? []);
  const currentDayTokens = $derived(accountDays[0]?.tokens ?? null);
  const sevenDayAverage = $derived(
    accountDays.length
      ? accountDays.slice(0, 7).reduce((sum, day) => sum + day.tokens, 0) /
          Math.min(7, accountDays.length)
      : null,
  );
</script>

<section class="view-stack">
  <section class="operational-header" aria-label="Operational status">
    <div>
      <span class:demo-mode={dashboard.demoMode} class="mode-chip"
        >{dashboard.demoMode ? 'DEMO DATA' : 'LIVE'}</span
      >
      <div>
        <strong>{dashboard.collectorState}</strong>
        <small>Collector · Codex {dashboard.codexVersion}</small>
      </div>
    </div>
    <dl>
      <div>
        <dt>Last successful refresh</dt>
        <dd>
          {dashboard.lastEventAt ? new Date(dashboard.lastEventAt).toLocaleString() : 'Waiting'}
        </dd>
      </div>
      <div>
        <dt>Active alerts</dt>
        <dd>{dashboard.alerts.active.length}</dd>
      </div>
    </dl>
    <button class="button secondary" onclick={onrefresh} disabled={refreshdisabled}
      >Refresh now</button
    >
  </section>

  {#if priorityForecast}
    <article class="quota-risk-hero risk-{priorityForecast.risk}">
      <div class="hero-main">
        <div>
          <p class="eyebrow">Highest-priority quota window</p>
          <h2>{priorityForecast.bucketName} · {priorityForecast.windowLabel}</h2>
          <p class="risk-assessment">
            <strong>{priorityForecast.risk.replaceAll('_', ' ')}</strong>
            <span>Locally derived pace assessment</span>
          </p>
        </div>
        <div class="hero-usage">
          <strong>{formatPercent(priorityForecast.currentUsedPercent)}</strong>
          <span>reported used</span>
          <small>{formatPercent(priorityForecast.remainingPercent)} derived remaining</small>
        </div>
      </div>
      <div class="hero-risk-grid">
        <div><span>Reset</span><strong>{formatTime(priorityForecast.resetsAt)}</strong></div>
        <div>
          <span>Current rolling burn</span><strong
            >{formatRate(priorityForecast.selectedRatePph)}</strong
          >
        </div>
        <div><span>Safe burn</span><strong>{formatRate(priorityForecast.safeRatePph)}</strong></div>
        <div><span>Pace ratio</span><strong>{formatRatio(priorityForecast.paceRatio)}</strong></div>
        <div>
          <span>Estimated quota exhaustion</span>
          <strong>{formatTime(priorityForecast.predictedExhaustionAt)}</strong>
        </div>
        <div>
          <span>Projected at reset</span>
          <strong>{formatPercent(priorityForecast.projectedUsageAtReset)}</strong>
        </div>
        <div>
          <span>Before reset?</span>
          <strong
            >{priorityForecast.exhaustionBeforeReset === null
              ? 'Unavailable'
              : priorityForecast.exhaustionBeforeReset
                ? 'Yes · attention required'
                : 'No'}</strong
          >
        </div>
        <div>
          <span>Forecast quality</span>
          <strong
            >{priorityForecast.quality.confidence} · {priorityForecast.quality.observationCount}
            samples</strong
          >
        </div>
      </div>
      <div class="hero-badges">
        <AccuracyBadge accuracy="reported_exact" />
        <AccuracyBadge accuracy="derived_exact" />
        <AccuracyBadge accuracy="estimated" />
        <button class="text-button" onclick={() => onnavigate('Forecast')}>Open Forecast →</button>
      </div>
    </article>
  {:else}
    <article class="panel">
      <TelemetryState status={dashboard.forecastStatus} />
    </article>
  {/if}

  {#if prominentAlerts.length}
    <section class="risk-zone" aria-labelledby="quota-risk-title">
      <div class="section-heading">
        <div>
          <p class="eyebrow">Quota risk</p>
          <h2 id="quota-risk-title">Action recommended</h2>
        </div>
        <button class="text-button" onclick={() => onnavigate('Alerts')}>Open alert center →</button
        >
      </div>
      <div class="alert-grid prominent-grid">
        {#each prominentAlerts.slice(0, 2) as alert (alert.id)}
          <QuotaAlertCard {alert} prominent {ondismiss} />
        {/each}
      </div>
    </section>
  {:else}
    <section class="risk-summary neutral-risk" aria-label="Quota risk: no warning-level alerts">
      <span aria-hidden="true">✓</span>
      <div>
        <strong>No warning-level quota alert</strong><small
          >Alerts become prominent at 75% used.</small
        >
      </div>
    </section>
  {/if}

  <section class="active-alert-summary" aria-labelledby="active-alerts-title">
    <div>
      <p class="eyebrow">Active alerts</p>
      <h2 id="active-alerts-title">
        {dashboard.alerts.active.length
          ? `${dashboard.alerts.active.length} active quota alert${dashboard.alerts.active.length === 1 ? '' : 's'}`
          : 'No active alerts'}
      </h2>
    </div>
    <button
      class="button secondary notification-button"
      aria-label={`Open alert center, ${dashboard.alerts.active.length} active alerts`}
      onclick={() => onnavigate('Alerts')}>View alert center</button
    >
  </section>

  <article class="panel chart-panel">
    <TimeSeriesChart
      title="Consumption trajectory"
      description="Reported history, derived rolling trend, and estimated quota forecast."
      timestamps={trajectory.map((point) => point.observedAt)}
      series={trajectorySeries}
      markers={trajectoryMarkers}
      thresholds={[50, 75, 90, 100]}
    />
  </article>

  {#if priorityForecast}
    <section class="kpi-strip" aria-label="Quota risk metrics">
      <div>
        <span>Current usage</span><strong
          >{formatPercent(priorityForecast.currentUsedPercent)}</strong
        >
      </div>
      <div>
        <span>Remaining quota</span><strong
          >{formatPercent(priorityForecast.remainingPercent)}</strong
        >
      </div>
      <div>
        <span>Latest interval</span><strong
          >{formatRate(priorityForecast.latestIntervalRatePph)}</strong
        >
      </div>
      <div>
        <span>1-hour rolling</span><strong>{formatRate(priorityForecast.rolling1hRatePph)}</strong>
      </div>
      <div>
        <span>6-hour rolling</span><strong>{formatRate(priorityForecast.rolling6hRatePph)}</strong>
      </div>
      <div><span>Safe rate</span><strong>{formatRate(priorityForecast.safeRatePph)}</strong></div>
      <div><span>Pace ratio</span><strong>{formatRatio(priorityForecast.paceRatio)}</strong></div>
      <div>
        <span>Estimated exhaustion</span><strong
          >{formatTime(priorityForecast.predictedExhaustionAt)}</strong
        >
      </div>
      <div><span>Confidence</span><strong>{priorityForecast.quality.confidence}</strong></div>
    </section>
  {/if}

  <div class="section-heading">
    <div>
      <p class="eyebrow">Current allowance</p>
      <h2>Quota windows</h2>
    </div>
    <p>
      {dashboard.quotaStatus.lastObservedAt
        ? `Collected ${new Date(dashboard.quotaStatus.lastObservedAt).toLocaleString()}`
        : 'No quota observation'}
    </p>
  </div>
  {#if dashboard.quota.length && ['live', 'fixture'].includes(dashboard.quotaStatus.state)}
    <div class="quota-grid">
      {#each dashboard.quota as quota (quota.id)}<QuotaCard {quota} />{/each}
    </div>
  {:else}
    <article class="panel">
      <TelemetryState status={dashboard.quotaStatus} />
    </article>
  {/if}

  <div class="overview-grid account-health-grid">
    <article class="panel">
      <div class="panel-head">
        <div>
          <p class="eyebrow">Account activity</p>
          <h2>Reported usage</h2>
        </div>
        <AccuracyBadge accuracy={dashboard.accountUsageStatus.accuracy} />
      </div>
      {#if dashboard.accountUsage}
        {#if dashboard.accountUsage.lifetimeTokens !== null}
          <strong class="hero-number">{formatNumber(dashboard.accountUsage.lifetimeTokens)}</strong>
          <span class="hero-label">lifetime tokens</span>
        {:else}
          <p class="unavailable-value">Lifetime token value unavailable</p>
        {/if}
        <dl class="facts">
          <div>
            <dt>Current day</dt>
            <dd>{currentDayTokens === null ? 'Unavailable' : formatNumber(currentDayTokens)}</dd>
          </div>
          <div>
            <dt>7-day average</dt>
            <dd>{sevenDayAverage === null ? 'Unavailable' : formatNumber(sevenDayAverage)}</dd>
          </div>
          <div>
            <dt>Peak day</dt>
            <dd>
              {dashboard.accountUsage.peakDailyTokens === null
                ? 'Unavailable'
                : formatNumber(dashboard.accountUsage.peakDailyTokens)}
            </dd>
          </div>
          <div>
            <dt>Current streak</dt>
            <dd>{dashboard.accountUsage.currentStreakDays ?? 'Unavailable'}</dd>
          </div>
          <div>
            <dt>Longest streak</dt>
            <dd>{dashboard.accountUsage.longestStreakDays ?? 'Unavailable'}</dd>
          </div>
          <div>
            <dt>Longest turn</dt>
            <dd>
              {dashboard.accountUsage.longestRunningTurnSec === null
                ? 'Unavailable'
                : `${Math.round(dashboard.accountUsage.longestRunningTurnSec / 60)} min`}
            </dd>
          </div>
        </dl>
        {#if accountDays.length}
          <div class="daily-trend" aria-label="Recent daily token trend">
            {#each accountDays.slice(0, 7).reverse() as day (day.startDate)}
              <div title={`${day.startDate}: ${formatNumber(day.tokens)} reported tokens`}>
                <span
                  style={`height:${Math.max(
                    8,
                    (day.tokens / Math.max(...accountDays.slice(0, 7).map((item) => item.tokens))) *
                      100,
                  )}%`}
                ></span>
                <small
                  >{new Date(`${day.startDate}T12:00:00Z`).toLocaleDateString([], {
                    weekday: 'narrow',
                  })}</small
                >
              </div>
            {/each}
          </div>
        {/if}
      {:else}
        <TelemetryState status={dashboard.accountUsageStatus} compact />
      {/if}
    </article>

    <article class="panel source-health-panel">
      <div class="panel-head">
        <div>
          <p class="eyebrow">Collection</p>
          <h2>Source health</h2>
        </div>
        <button class="text-button" onclick={() => onnavigate('Settings')}>Configure →</button>
      </div>
      <div class="channel-list">
        {#each dashboard.channels as channel (channel.id)}
          <div class="channel-health-row">
            <span
              class:healthy={channel.healthy}
              class:degraded={channel.status === 'degraded'}
              class="channel-dot"
              aria-hidden="true"
            ></span>
            <div class="channel-detail">
              <p>
                <strong>{channel.name}</strong>
                <small
                  >{channel.enabled ? 'Enabled' : 'Disabled'} · {channel.configurationStatus}</small
                >
              </p>
              <div class="source-state-row">
                <span data-state={channel.telemetryState}>{channel.telemetryState}</span>
                <strong>{channel.healthy ? 'Healthy' : channel.detail}</strong>
              </div>
              <dl>
                <div>
                  <dt>Last event</dt>
                  <dd>
                    {channel.lastEventAt ? new Date(channel.lastEventAt).toLocaleString() : 'None'}
                  </dd>
                </div>
                <div>
                  <dt>Restarts</dt>
                  <dd>{channel.restartCount}</dd>
                </div>
                <div>
                  <dt>Current health</dt>
                  <dd>{channel.status}</dd>
                </div>
                <div>
                  <dt>Last success</dt>
                  <dd>
                    {channel.lastSuccessfulCollectionAt
                      ? new Date(channel.lastSuccessfulCollectionAt).toLocaleString()
                      : 'None'}
                  </dd>
                </div>
              </dl>
              <p class="capability-copy">
                Supplies: {channel.capabilities.join(', ') || 'No mapped capabilities'}
              </p>
              {#if channel.latestError}
                <p class:historical-error={channel.latestError.historical} class="source-error">
                  <strong
                    >{channel.latestError.historical
                      ? 'Last historical error'
                      : 'Latest error'}</strong
                  >
                  {new Date(channel.latestError.occurredAt).toLocaleString()} · {channel.latestError
                    .message}
                </p>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    </article>
  </div>

  <div class="section-heading">
    <div>
      <p class="eyebrow">Detailed turn analytics</p>
      <h2>Evidence from completed turns</h2>
    </div>
  </div>
  {#if detailedDisabled}
    <article class="panel integration-card">
      <div class="integration-icon" aria-hidden="true">＋</div>
      <div>
        <h2>Detailed turn telemetry is not configured.</h2>
        <p>Enable a reviewed local integration to unlock:</p>
        <ul>
          <li>token composition</li>
          <li>model and reasoning analytics</li>
          <li>project and chat attribution</li>
          <li>expensive-turn ranking</li>
          <li>Usage Burn evidence</li>
        </ul>
      </div>
      <button class="button primary" onclick={() => onnavigate('Settings')}
        >Open integration settings</button
      >
    </article>
  {:else}
    <div class="detailed-grid">
      <article class="panel token-panel">
        <div class="panel-head">
          <div>
            <p class="eyebrow">Observed period</p>
            <h2>Token composition</h2>
          </div>
          <AccuracyBadge accuracy={dashboard.tokenStatus.accuracy} />
        </div>
        {#if dashboard.today.total !== null}
          <strong class="hero-number">{formatNumber(dashboard.today.total)}</strong>
          <span class="hero-label">total reported tokens</span>
          <div class="composition" aria-label="Token composition">
            <span class="input" style={`flex-grow:${dashboard.today.input ?? 0}`}></span>
            <span class="cached" style={`flex-grow:${dashboard.today.cachedInput ?? 0}`}></span>
            <span class="output" style={`flex-grow:${dashboard.today.output ?? 0}`}></span>
            <span class="reasoning" style={`flex-grow:${dashboard.today.reasoningOutput ?? 0}`}
            ></span>
          </div>
          <div class="legend">
            <div>
              <i class="input"></i><span>Input</span><strong
                >{formatNumber(dashboard.today.input ?? 0)}</strong
              >
            </div>
            <div>
              <i class="cached"></i><span>Cached input</span><strong
                >{formatNumber(dashboard.today.cachedInput ?? 0)}</strong
              >
            </div>
            <div>
              <i class="output"></i><span>Output</span><strong
                >{formatNumber(dashboard.today.output ?? 0)}</strong
              >
            </div>
            <div>
              <i class="reasoning"></i><span>Reasoning output</span><strong
                >{formatNumber(dashboard.today.reasoningOutput ?? 0)}</strong
              >
            </div>
          </div>
        {:else}
          <TelemetryState
            status={dashboard.tokenStatus}
            compact
            onconfigure={() => onnavigate('Settings')}
          />
        {/if}
      </article>

      <article class="panel distribution-panel">
        <div class="panel-head">
          <div>
            <p class="eyebrow">Mix</p>
            <h2>Models & reasoning</h2>
          </div>
        </div>
        {#if dashboard.modelDistribution.length}
          <h3>Models</h3>
          {#each dashboard.modelDistribution as row (row.label)}
            <div class="bar-row">
              <span>{row.label}</span>
              <div><i style={`width:${row.value}%`}></i></div>
              <strong>{row.value}%</strong>
            </div>
          {/each}
          <h3>Reasoning effort</h3>
          {#each dashboard.reasoningDistribution as row (row.label)}
            <div class="bar-row reasoning-row">
              <span>{row.label}</span>
              <div><i style={`width:${row.value}%`}></i></div>
              <strong>{row.value}%</strong>
            </div>
          {/each}
        {:else}
          <TelemetryState
            status={dashboard.modelStatus}
            compact
            onconfigure={() => onnavigate('Settings')}
          />
        {/if}
      </article>

      <article class="panel detailed-wide">
        <div class="panel-head">
          <div>
            <p class="eyebrow">Ranked by tokens</p>
            <h2>Recent expensive turns</h2>
          </div>
          <button class="text-button" onclick={() => onnavigate('Turns')}>View turns →</button>
        </div>
        {#if dashboard.turns.length}
          <div class="turn-list">
            {#each dashboard.turns as turn, index (turn.id)}
              <div class="turn-row">
                <div class="turn-rank">{index + 1}</div>
                <div class="turn-copy">
                  <strong>{turn.project ?? 'Unattributed project'}</strong><span
                    >{turn.model ?? 'Model unavailable'} · {turn.reasoningEffort ??
                      'effort unavailable'} · {formatDuration(turn.durationMs)}</span
                  >
                </div>
                <div class="turn-tokens">
                  <strong>{formatNumber(turn.tokens)}</strong><span>tokens</span>
                </div>
                <AccuracyBadge accuracy={turn.accuracy} />
              </div>
            {/each}
          </div>
        {:else if dashboard.turnsStatus.state === 'live'}
          <p class="muted-copy">
            No expensive turns were found in the selected period. The telemetry query completed
            successfully.
          </p>
        {:else}
          <TelemetryState
            status={dashboard.turnsStatus}
            compact
            onconfigure={() => onnavigate('Settings')}
          />
        {/if}
      </article>

      <article class="panel">
        <div class="panel-head">
          <div>
            <p class="eyebrow">Usage Burn</p>
            <h2>{dashboard.burn.headline}</h2>
          </div>
          <AccuracyBadge accuracy={dashboard.burnStatus.accuracy} />
        </div>
        {#if dashboard.burn.factors.length}
          <ul class="missing-list">
            {#each dashboard.burn.factors.slice(0, 3) as factor (factor)}<li>{factor}</li>{/each}
          </ul>
          <button class="text-button" onclick={() => onnavigate('Usage Burn')}
            >Open evidence →</button
          >
        {:else}
          <TelemetryState
            status={dashboard.burnStatus}
            compact
            onconfigure={() => onnavigate('Settings')}
          />
        {/if}
      </article>
    </div>
  {/if}

  {#if !dashboard.demoMode && dashboard.quota.length === 0}
    <button class="text-button demo-link" onclick={onenterdemo}
      >Preview the interface with clearly labeled demo data</button
    >
  {/if}
</section>
