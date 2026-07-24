<script lang="ts">
  import { onMount } from 'svelte';
  import AccuracyBadge from './lib/components/AccuracyBadge.svelte';
  import AlertCenter from './lib/components/AlertCenter.svelte';
  import Overview from './lib/components/Overview.svelte';
  import TelemetryState from './lib/components/TelemetryState.svelte';
  import {
    deleteLocalData,
    dismissAlert,
    enterDemoMode,
    exportData,
    getCollectorDiagnostics,
    getDashboard,
    getOtelConfigSnippet,
    getSettings,
    saveSettings,
  } from './lib/api';
  import type { AppSettings, CollectorDiagnostics, Dashboard, QuotaAlert } from './lib/types';

  type View =
    | 'Overview'
    | 'Alerts'
    | 'Usage Burn'
    | 'Projects'
    | 'Chats'
    | 'Turns'
    | 'Models'
    | 'History'
    | 'Diagnostics'
    | 'Settings';

  const views: Array<{ name: View; glyph: string }> = [
    { name: 'Overview', glyph: '◌' },
    { name: 'Alerts', glyph: '!' },
    { name: 'Usage Burn', glyph: '↗' },
    { name: 'Projects', glyph: '◇' },
    { name: 'Chats', glyph: '◫' },
    { name: 'Turns', glyph: '↳' },
    { name: 'Models', glyph: '⬡' },
    { name: 'History', glyph: '◈' },
    { name: 'Diagnostics', glyph: '◉' },
    { name: 'Settings', glyph: '⚙' },
  ];

  let active: View = 'Overview';
  let dashboard: Dashboard | null = null;
  let liveDashboard: Dashboard | null = null;
  let settings: AppSettings | null = null;
  let diagnostics: CollectorDiagnostics | null = null;
  let displayMode: 'live' | 'demo' = 'live';
  let loading = true;
  let busy = false;
  let error = '';
  let notice = '';
  let deleteArmed = false;
  let otelSnippet = '';

  onMount(async () => {
    try {
      [liveDashboard, settings, diagnostics] = await Promise.all([
        getDashboard(),
        getSettings(),
        getCollectorDiagnostics(),
      ]);
      dashboard = liveDashboard;
    } catch (caught) {
      error = caught instanceof Error ? caught.message : 'Could not open local telemetry.';
    } finally {
      loading = false;
    }
  });

  onMount(() => {
    if (!('__TAURI_INTERNALS__' in window)) return;
    let disposed = false;
    let refreshing = false;
    const unlisten: Array<() => void> = [];

    const refreshLiveData = async (): Promise<void> => {
      if (refreshing) return;
      refreshing = true;
      try {
        const [nextDashboard, nextDiagnostics] = await Promise.all([
          getDashboard(),
          getCollectorDiagnostics(),
        ]);
        liveDashboard = nextDashboard;
        diagnostics = nextDiagnostics;
        if (displayMode === 'live') dashboard = nextDashboard;
      } catch (caught) {
        error = caught instanceof Error ? caught.message : 'Live telemetry refresh failed.';
      } finally {
        refreshing = false;
      }
    };

    void (async () => {
      const { listen } = await import('@tauri-apps/api/event');
      const stopStatus = await listen('collector-status', () => void refreshLiveData());
      const stopThreshold = await listen<{
        bucketName: string;
        threshold: number;
        usedPercent: number;
        severity: string;
      }>('quota-threshold', ({ payload }) => {
        void refreshLiveData();
        if (displayMode === 'live') {
          notice = `${payload.severity} quota alert: ${payload.bucketName} crossed ${payload.threshold}% (${payload.usedPercent.toFixed(0)}% used).`;
        }
      });
      if (disposed) {
        stopStatus();
        stopThreshold();
      } else {
        unlisten.push(stopStatus, stopThreshold);
      }
    })();

    return () => {
      disposed = true;
      unlisten.forEach((stop) => stop());
    };
  });

  const formatNumber = (value: number): string =>
    new Intl.NumberFormat(undefined, { notation: value > 999_999 ? 'compact' : 'standard' }).format(
      value,
    );

  const formatDuration = (duration: number | null): string => {
    if (duration === null) return 'Unavailable';
    const minutes = Math.round(duration / 60_000);
    return minutes > 59 ? `${Math.floor(minutes / 60)}h ${minutes % 60}m` : `${minutes}m`;
  };

  const switchToDemo = async (): Promise<void> => {
    busy = true;
    error = '';
    try {
      dashboard = await enterDemoMode();
      displayMode = 'demo';
      notice = 'Demo mode enabled. Demo data is synthetic, non-persistent, and notification-safe.';
    } finally {
      busy = false;
    }
  };

  const switchToLive = async (): Promise<void> => {
    busy = true;
    error = '';
    try {
      liveDashboard = await getDashboard();
      dashboard = liveDashboard;
      displayMode = 'live';
      notice = 'Live telemetry restored.';
    } catch (caught) {
      error = caught instanceof Error ? caught.message : 'Live telemetry could not be restored.';
    } finally {
      busy = false;
    }
  };

  const handleDismiss = async (alert: QuotaAlert): Promise<void> => {
    if (!dashboard) return;
    if (displayMode === 'demo') {
      const dismissed = { ...alert, dismissedAt: new Date().toISOString() };
      dashboard = {
        ...dashboard,
        alerts: {
          active: dashboard.alerts.active.filter((item) => item.id !== alert.id),
          dismissed: [dismissed, ...dashboard.alerts.dismissed],
          history: dashboard.alerts.history.length
            ? dashboard.alerts.history
            : [...dashboard.alerts.active],
        },
      };
      notice = 'Demo alert dismissed in memory only.';
      return;
    }
    try {
      dashboard = await dismissAlert(alert);
      liveDashboard = dashboard;
      notice = 'Alert dismissed. It remains in alert history.';
    } catch (caught) {
      error = caught instanceof Error ? caught.message : 'Alert could not be dismissed.';
    }
  };

  const persistSettings = async (): Promise<void> => {
    if (!settings) return;
    busy = true;
    try {
      await saveSettings(settings);
      notice = 'Settings saved locally.';
    } catch (caught) {
      error = caught instanceof Error ? caught.message : 'Settings could not be saved.';
    } finally {
      busy = false;
    }
  };

  const runExport = async (format: 'json' | 'csv' | 'diagnostics'): Promise<void> => {
    busy = true;
    try {
      const path = await exportData(format);
      notice = `Export created: ${path}`;
    } catch (caught) {
      error = caught instanceof Error ? caught.message : 'Export failed.';
    } finally {
      busy = false;
    }
  };

  const clearData = async (): Promise<void> => {
    if (!deleteArmed) {
      deleteArmed = true;
      return;
    }
    busy = true;
    try {
      liveDashboard = await deleteLocalData();
      if (displayMode === 'live') dashboard = liveDashboard;
      notice = 'All local analytics data was deleted.';
      deleteArmed = false;
    } catch (caught) {
      error = caught instanceof Error ? caught.message : 'Local data could not be deleted.';
    } finally {
      busy = false;
    }
  };

  const showOtelSnippet = async (): Promise<void> => {
    otelSnippet = await getOtelConfigSnippet();
  };

  const copyOtelSnippet = async (): Promise<void> => {
    if (!otelSnippet) await showOtelSnippet();
    await navigator.clipboard.writeText(otelSnippet);
    notice = 'OpenTelemetry configuration copied for review.';
  };
