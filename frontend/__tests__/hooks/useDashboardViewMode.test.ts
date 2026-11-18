import { renderHook, act, waitFor } from '@testing-library/react';
import { useDashboardViewMode } from '../../hooks/useDashboardViewMode';
import { DASHBOARD_VIEW_MODES, DASHBOARD_VIEW_STORAGE_KEY } from '../../lib/dashboardPipeline';

beforeEach(() => {
  localStorage.clear();
});

describe('useDashboardViewMode', () => {
  it('default view mode is list', () => {
    const { result } = renderHook(() => useDashboardViewMode());
    expect(result.current.viewMode).toBe(DASHBOARD_VIEW_MODES.LIST);
  });

  it('toggling updates the value', () => {
    const { result } = renderHook(() => useDashboardViewMode());
    act(() => {
      result.current.setViewMode(DASHBOARD_VIEW_MODES.PIPELINE);
    });
    expect(result.current.viewMode).toBe(DASHBOARD_VIEW_MODES.PIPELINE);
  });

  it('value is read from localStorage on mount', async () => {
    localStorage.setItem(DASHBOARD_VIEW_STORAGE_KEY, DASHBOARD_VIEW_MODES.PIPELINE);
    const { result } = renderHook(() => useDashboardViewMode());
    await waitFor(() => {
      expect(result.current.viewMode).toBe(DASHBOARD_VIEW_MODES.PIPELINE);
    });
  });

  it('persists value to localStorage after toggle', async () => {
    const { result } = renderHook(() => useDashboardViewMode());
    act(() => {
      result.current.setViewMode(DASHBOARD_VIEW_MODES.PIPELINE);
    });
    await waitFor(() => {
      expect(localStorage.getItem(DASHBOARD_VIEW_STORAGE_KEY)).toBe(DASHBOARD_VIEW_MODES.PIPELINE);
    });
  });

  it('invalid localStorage value falls back to default', async () => {
    localStorage.setItem(DASHBOARD_VIEW_STORAGE_KEY, 'invalid_value');
    const { result } = renderHook(() => useDashboardViewMode());
    await waitFor(() => {
      expect(result.current.viewMode).toBe(DASHBOARD_VIEW_MODES.LIST);
    });
  });

  it('supports custom default mode', () => {
    const { result } = renderHook(() => useDashboardViewMode(DASHBOARD_VIEW_MODES.PIPELINE));
    expect(result.current.viewMode).toBe(DASHBOARD_VIEW_MODES.PIPELINE);
  });
});
