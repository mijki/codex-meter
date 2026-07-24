import type { AppSettings, CollectorDiagnostics, Dashboard, QuotaAlert } from './types';
import { fixtureDashboard } from './fixture';

const isTauri = (): boolean => '__TAURI_INTERNALS__' in window;

async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const tauri = await import('@tauri-apps/api/core');
  return tauri.invoke<T>(command, args);
}

export async function getDashboard(): Promise<Dashboard> {
  if (!isTauri()) {
    return emptyDashboard();
  }
  return invoke<Dashboard>('get_dashboard');
}

export async function enterDemoMode(): Promise<Dashboard> {
  return structuredClone(fixtureDashboard);
}

export async function dismissAlert(alert: QuotaAlert): Promise<Dashboard> {
  if (!isTauri()) {
    const dashboard = emptyDashboard();
    return dashboard;
  }
  await invoke<void>('dismiss_alert', {
    bucketId: alert.bucketId,
    resetWindowId: alert.resetWindowId,
    threshold: alert.threshold,
  });
  return getDashboard();
}

export async function markAlertsRead(): Promise<Dashboard> {
  if (!isTauri()) return emptyDashboard();
  await invoke<void>('mark_alerts_read');
  return getDashboard();
}

export async function getResolvedDatabasePath(): Promise<string> {
  if (!isTauri()) throw new Error('The resolved database path is available in the desktop app.');
  return invoke<string>('get_resolved_database_path');
}

export async function getSettings(): Promise<AppSettings> {
  if (!isTauri()) {
    return {
      collectionEnabled: true,
      minimizeToTray: true,
      launchAtLogin: false,
      retentionDays: 90,
      quotaThresholds: [75, 90],
      rawEventRetentionEnabled: false,
    };
  }
  return invoke<AppSettings>('get_settings');
}

export async function getCollectorDiagnostics(): Promise<CollectorDiagnostics> {
  if (!isTauri()) {
    return {
      health: 'disabled',
      restartCount: 0,
      pendingRequests: 0,
      childRunning: false,
      executable: null,
      transport: 'stdio',
      databasePath: '%APPDATA%\\dev.codexmeter.app\\codex-meter.sqlite',
      logPath: null,
      schemaVersion: '0.144.1 · v1 + v2 + v3 + v4',
      resetInterpretation: 'No live reset value observed',
      latestError: null,
    };
  }
  return invoke<CollectorDiagnostics>('get_collector_diagnostics');
}

export async function saveSettings(settings: AppSettings): Promise<void> {
  if (isTauri()) {
    await invoke<void>('save_settings', { settings });
    const autostart = await import('@tauri-apps/plugin-autostart');
    if (settings.launchAtLogin) {
      await autostart.enable();
    } else {
      await autostart.disable();
    }
  }
}

export async function getOtelConfigSnippet(): Promise<string> {
  if (!isTauri()) {
    return '[otel]\nlog_user_prompt = false\n# Start the desktop receiver before enabling export.';
  }
  return invoke<string>('get_otel_config_snippet');
}

export async function exportData(format: 'json' | 'csv' | 'diagnostics'): Promise<string> {
  if (!isTauri()) {
    throw new Error('Exports are available in the desktop application.');
  }
  return invoke<string>('export_data', { format });
}

export async function deleteLocalData(): Promise<Dashboard> {
  if (!isTauri()) {
    return emptyDashboard();
  }
  await invoke<void>('delete_local_data');
  return getDashboard();
}

