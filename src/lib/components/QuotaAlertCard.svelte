<script lang="ts">
  import { onMount } from 'svelte';
  import AccuracyBadge from './AccuracyBadge.svelte';
  import type { QuotaAlert } from '../types';

  let {
    alert,
    prominent = false,
    ondismiss,
  }: {
    alert: QuotaAlert;
    prominent?: boolean;
    ondismiss?: (alert: QuotaAlert) => void;
  } = $props();
  let now = $state(Date.now());

  const severityLabels = {
    neutral: 'Neutral',
    informational: 'Informational',
    warning: 'Warning',
    critical: 'Critical',
    exhausted: 'Exhausted',
  } as const;
  const alertTypeLabels: Record<string, string> = {
    quota_threshold: 'Quota threshold crossed',
    pace_exceeds_safe: 'Current pace exceeds safe rate',
    forecast_exhaustion_before_reset: 'Quota exhaustion forecast before reset',
    forecast_exhaustion_within_4h: 'Estimated quota exhaustion within 4 hours',
    forecast_exhaustion_within_1h: 'Estimated quota exhaustion within 1 hour',
    collector_unhealthy: 'Collector unhealthy',
    forecast_confidence_changed: 'Forecast confidence changed',
  };

  onMount(() => {
    const timer = window.setInterval(() => (now = Date.now()), 1_000);
    return () => window.clearInterval(timer);
  });

  const countdown = (reset: string | null): string => {
    if (!reset) return 'Reset countdown unavailable';
    const resetAt = new Date(reset).getTime();
    if (Number.isNaN(resetAt)) return 'Reset countdown unavailable';
    const seconds = Math.max(0, Math.floor((resetAt - now) / 1_000));
    const days = Math.floor(seconds / 86_400);
    const hours = Math.floor((seconds % 86_400) / 3_600);
    const minutes = Math.floor((seconds % 3_600) / 60);
    return seconds === 0 ? 'Window completed' : `${days ? `${days}d ` : ''}${hours}h ${minutes}m`;
  };
</script>

<article
  class="quota-alert severity-{alert.severity}"
  class:prominent
  aria-label={`${severityLabels[alert.severity]} quota alert for ${alert.bucketName}`}
>
  <div class="alert-heading">
    <div class="severity-label">
      <span class="severity-icon" aria-hidden="true">!</span>
      <strong>{severityLabels[alert.severity]}</strong>
    </div>
    <AccuracyBadge accuracy={alert.accuracy} />
  </div>
  <h3>{alert.bucketName}</h3>
  <p class="alert-type">
    {alertTypeLabels[alert.alertType] ?? alert.alertType.replaceAll('_', ' ')}
    {#if alert.unread}<span>Unread</span>{/if}
  </p>
  <p class="alert-usage">
    <strong>{alert.usedPercent.toFixed(0)}% used</strong>
    <span>{alert.remainingPercent.toFixed(0)}% remaining</span>
  </p>
  <dl class="alert-facts">
    {#if alert.alertType === 'quota_threshold'}
      <div>
        <dt>Threshold crossed</dt>
        <dd>{alert.threshold}%</dd>
      </div>
    {:else}
      <div>
        <dt>Alert source</dt>
        <dd>{alert.alertSource.replaceAll('_', ' ')}</dd>
      </div>
    {/if}
    <div>
      <dt>Reset</dt>
      <dd>{alert.resetsAt ? new Date(alert.resetsAt).toLocaleString() : 'Unavailable'}</dd>
    </div>
    <div>
      <dt>Countdown</dt>
      <dd>{countdown(alert.resetsAt)}</dd>
    </div>
    <div>
      <dt>Collected</dt>
      <dd>{new Date(alert.collectionTimestamp).toLocaleString()}</dd>
    </div>
    {#if alert.predictedExhaustionAt}
      <div>
        <dt>Estimated exhaustion</dt>
        <dd>{new Date(alert.predictedExhaustionAt).toLocaleString()}</dd>
      </div>
    {/if}
    {#if alert.forecastConfidence}
      <div>
        <dt>Forecast confidence</dt>
        <dd>{alert.forecastConfidence}</dd>
      </div>
    {/if}
  </dl>
  {#if ondismiss && !alert.dismissedAt}
    <button
      class="button secondary alert-dismiss"
      aria-label={`Dismiss ${severityLabels[alert.severity]} alert for ${alert.bucketName}`}
      onclick={() => ondismiss?.(alert)}>Dismiss</button
    >
  {/if}
</article>
