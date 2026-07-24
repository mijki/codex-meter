// @vitest-environment jsdom

import '../../test-setup';

import { cleanup, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it } from 'vitest';
import TelemetryState from './TelemetryState.svelte';
import type { TelemetryState as State } from '../types';

describe('TelemetryState', () => {
  afterEach(cleanup);

  it.each([
    'live',
    'waiting',
    'disabled',
    'unsupported',
    'unavailable',
    'error',
    'fixture',
  ] as const)('renders the shared %s state', (state: State) => {
    render(TelemetryState, {
      status: {
        state,
        accuracy: state === 'live' ? 'reported_exact' : 'unavailable',
        source: 'Deterministic test source',
        lastObservedAt: state === 'live' ? '2026-07-23T12:00:00Z' : null,
        reason: state === 'live' ? null : `${state} reason`,
        requiredIntegration: 'Test integration',
      },
    });
    expect(
      screen.getByLabelText(
        `Telemetry state: ${state === 'fixture' ? 'Demo data' : state.charAt(0).toUpperCase() + state.slice(1)}`,
      ),
    ).toBeInTheDocument();
  });
});
