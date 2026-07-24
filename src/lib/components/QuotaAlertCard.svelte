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
  <p class="alert-usage">
    <strong>{alert.usedPercent.toFixed(0)}% used</strong>
    <span>{alert.remainingPercent.toFixed(0)}% remaining</span>
  </p>
  <dl class="alert-facts">
    <div>
      <dt>Threshold crossed</dt>
      <dd>{alert.threshold}%</dd>
    </div>
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
  </dl>
  {#if ondismiss && !alert.dismissedAt}
    <button
      class="button secondary alert-dismiss"
      aria-label={`Dismiss ${severityLabels[alert.severity]} alert for ${alert.bucketName}`}
      onclick={() => ondismiss?.(alert)}>Dismiss</button
    >
  {/if}
</article>
