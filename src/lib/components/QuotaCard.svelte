<script lang="ts">
  import { onMount } from 'svelte';
  import AccuracyBadge from './AccuracyBadge.svelte';
  import type { QuotaWindow } from '../types';

  let { quota }: { quota: QuotaWindow } = $props();
  let now = $state(Date.now());
  type QuotaSeverity =
    'neutral' | 'informational' | 'warning' | 'critical' | 'exhausted' | 'unavailable';

  onMount(() => {
    const timer = window.setInterval(() => {
      now = Date.now();
    }, 1_000);
    return () => window.clearInterval(timer);
  });

  const formatReset = (value: string | null): string => {
    if (!value) return 'Reset unavailable';
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return 'Reset unit unavailable';
    const remainingSeconds = Math.max(0, Math.floor((date.getTime() - now) / 1_000));
    if (remainingSeconds === 0) return `Window completed ${date.toLocaleString()}`;

    const days = Math.floor(remainingSeconds / 86_400);
    const hours = Math.floor((remainingSeconds % 86_400) / 3_600);
    const minutes = Math.floor((remainingSeconds % 3_600) / 60);
    const seconds = remainingSeconds % 60;
    const countdown = days
      ? `${days}d ${hours}h ${minutes}m`
      : `${hours.toString().padStart(2, '0')}:${minutes.toString().padStart(2, '0')}:${seconds
          .toString()
          .padStart(2, '0')}`;
    return `Resets ${date.toLocaleString()} · ${countdown}`;
  };

  const severity = (used: number | null): QuotaSeverity => {
    if (used === null) return 'unavailable';
    if (used >= 100) return 'exhausted';
    if (used >= 90) return 'critical';
    if (used >= 75) return 'warning';
    if (used >= 50) return 'informational';
    return 'neutral';
  };
  const severityLabel = (used: number | null): string => {
    const labels: Record<QuotaSeverity, string> = {
      neutral: 'Neutral',
      informational: 'Informational',
      warning: 'Warning',
      critical: 'Critical',
      exhausted: 'Exhausted',
      unavailable: 'Unavailable',
    };
    return labels[severity(used)];
  };
  const severityIcon = (used: number | null): string => {
    const icons: Record<QuotaSeverity, string> = {
      neutral: '·',
      informational: 'i',
      warning: '!',
      critical: '!',
      exhausted: '×',
      unavailable: '?',
    };
    return icons[severity(used)];
  };
</script>

<article class="quota-card quota-{severity(quota.usedPercent)}">
  <div class="quota-heading">
    <div>
      <p class="eyebrow">{quota.name}</p>
      <h3>{quota.windowLabel}</h3>
    </div>
    <div class="quota-badges">
      <span
        class="severity-mini severity-{severity(quota.usedPercent)}"
        aria-label={`Quota severity: ${severityLabel(quota.usedPercent)}`}
      >
        <span aria-hidden="true">{severityIcon(quota.usedPercent)}</span>
        {severityLabel(quota.usedPercent)}
      </span>
      <AccuracyBadge accuracy={quota.accuracy} />
    </div>
  </div>
  <div class="quota-number">
    <strong>{quota.remainingPercent ?? '—'}<small>%</small></strong>
    <span>remaining</span>
    {#if quota.remainingPercent !== null}<AccuracyBadge accuracy="derived_exact" />{/if}
  </div>
  <div
    class="meter-track"
    aria-label={quota.usedPercent === null
      ? 'Used percentage unavailable'
      : `${quota.usedPercent}% used`}
  >
    <div class="meter-fill" style={`width: ${quota.usedPercent ?? 0}%`}></div>
  </div>
  <div class="quota-foot">
    <span>{quota.usedPercent ?? '—'}% used</span>
    <span>{formatReset(quota.resetsAt)}</span>
  </div>
</article>
