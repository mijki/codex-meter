<script lang="ts">
  import QuotaAlertCard from './QuotaAlertCard.svelte';
  import type { AlertCenter as AlertCenterModel, QuotaAlert } from '../types';

  let {
    alerts,
    ondismiss,
    onmarkread,
  }: {
    alerts: AlertCenterModel;
    ondismiss: (alert: QuotaAlert) => void;
    onmarkread: () => void;
  } = $props();

  const unread = $derived(alerts.active.filter((alert) => alert.unread));
</script>

<section class="view-stack" aria-labelledby="alert-center-title">
  <div class="section-heading">
    <div>
      <p class="eyebrow">Persistent quota events</p>
      <h2 id="alert-center-title">Alert center</h2>
    </div>
    <p>{alerts.active.length} active</p>
  </div>

  <article class="panel">
    <div class="panel-head">
      <div>
        <p class="eyebrow">New since last review</p>
        <h2>Unread alerts</h2>
      </div>
      <span class="alert-count">{unread.length}</span>
    </div>
    {#if unread.length}
      <button class="button secondary" onclick={onmarkread}>Mark all active alerts read</button>
      <div class="alert-history-list">
        {#each unread as alert (alert.id)}
          <div>
            <strong>{alert.bucketName} · {alert.severity}</strong>
            <span
              >{alert.alertType.replaceAll('_', ' ')} · created
              {new Date(alert.createdAt).toLocaleString()}</span
            >
          </div>
        {/each}
      </div>
    {:else}
      <p class="muted-copy">No unread active alerts.</p>
    {/if}
  </article>

  <article class="panel">
    <div class="panel-head">
      <div>
        <p class="eyebrow">Needs attention</p>
        <h2>Active alerts</h2>
      </div>
    </div>
    {#if alerts.active.length}
      <div class="alert-grid">
        {#each alerts.active as alert (alert.id)}
          <QuotaAlertCard {alert} {ondismiss} />
        {/each}
      </div>
    {:else}
      <p class="muted-copy">No active quota alerts.</p>
    {/if}
  </article>

  <article class="panel">
    <div class="panel-head">
      <div>
        <p class="eyebrow">Acknowledged</p>
        <h2>Dismissed alerts</h2>
      </div>
    </div>
    {#if alerts.dismissed.length}
      <div class="alert-history-list">
        {#each alerts.dismissed as alert (alert.id)}
          <div>
            <strong>{alert.bucketName} · {alert.severity}</strong>
            <span
              >{alert.usedPercent}% used · dismissed {new Date(
                alert.dismissedAt ?? '',
              ).toLocaleString()}</span
            >
          </div>
        {/each}
      </div>
    {:else}
      <p class="muted-copy">No dismissed alerts.</p>
    {/if}
  </article>

  <article class="panel">
    <div class="panel-head">
      <div>
        <p class="eyebrow">Audit trail</p>
        <h2>Alert history</h2>
      </div>
    </div>
    {#if alerts.history.length}
      <div class="alert-history-list">
        {#each alerts.history as alert (alert.id)}
          <div>
            <strong
              >{alert.bucketName} · {alert.severity} · {alert.alertType.replaceAll(
                '_',
                ' ',
              )}</strong
            >
            <span>
              Created {new Date(alert.createdAt).toLocaleString()} · reset window {alert.resetWindowId}
              · {alert.accuracy.replace('_', ' ')}
              {alert.resolvedAt ? ` · resolved ${new Date(alert.resolvedAt).toLocaleString()}` : ''}
            </span>
          </div>
        {/each}
      </div>
    {:else}
      <p class="muted-copy">No quota threshold has been crossed in the retained history.</p>
    {/if}
  </article>
</section>
