<script lang="ts">
  import AccuracyBadge from './AccuracyBadge.svelte';
  import QuotaAlertCard from './QuotaAlertCard.svelte';
  import QuotaCard from './QuotaCard.svelte';
  import TelemetryState from './TelemetryState.svelte';
  import type { Dashboard, QuotaAlert } from '../types';

  let {
    dashboard,
    onnavigate,
    ondismiss,
    onenterdemo,
  }: {
    dashboard: Dashboard;
    onnavigate: (view: 'Alerts' | 'Settings' | 'Turns' | 'Usage Burn') => void;
    ondismiss: (alert: QuotaAlert) => void;
    onenterdemo: () => void;
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
</script>

<section class="view-stack">
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
                  >{channel.enabled ? 'Enabled' : 'Disabled'} · {channel.healthy
                    ? 'Healthy'
                    : 'Not healthy'}</small
                >
              </p>
              <dl>
                <div>
                  <dt>Last event</dt>
                  <dd>
                    {channel.lastEventAt ? new Date(channel.lastEventAt).toLocaleString() : 'None'}
                  </dd>
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
