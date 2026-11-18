import { useEffect, useState } from 'react';
import {
  DASHBOARD_VIEW_MODES,
  DASHBOARD_VIEW_STORAGE_KEY,
  type DashboardViewMode,
} from '@/lib/dashboardPipeline';

export function useDashboardViewMode(defaultMode: DashboardViewMode = DASHBOARD_VIEW_MODES.LIST) {
  const [viewMode, setViewMode] = useState<DashboardViewMode>(defaultMode);
  const [hydrated, setHydrated] = useState(false);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      setHydrated(true);
      const stored = window.localStorage.getItem(DASHBOARD_VIEW_STORAGE_KEY);
      if (stored === DASHBOARD_VIEW_MODES.LIST || stored === DASHBOARD_VIEW_MODES.PIPELINE) {
        setViewMode(stored);
      }
    }, 0);

    return () => window.clearTimeout(timer);
  }, []);

  useEffect(() => {
    if (!hydrated) return;
    window.localStorage.setItem(DASHBOARD_VIEW_STORAGE_KEY, viewMode);
  }, [hydrated, viewMode]);

  return { viewMode, setViewMode, hydrated };
}
