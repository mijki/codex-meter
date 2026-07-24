// @vitest-environment jsdom

import './test-setup';

import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import App from './App.svelte';
import { fixtureDashboard } from './lib/fixture';
import type {
  AppSettings,
  CollectorDiagnostics,
  Dashboard,
  QuotaAlert,
  TelemetryState,
} from './lib/types';

const defaultSettings: AppSettings = {
  collectionEnabled: true,
  minimizeToTray: true,
  launchAtLogin: false,
  retentionDays: 90,
  quotaThresholds: [50, 75, 90, 100],
  rawEventRetentionEnabled: false,
};

const defaultDiagnostics: CollectorDiagnostics = {
  health: 'healthy',
  restartCount: 0,
  pendingRequests: 0,
  childRunning: true,
  executable: 'codex',
  transport: 'stdio',
  databasePath: 'local-app-data/codex-meter.sqlite',
  logPath: null,
  schemaVersion: '0.144.1 · v1 + v2 + v3',
  resetInterpretation: 'Unix seconds',
  latestError: null,
};

const apiMocks = vi.hoisted(() => ({
  deleteLocalData: vi.fn(),
  dismissAlert: vi.fn(),
  enterDemoMode: vi.fn(),
  exportData: vi.fn(),
  getCollectorDiagnostics: vi.fn(),
  getDashboard: vi.fn(),
  getOtelConfigSnippet: vi.fn(),
  getSettings: vi.fn(),
  saveSettings: vi.fn(),
}));

vi.mock('./lib/api', () => apiMocks);

function buildDashboard(overrides: Partial<Dashboard> = {}): Dashboard {
  return { ...structuredClone(fixtureDashboard), demoMode: false, ...overrides };
}

function waitingDetailedDashboard(state: TelemetryState = 'waiting'): Dashboard {
  const dashboard = buildDashboard({
    today: {
      input: null,
      cachedInput: null,
      output: null,
      reasoningOutput: null,
      total: null,
      accuracy: 'unavailable',
    },
    turns: [],
    modelDistribution: [],
    reasoningDistribution: [],
  });
  dashboard.channels = dashboard.channels.map((channel) =>
    ['lifecycle-hooks', 'opentelemetry'].includes(channel.id)
      ? {
          ...channel,
          enabled: true,
          healthy: state === 'waiting',
          status: state === 'error' ? 'degraded' : 'healthy',
        }
      : channel,
  );
  dashboard.tokenStatus = {
    state,
    accuracy: 'unavailable',
    source: 'Lifecycle hooks or OpenTelemetry',
    lastObservedAt: null,
    reason:
      state === 'waiting'
        ? 'Waiting for the first completed turn that supplies Token composition.'
        : 'Token composition collection was attempted and failed.',
    requiredIntegration: 'Complete a turn after detailed telemetry is enabled',
  };
  dashboard.turnsStatus = {
    ...dashboard.tokenStatus,
    reason: 'Waiting for the first completed turn that supplies Recent expensive turns.',
  };
  dashboard.modelStatus = {
    ...dashboard.tokenStatus,
    reason: 'Waiting for the first completed turn that supplies Models and reasoning.',
  };
  dashboard.burnStatus = {
    ...dashboard.tokenStatus,
    reason: 'Usage Burn is missing required signals.',
  };
  dashboard.burn = {
    headline: 'Usage Burn needs more evidence.',
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
  };
  return dashboard;
}

function alertWith(
  severity: QuotaAlert['severity'],
  usedPercent: number,
  threshold: number,
): QuotaAlert {
  return {
    ...structuredClone(fixtureDashboard.alerts.active[0]!),
    id: `alert-${severity}`,
    severity,
    usedPercent,
    remainingPercent: 100 - usedPercent,
    threshold,
    bucketName: `${severity} bucket`,
  };
}