export function emptyDashboard(): Dashboard {
  return {
    demoMode: false,
    collectorState: 'ready — no telemetry received',
    codexVersion: '0.144.1',
    lastEventAt: null,
    quota: [],
    quotaStatus: {
      state: 'unavailable',
      accuracy: 'unavailable',
      source: 'Codex App Server',
      lastObservedAt: null,
      reason: 'No reliable quota snapshot is available.',
      requiredIntegration: 'Read-only App Server collection',
    },
    forecasts: [],
    forecastStatus: {
      state: 'unavailable',
      accuracy: 'unavailable',
      source: 'Local deterministic forecast',
      lastObservedAt: null,
      reason: 'A quota forecast needs at least three compatible observations.',
      requiredIntegration: 'Read-only App Server quota history',
    },
    accountUsage: null,
    accountUsageStatus: {
      state: 'unavailable',
      accuracy: 'unavailable',
      source: 'Codex App Server',
      lastObservedAt: null,
      reason: 'No account usage response is available.',
      requiredIntegration: 'Read-only App Server collection',
    },
    today: {
      input: null,
      cachedInput: null,
      output: null,
      reasoningOutput: null,
      total: null,
      accuracy: 'unavailable',
    },
    tokenStatus: {
      state: 'disabled',
      accuracy: 'unavailable',
      source: 'Lifecycle hooks or OpenTelemetry',
      lastObservedAt: null,
      reason: 'Detailed turn telemetry is not configured.',
      requiredIntegration: 'Enable lifecycle hooks or the local OpenTelemetry receiver',
    },
    turns: [],
    turnsStatus: {
      state: 'disabled',
      accuracy: 'unavailable',
      source: 'Lifecycle hooks or OpenTelemetry',
      lastObservedAt: null,
      reason: 'Detailed turn telemetry is not configured.',
      requiredIntegration: 'Enable lifecycle hooks or the local OpenTelemetry receiver',
    },
    burn: {
      headline: 'Usage burn analysis needs telemetry from at least one completed turn.',
      factors: [],
      missingSignals: [
        {
          name: 'Token usage',
          source: 'Lifecycle hooks or OpenTelemetry',
          setupState: 'disabled',
          detail: 'Detailed turn telemetry is not configured.',
        },
        {
          name: 'Quota snapshots',
          source: 'Codex App Server',
          setupState: 'unavailable',
          detail: 'No reliable quota snapshot is available.',
        },
        {
          name: 'Completed turns',
          source: 'Lifecycle hooks or OpenTelemetry',
          setupState: 'disabled',
          detail: 'No completed turn event is available.',
        },
      ],
      method: 'No calculation performed.',
      confidence: 'low',
      accuracy: 'unavailable',
    },
    burnStatus: {
      state: 'disabled',
      accuracy: 'unavailable',
      source: 'Local deterministic analysis',
      lastObservedAt: null,
      reason: 'Detailed turn telemetry is not configured.',
      requiredIntegration: 'Token usage, completed turns, and quota snapshots',
    },
    channels: [
      {
        id: 'app-server',
        name: 'App Server',
        status: 'inactive',
        enabled: true,
        healthy: false,
        detail: 'Desktop backend unavailable in browser mode',
        lastEventAt: null,
        lastSuccessfulCollectionAt: null,
        latestError: null,
        capabilities: ['Quota windows', 'Account activity'],
        telemetryState: 'waiting',
        configurationStatus: 'Configured',
        restartCount: 0,
      },
      {
        id: 'lifecycle-hooks',
        name: 'Lifecycle hooks',
        status: 'inactive',
        enabled: false,
        healthy: false,
        detail: 'Not configured',
        lastEventAt: null,
        lastSuccessfulCollectionAt: null,
        latestError: null,
        capabilities: ['Turn completion', 'Model and reasoning', 'Attribution'],
        telemetryState: 'disabled',
        configurationStatus: 'Not configured',
        restartCount: 0,
      },
      {
        id: 'opentelemetry',
        name: 'OpenTelemetry',
        status: 'inactive',
        enabled: false,
        healthy: false,
        detail: 'Not configured',
        lastEventAt: null,
        lastSuccessfulCollectionAt: null,
        latestError: null,
        capabilities: ['Token composition', 'Usage Burn evidence'],
        telemetryState: 'disabled',
        configurationStatus: 'Not configured',
        restartCount: 0,
      },
    ],
    modelStatus: {
      state: 'disabled',
      accuracy: 'unavailable',
      source: 'Lifecycle hooks or OpenTelemetry',
      lastObservedAt: null,
      reason: 'Model and reasoning telemetry is not configured.',
      requiredIntegration: 'Enable lifecycle hooks or the local OpenTelemetry receiver',
    },
    alerts: { active: [], dismissed: [], history: [] },
    modelDistribution: [],
    reasoningDistribution: [],
    warnings: ['No telemetry is available. Load the synthetic fixture to explore the interface.'],
  };
}
