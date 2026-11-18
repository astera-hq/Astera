/**
 * Tests for useSseEvents React Hook (Issue #157)
 *
 * Tests cover:
 * - Hook initialization
 * - Start/stop polling on mount/unmount
 * - Role and interval configuration
 * - Cleanup on unmount
 */

import { renderHook, act } from '@testing-library/react';
import { useSseEvents } from '../../hooks/useSseEvents';
import { sseEventsService } from '../../lib/sse-events';
import { useStore } from '../../lib/store';

// ---- Mocks ----

jest.mock('../../lib/sse-events', () => ({
  sseEventsService: {
    start: jest.fn(),
    stop: jest.fn(),
    setRole: jest.fn(),
    setPollInterval: jest.fn(),
    isActive: jest.fn(() => false),
  },
  DEFAULT_POLL_INTERVAL_MS: 15000,
}));

jest.mock('../../lib/store', () => ({
  useStore: jest.fn(() => jest.fn()),
}));

describe('useSseEvents Hook', () => {
  beforeEach(() => {
    jest.clearAllMocks();
    (sseEventsService.isActive as jest.Mock).mockReturnValue(false);
    jest.useFakeTimers();
  });

  afterEach(() => {
    jest.useRealTimers();
  });

  // ---- Test 1: Hook initializes and starts polling ----
  it('should start polling on mount when enabled', () => {
    (useStore as unknown as jest.Mock).mockReturnValue(jest.fn());

    renderHook(() => useSseEvents({ enabled: true }));

    expect(sseEventsService.start).toHaveBeenCalledWith({
      intervalMs: 15000,
      role: 'Admin',
    });
  });

  // ---- Test 2: Hook respects disabled state ----
  it('should not start polling when disabled', () => {
    (useStore as unknown as jest.Mock).mockReturnValue(jest.fn());

    renderHook(() => useSseEvents({ enabled: false }));

    expect(sseEventsService.start).not.toHaveBeenCalled();
  });

  // ---- Test 3: Hook stops polling on unmount ----
  it('should stop polling on unmount', () => {
    (useStore as unknown as jest.Mock).mockReturnValue(jest.fn());

    const { unmount } = renderHook(() => useSseEvents({ enabled: true }));

    unmount();

    expect(sseEventsService.stop).toHaveBeenCalled();
  });

  // ---- Test 4: Hook passes custom role ----
  it('should pass custom role to service', () => {
    (useStore as unknown as jest.Mock).mockReturnValue(jest.fn());

    renderHook(() => useSseEvents({ role: 'SME', enabled: true }));

    expect(sseEventsService.setRole).toHaveBeenCalledWith('SME');
    expect(sseEventsService.start).toHaveBeenCalledWith({
      intervalMs: 15000,
      role: 'SME',
    });
  });

  // ---- Test 5: Hook passes custom interval ----
  it('should pass custom interval to service', () => {
    (useStore as unknown as jest.Mock).mockReturnValue(jest.fn());

    renderHook(() => useSseEvents({ intervalMs: 10000, enabled: true }));

    expect(sseEventsService.start).toHaveBeenCalledWith({
      intervalMs: 10000,
      role: 'Admin',
    });
  });

  // ---- Test 6: Hook returns setPollingInterval function ----
  it('should return setPollingInterval function', () => {
    const mockSetPollingInterval = jest.fn();
    (useStore as unknown as jest.Mock).mockReturnValue(mockSetPollingInterval);

    const { result } = renderHook(() => useSseEvents({ enabled: true }));

    act(() => {
      result.current.setPollingInterval(20000);
    });

    expect(sseEventsService.setPollInterval).toHaveBeenCalledWith(20000);
    expect(mockSetPollingInterval).toHaveBeenCalledWith(20000);
  });

  // ---- Test 7: Hook defaults to Admin role ----
  it('should default to Admin role', () => {
    (useStore as unknown as jest.Mock).mockReturnValue(jest.fn());

    renderHook(() => useSseEvents({ enabled: true }));

    expect(sseEventsService.setRole).toHaveBeenCalledWith('Admin');
  });

  // ---- Test 8: Hook supports Investor role ----
  it('should support Investor role', () => {
    (useStore as unknown as jest.Mock).mockReturnValue(jest.fn());

    renderHook(() => useSseEvents({ role: 'Investor', enabled: true }));

    expect(sseEventsService.setRole).toHaveBeenCalledWith('Investor');
  });

  // ---- Test 9: Hook returns correct intervalMs ----
  it('should return the configured intervalMs', () => {
    (useStore as unknown as jest.Mock).mockReturnValue(jest.fn());

    const { result } = renderHook(() => useSseEvents({ intervalMs: 10000, enabled: true }));

    expect(result.current.intervalMs).toBe(10000);
  });

  // ---- Test 10: Hook updates role when it changes ----
  it('should update role when role prop changes', () => {
    (useStore as unknown as jest.Mock).mockReturnValue(jest.fn());

    const { rerender } = renderHook(
      ({ role }: { role: 'SME' | 'Investor' }) => useSseEvents({ role, enabled: true }),
      {
        initialProps: { role: 'SME' },
      },
    );

    expect(sseEventsService.setRole).toHaveBeenCalledWith('SME');

    rerender({ role: 'Investor' as const });

    expect(sseEventsService.setRole).toHaveBeenCalledWith('Investor');
  });
});
