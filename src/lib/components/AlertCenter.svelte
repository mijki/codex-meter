<script lang="ts">
  import QuotaAlertCard from './QuotaAlertCard.svelte';
  import type { AlertCenter as AlertCenterModel, QuotaAlert } from '../types';

  let {
    alerts,
    ondismiss,
  }: {
    alerts: AlertCenterModel;
    ondismiss: (alert: QuotaAlert) => void;
  } = $props();
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
            <strong>{alert.bucketName} · {alert.severity} · {alert.threshold}% threshold</strong>
            <span>
              Created {new Date(alert.createdAt).toLocaleString()} · reset window {alert.resetWindowId}
              · {alert.accuracy.replace('_', ' ')}
            </span>
          </div>
        {/each}
      </div>
    {:else}
      <p class="muted-copy">No quota threshold has been crossed in the retained history.</p>
    {/if}
  </article>
</section>
