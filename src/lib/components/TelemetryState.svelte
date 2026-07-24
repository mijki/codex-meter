<script lang="ts">
  import type { TelemetryStatus } from '../types';

  let {
    status,
    compact = false,
    onconfigure,
  }: {
    status: TelemetryStatus;
    compact?: boolean;
    onconfigure?: () => void;
  } = $props();

  const labels = {
    live: 'Live',
    waiting: 'Waiting',
    disabled: 'Disabled',
    unsupported: 'Unsupported',
    unavailable: 'Unavailable',
    error: 'Error',
    fixture: 'Demo data',
  } as const;
</script>

<div
  class="telemetry-state"
  class:compact
  class:error-state={status.state === 'error'}
  class:fixture-state={status.state === 'fixture'}
  data-state={status.state}
  aria-label={`Telemetry state: ${labels[status.state]}`}
>
  <div class="state-title">
    <span class="state-icon" aria-hidden="true"></span>
    <strong>{labels[status.state]}</strong>
  </div>
  {#if status.reason}<p>{status.reason}</p>{/if}
  <dl>
    <div>
      <dt>Source</dt>
      <dd>{status.source}</dd>
    </div>
    <div>
      <dt>Last relevant event</dt>
      <dd>
        {status.lastObservedAt
          ? new Date(status.lastObservedAt).toLocaleString()
          : 'No event observed'}
      </dd>
    </div>
    {#if status.requiredIntegration}
      <div>
        <dt>Required integration</dt>
        <dd>{status.requiredIntegration}</dd>
      </div>
    {/if}
  </dl>
  {#if onconfigure && ['disabled', 'waiting', 'error'].includes(status.state)}
    <button class="button secondary state-action" onclick={onconfigure}>Open Settings</button>
  {/if}
</div>
