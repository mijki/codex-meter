export type Accuracy = 'reported_exact' | 'derived_exact' | 'estimated' | 'unavailable';
export type TelemetryState =
  'live' | 'waiting' | 'disabled' | 'unsupported' | 'unavailable' | 'error' | 'fixture';

export interface TelemetryStatus {
  state: TelemetryState;
  accuracy: Accuracy;
  source: string;
  lastObservedAt: string | null;
  reason: string | null;
  requiredIntegration: string | null;
}

export type AlertSeverity = 'neutral' | 'informational' | 'warning' | 'critical' | 'exhausted';

export interface QuotaAlert {
  id: string;
  createdAt: string;
  collectionTimestamp: string;
  bucketId: string;
  bucketName: string;
  resetWindowId: string;
  resetsAt: string | null;
  threshold: number;
  usedPercent: number;
  remainingPercent: number;
  severity: AlertSeverity;
  accuracy: Accuracy;
  dismissedAt: string | null;
}

export interface AlertCenter {
  active: QuotaAlert[];
  dismissed: QuotaAlert[];
  history: QuotaAlert[];
}

export interface QuotaWindow {
  id: string;
  sourceLimitId?: string | null;
  name: string;
  windowKind?: string;
  windowDurationMinutes?: number | null;
  windowLabel: string;
  usedPercent: number | null;
  remainingPercent: number | null;
  resetsAt: string | null;
  resetsAtRaw?: number | null;
  resetInterpretation?: string;
  accuracy: Accuracy;
}

export interface AccountUsageDailyBucket {
  startDate: string;
  tokens: number;
  observedAt: string;
  accuracy: Accuracy;
}

export interface AccountUsageSummary {
  observedAt: string;
  lifetimeTokens: number | null;
  peakDailyTokens: number | null;
  longestRunningTurnSec: number | null;
  currentStreakDays: number | null;
  longestStreakDays: number | null;
  dailyUsageBuckets: AccountUsageDailyBucket[];
  accuracy: Accuracy;
}

export interface TokenTotals {
  input: number | null;
  cachedInput: number | null;
  output: number | null;
  reasoningOutput: number | null;
  total: number | null;
  accuracy: Accuracy;
}

export interface ExpensiveTurn {
  id: string;
  threadId: string;
  project: string | null;
  model: string | null;
  reasoningEffort: string | null;
  tokens: number;
  durationMs: number | null;
  accuracy: Accuracy;
}

export interface BurnAnalysis {
  headline: string;
  factors: string[];
  missingSignals: MissingSignal[];
  method: string;
  confidence: 'high' | 'medium' | 'low';
  accuracy: Accuracy;
}

export interface MissingSignal {
  name: string;
  source: string;
  setupState: TelemetryState;
  detail: string;
}

export interface SourceError {
  occurredAt: string;
  message: string;
  historical: boolean;
}

export interface ChannelHealth {
  id: string;
  name: string;
  status: 'healthy' | 'inactive' | 'degraded' | 'unavailable';
  enabled: boolean;
  healthy: boolean;
  detail: string;
  lastEventAt: string | null;
  lastSuccessfulCollectionAt: string | null;
  latestError: SourceError | null;
  capabilities: string[];
}

export interface Dashboard {
  demoMode: boolean;
  collectorState: string;
  codexVersion: string;
  lastEventAt: string | null;
  quota: QuotaWindow[];
  quotaStatus: TelemetryStatus;
  accountUsage: AccountUsageSummary | null;
  accountUsageStatus: TelemetryStatus;
  today: TokenTotals;
  tokenStatus: TelemetryStatus;
  turns: ExpensiveTurn[];
  turnsStatus: TelemetryStatus;
  burn: BurnAnalysis;
  burnStatus: TelemetryStatus;
  channels: ChannelHealth[];
  modelStatus: TelemetryStatus;
  alerts: AlertCenter;
  modelDistribution: Array<{ label: string; value: number }>;
  reasoningDistribution: Array<{ label: string; value: number }>;
  warnings: string[];
}

export interface AppSettings {
  collectionEnabled: boolean;
  minimizeToTray: boolean;
  launchAtLogin: boolean;
  retentionDays: number;
  quotaThresholds: number[];
  rawEventRetentionEnabled: boolean;
}

export interface CollectorDiagnostics {
  health: string;
  restartCount: number;
  pendingRequests: number;
  childRunning: boolean;
  executable: string | null;
  transport: string;
  databasePath: string | null;
  logPath: string | null;
  schemaVersion: string;
  resetInterpretation: string;
  latestError: string | null;
}