</script>

<svelte:head><meta name="color-scheme" content="dark light" /></svelte:head>

<div class="shell" class:demo-shell={displayMode === 'demo'}>
  <aside class="sidebar">
    <div class="brand">
      <div class="brand-mark" aria-hidden="true"><span></span></div>
      <div><strong>Codex Meter</strong><small>Local telemetry</small></div>
    </div>

    <nav aria-label="Primary navigation">
      {#each views as view (view.name)}
        <button class:active={active === view.name} onclick={() => (active = view.name)}>
          <span aria-hidden="true">{view.glyph}</span>{view.name}
          {#if view.name === 'Alerts' && (dashboard?.alerts.active.length ?? 0) > 0}
            <em
              class="nav-alert-count"
              aria-label={`${dashboard?.alerts.active.length ?? 0} active alerts`}
              >{dashboard?.alerts.active.length}</em
            >
          {/if}
        </button>
      {/each}
    </nav>

    <div class="sidebar-foot">
      <div class="health-dot" class:online={dashboard?.lastEventAt !== null}></div>
      <div>
        <strong
          >{displayMode === 'demo'
            ? 'DEMO DATA'
            : (dashboard?.collectorState ?? 'Starting collector')}</strong
        >
        <small
          >{displayMode === 'demo'
            ? 'Synthetic · not persisted'
            : 'Data remains on this device'}</small
        >
      </div>
    </div>
  </aside>

  <main>
    <header class="topbar">
      <div>
        <p class="eyebrow">Codex usage, with provenance</p>
        <h1>{active}</h1>
      </div>
      <div class="top-actions">
        <span class="version">Codex {dashboard?.codexVersion ?? 'detecting…'}</span>
        <div class="mode-selector" role="group" aria-label="Telemetry display mode">
          <button
            class:active={displayMode === 'live'}
            aria-pressed={displayMode === 'live'}
            onclick={switchToLive}
            disabled={busy}>Live</button
          >
          <button
            class:active={displayMode === 'demo'}
            aria-pressed={displayMode === 'demo'}
            onclick={switchToDemo}
            disabled={busy}>Demo</button
          >
        </div>
        <button
          class="notification-indicator"
          class:has-alerts={(dashboard?.alerts.active.length ?? 0) > 0}
          aria-label={`Open alert center, ${dashboard?.alerts.active.length ?? 0} active alerts`}
          onclick={() => (active = 'Alerts')}
        >
          <span aria-hidden="true">!</span>
          {dashboard?.alerts.active.length ?? 0}
        </button>
      </div>
    </header>

    {#if displayMode === 'demo'}
      <div class="demo-banner" role="status">
        <strong>DEMO DATA</strong> — every visible value is synthetic. Nothing here is persisted or eligible
        for native notifications.
      </div>
    {/if}
    {#if notice}<div class="notice" role="status">{notice}</div>{/if}
    {#if error}<div class="error" role="alert">{error}</div>{/if}

    {#if loading}
      <section class="loading-state" aria-label="Loading">
        <div class="spinner"></div>
        <p>Opening the local telemetry store…</p>
      </section>
    {:else if dashboard}
      {#if active === 'Overview'}
        <Overview
          {dashboard}
          onnavigate={(view) => (active = view)}
          ondismiss={handleDismiss}
          onenterdemo={switchToDemo}
        />
      {:else if active === 'Alerts'}
        <AlertCenter alerts={dashboard.alerts} ondismiss={handleDismiss} />
      {:else if active === 'Usage Burn'}
        <section class="view-stack burn-layout">
          <article class="burn-hero panel">
            <div class="panel-head">
              <div>
                <p class="eyebrow">Deterministic analysis</p>
                <h2>{dashboard.burn.headline}</h2>
              </div>
              <AccuracyBadge accuracy={dashboard.burnStatus.accuracy} />
            </div>
            {#if dashboard.burn.factors.length}
              <p class="analysis-note">Correlation only — no forecast or causal claim is made.</p>
              <div class="driver-list">
                {#each dashboard.burn.factors as factor, index (factor)}
                  <div><span>{index + 1}</span><strong>{factor}</strong></div>
                {/each}
              </div>
            {:else}
              <TelemetryState
                status={dashboard.burnStatus}
                onconfigure={() => (active = 'Settings')}
              />
            {/if}
          </article>
          <div class="overview-grid">
            <article class="panel">
              <p class="eyebrow">Method</p>
              <h2>How this was calculated</h2>
              <p class="body-copy">{dashboard.burn.method}</p>
              <dl class="facts">
                <div>
                  <dt>Confidence</dt>
                  <dd>{dashboard.burn.confidence}</dd>
                </div>
                <div>
                  <dt>Attribution</dt>
                  <dd>{dashboard.burn.accuracy.replace('_', ' ')}</dd>
                </div>
              </dl>
            </article>
            <article class="panel">
              <p class="eyebrow">Missing signals</p>
              <h2>Evidence and setup gaps</h2>
              <div class="signal-list">
                {#each dashboard.burn.missingSignals as signal (signal.name)}
                  <div data-state={signal.setupState}>
                    <strong>{signal.name}</strong>
                    <span>{signal.source} · {signal.setupState}</span>
                    <p>{signal.detail}</p>
                  </div>
                {/each}
              </div>
            </article>
          </div>
        </section>
      {:else if ['Projects', 'Chats', 'Turns', 'Models', 'History'].includes(active)}
        <section class="view-stack">
          <article class="panel data-panel">
            <div class="panel-head">
              <div>
                <p class="eyebrow">Normalized local history</p>
                <h2>{active}</h2>
              </div>
              <AccuracyBadge accuracy={dashboard.turnsStatus.accuracy} />
            </div>
            {#if dashboard.turns.length}
              <div class="data-table" role="table" aria-label={active}>
                <div class="data-row table-head" role="row">
                  <span>Identity</span><span>Project / model</span><span>Duration</span><span
                    >Tokens</span
                  ><span>Classification</span>
                </div>
                {#each dashboard.turns as turn (turn.id)}
                  <div class="data-row" role="row">
                    <strong
                      >{active === 'Chats'
                        ? turn.threadId
                        : active === 'Models'
                          ? (turn.model ?? 'Unavailable')
                          : turn.id}</strong
                    >
                    <span
                      >{turn.project ?? 'Unattributed'}<small
                        >{turn.model ?? 'Model unavailable'} · {turn.reasoningEffort ??
                          'effort unavailable'}</small
                      ></span
                    >
                    <span>{formatDuration(turn.durationMs)}</span>
                    <span>{formatNumber(turn.tokens)}</span>
                    <AccuracyBadge accuracy={turn.accuracy} />
                  </div>
                {/each}
              </div>
            {:else}
              <TelemetryState
                status={dashboard.turnsStatus}
                onconfigure={() => (active = 'Settings')}
              />
            {/if}
          </article>
        </section>
      {:else if active === 'Diagnostics'}
        <section class="view-stack">
          <div class="overview-grid diagnostics-grid">
            <article class="panel">
              <p class="eyebrow">Collector</p>
              <h2>Codex App Server</h2>
              <dl class="facts">
                <div>
                  <dt>Health</dt>
                  <dd>{diagnostics?.health ?? dashboard.collectorState}</dd>
                </div>
                <div>
                  <dt>Child process</dt>
                  <dd>{diagnostics?.childRunning ? 'Running' : 'Stopped'}</dd>
                </div>
                <div>
                  <dt>Pending requests</dt>
                  <dd>{diagnostics?.pendingRequests ?? 0}</dd>
                </div>
                <div>
                  <dt>Restarts</dt>
                  <dd>{diagnostics?.restartCount ?? 0}</dd>
                </div>
                <div>
                  <dt>Transport</dt>
                  <dd>{diagnostics?.transport ?? 'stdio'}</dd>
                </div>
              </dl>
            </article>
            <article class="panel">
              <p class="eyebrow">Schema</p>
              <h2>Capability status</h2>
              <dl class="facts">
                <div>
                  <dt>Codex version</dt>
                  <dd>{dashboard.codexVersion}</dd>
                </div>
                <div>
                  <dt>Schema/migrations</dt>
                  <dd>{diagnostics?.schemaVersion ?? 'Unavailable'}</dd>
                </div>
                <div>
                  <dt>Reset interpretation</dt>
                  <dd>{diagnostics?.resetInterpretation ?? 'Unavailable'}</dd>
                </div>
                <div>
                  <dt>Latest current error</dt>
                  <dd>{diagnostics?.latestError ?? 'None'}</dd>
                </div>
              </dl>
            </article>
            <article class="panel wide">
              <p class="eyebrow">Privacy</p>
              <h2>Local storage and redaction</h2>
              <dl class="facts">
                <div>
                  <dt>Database</dt>
                  <dd>{diagnostics?.databasePath ?? 'Desktop backend only'}</dd>
                </div>
                <div>
                  <dt>Logs</dt>
                  <dd>{diagnostics?.logPath ?? 'No file log configured'}</dd>
                </div>
                <div>
                  <dt>Prompts / responses</dt>
                  <dd>Not stored</dd>
                </div>
                <div>
                  <dt>Raw payloads</dt>
                  <dd>Discarded by allowlist adapters</dd>
                </div>
              </dl>
              <button class="button secondary" onclick={() => runExport('diagnostics')}
                >Export redacted diagnostics</button
              >
            </article>
          </div>
        </section>
      {:else if active === 'Settings' && settings}
        <section class="view-stack">
          <div class="overview-grid settings-layout">
            <article class="panel settings-card">
              <p class="eyebrow">Application</p>
              <h2>Local behavior</h2>
              <label class="toggle"
                ><span
                  ><strong>Collection enabled</strong><small
                    >Supervise the read-only App Server collector.</small
                  ></span
                ><input
                  aria-label="Collection enabled"
                  type="checkbox"
                  bind:checked={settings.collectionEnabled}
                /></label
              >
              <label class="toggle"
                ><span
                  ><strong>Minimize to tray on close</strong><small
                    >Use Exit to stop the application.</small
                  ></span
                ><input type="checkbox" bind:checked={settings.minimizeToTray} /></label
              >
              <label class="toggle"
                ><span
                  ><strong>Launch at Windows login</strong><small
                    >Disabled until explicitly enabled.</small
                  ></span
                ><input type="checkbox" bind:checked={settings.launchAtLogin} /></label
              >
              <label class="field"
                ><span>Retention period</span><select bind:value={settings.retentionDays}
                  ><option value={30}>30 days</option><option value={90}>90 days</option><option
                    value={180}>180 days</option
                  ><option value={365}>1 year</option></select
                ></label
              >
              <button class="button primary" onclick={persistSettings} disabled={busy}
                >Save settings</button
              >
            </article>
            <article class="panel settings-card" id="detailed-telemetry-settings">
              <p class="eyebrow">Detailed telemetry integrations</p>
              <h2>Manual, local setup</h2>
              <div class="setup-item">
                <strong>Lifecycle hooks</strong>
                <p>
                  Review the optional package manually. It is not installed or trusted by Codex
                  Meter.
                </p>
                <code>integrations/codex-plugin/</code>
              </div>
              <div class="setup-item">
                <strong>OpenTelemetry</strong>
                <p>
                  The local receiver is not enabled. The preview does not edit global Codex
                  configuration.
                </p>
                <div class="snippet-actions">
                  <button class="button secondary" onclick={showOtelSnippet}
                    >Show configuration</button
                  >
                  <button class="button secondary" onclick={copyOtelSnippet}
                    >Copy configuration</button
                  >
                </div>
                {#if otelSnippet}<pre class="config-snippet">{otelSnippet}</pre>{/if}
              </div>
            </article>
            <article class="panel settings-card wide">
              <p class="eyebrow">Data controls</p>
              <h2>Export or delete local analytics</h2>
              <div class="button-row">
                <button class="button secondary" onclick={() => runExport('json')}
                  >Export JSON</button
                >
                <button class="button secondary" onclick={() => runExport('csv')}>Export CSV</button
                >
                <button class:armed={deleteArmed} class="button danger" onclick={clearData}
                  >{deleteArmed ? 'Confirm delete all local data' : 'Delete all local data'}</button
                >
              </div>
            </article>
          </div>
        </section>
      {/if}
    {/if}

    <footer>
      Codex Meter is independent and is not affiliated with, endorsed by, or supported by OpenAI.
      Interfaces may change.
    </footer>
  </main>
</div>