describe('telemetry-state overview', () => {
  beforeEach(() => {
    Object.values(apiMocks).forEach((mock) => mock.mockReset());
    apiMocks.getDashboard.mockResolvedValue(buildDashboard());
    apiMocks.getCollectorDiagnostics.mockResolvedValue(defaultDiagnostics);
    apiMocks.getSettings.mockResolvedValue(defaultSettings);
    apiMocks.enterDemoMode.mockResolvedValue(structuredClone(fixtureDashboard));
    apiMocks.dismissAlert.mockImplementation(async () =>
      buildDashboard({ alerts: { active: [], dismissed: [], history: [] } }),
    );
  });

  afterEach(cleanup);

  it('shows a loading state while live telemetry is opening', () => {
    apiMocks.getDashboard.mockReturnValueOnce(new Promise(() => {}));
    render(App);
    expect(screen.getByLabelText('Loading')).toBeInTheDocument();
  });

  it('shows waiting token state without rendering numeric zero', async () => {
    apiMocks.getDashboard.mockResolvedValueOnce(waitingDetailedDashboard());
    render(App);
    const panel = (await screen.findByRole('heading', { name: 'Token composition' })).closest(
      'article',
    );
    expect(panel).not.toBeNull();
    expect(within(panel!).getByLabelText('Telemetry state: Waiting')).toBeInTheDocument();
    expect(within(panel!).queryByText('total reported tokens')).not.toBeInTheDocument();
  });

  it('renders an explicitly reported zero token total as zero', async () => {
    const dashboard = waitingDetailedDashboard();
    dashboard.today = {
      input: 0,
      cachedInput: 0,
      output: 0,
      reasoningOutput: 0,
      total: 0,
      accuracy: 'reported_exact',
    };
    dashboard.tokenStatus = {
      ...dashboard.tokenStatus,
      state: 'live',
      accuracy: 'reported_exact',
      reason: null,
      lastObservedAt: '2026-07-23T12:00:00Z',
    };
    apiMocks.getDashboard.mockResolvedValueOnce(dashboard);
    render(App);
    const panel = (await screen.findByRole('heading', { name: 'Token composition' })).closest(
      'article',
    );
    expect(within(panel!).getByText('total reported tokens')).toBeInTheDocument();
    expect(within(panel!).getAllByText('0').length).toBeGreaterThan(0);
  });

  it('consolidates disabled model and turn panels into one integration card', async () => {
    const dashboard = buildDashboard();
    dashboard.channels = dashboard.channels.map((channel) =>
      ['lifecycle-hooks', 'opentelemetry'].includes(channel.id)
        ? { ...channel, enabled: false, healthy: false, status: 'inactive' }
        : channel,
    );
    apiMocks.getDashboard.mockResolvedValueOnce(dashboard);
    render(App);
    expect(
      await screen.findByRole('heading', { name: 'Detailed turn telemetry is not configured.' }),
    ).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'Models & reasoning' })).not.toBeInTheDocument();
  });

  it('distinguishes an expensive-turn waiting state', async () => {
    apiMocks.getDashboard.mockResolvedValueOnce(waitingDetailedDashboard());
    render(App);
    const panel = (await screen.findByRole('heading', { name: 'Recent expensive turns' })).closest(
      'article',
    );
    expect(within(panel!).getByLabelText('Telemetry state: Waiting')).toBeInTheDocument();
    expect(within(panel!).getByText(/Waiting for the first completed turn/i)).toBeInTheDocument();
  });

  it('lists Usage Burn missing signals with their sources and setup states', async () => {
    apiMocks.getDashboard.mockResolvedValueOnce(waitingDetailedDashboard());
    render(App);
    await fireEvent.click(await screen.findByRole('button', { name: 'Usage Burn' }));
    expect(await screen.findByText('Token usage')).toBeInTheDocument();
    expect(
      screen.getAllByText(/Lifecycle hooks or OpenTelemetry · disabled/i).length,
    ).toBeGreaterThan(0);
    expect(screen.getByText(/No reliable quota snapshot is available/i)).toBeInTheDocument();
  });

  it.each([
    ['informational', 50, 50, 'Informational'],
    ['warning', 75, 75, 'Warning'],
    ['critical', 90, 90, 'Critical'],
    ['exhausted', 100, 100, 'Exhausted'],
  ] as const)(
    'renders %s alert severity with an accessible label',
    async (severity, used, threshold, label) => {
      const alert = alertWith(severity, used, threshold);
      apiMocks.getDashboard.mockResolvedValueOnce(
        buildDashboard({ alerts: { active: [alert], dismissed: [], history: [alert] } }),
      );
      render(App);
      await fireEvent.click(await screen.findByRole('button', { name: 'Alerts' }));
      expect(
        await screen.findByLabelText(`${label} quota alert for ${severity} bucket`),
      ).toBeInTheDocument();
    },
  );

  it('dismisses a live alert while retaining the history contract', async () => {
    const alert = alertWith('warning', 80, 75);
    const dismissed = { ...alert, dismissedAt: '2026-07-23T12:30:00Z' };
    apiMocks.getDashboard.mockResolvedValueOnce(
      buildDashboard({ alerts: { active: [alert], dismissed: [], history: [alert] } }),
    );
    apiMocks.dismissAlert.mockResolvedValueOnce(
      buildDashboard({ alerts: { active: [], dismissed: [dismissed], history: [dismissed] } }),
    );
    render(App);
    await fireEvent.click(
      await screen.findByRole('button', { name: /Dismiss Warning alert for warning bucket/i }),
    );
    expect(apiMocks.dismissAlert).toHaveBeenCalledWith(alert);
    await fireEvent.click(screen.getByRole('button', { name: 'Alerts' }));
    expect((await screen.findAllByText(/warning bucket · warning/i)).length).toBeGreaterThan(0);
  });

  it('switches explicitly between Live and non-persistent Demo mode', async () => {
    const live = buildDashboard({ quota: [] });
    apiMocks.getDashboard.mockResolvedValue(live);
    render(App);
    await fireEvent.click(await screen.findByRole('button', { name: 'Demo' }));
    expect((await screen.findAllByText('DEMO DATA')).length).toBeGreaterThan(0);
    expect(screen.getByText(/Nothing here is persisted/i)).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Live' }));
    await waitFor(() => expect(screen.queryByText('DEMO DATA')).not.toBeInTheDocument());
    expect(apiMocks.getDashboard).toHaveBeenCalledTimes(2);
  });

  it('exposes source health timestamps, capabilities, and historical-error wording', async () => {
    const dashboard = buildDashboard();
    dashboard.channels[0] = {
      ...dashboard.channels[0]!,
      latestError: {
        occurredAt: '2026-07-23T10:00:00Z',
        message: 'Recovered timeout',
        historical: true,
      },
      lastSuccessfulCollectionAt: '2026-07-23T11:00:00Z',
    };
    apiMocks.getDashboard.mockResolvedValueOnce(dashboard);
    render(App);
    expect(await screen.findByText('Last historical error')).toBeInTheDocument();
    expect(screen.getByText(/Supplies: Quota windows, Account activity/i)).toBeInTheDocument();
    expect(screen.getAllByText('Last success').length).toBeGreaterThan(0);
  });
});
