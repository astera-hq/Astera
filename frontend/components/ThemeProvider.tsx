'use client';

import { createContext, useCallback, useContext, useEffect, useRef, useState } from 'react';

type Theme = 'dark' | 'light';

interface ThemeContextValue {
  theme: Theme;
  toggleTheme: () => void;
}

const ThemeContext = createContext<ThemeContextValue>({
  theme: 'dark',
  toggleTheme: () => {},
});

export function useTheme(): ThemeContextValue {
  return useContext(ThemeContext);
}

function applyTheme(next: Theme) {
  const root = document.documentElement;
  if (next === 'light') {
    root.classList.add('light');
  } else {
    root.classList.remove('light');
  }
}

const STORAGE_KEY = 'astera-theme';

export default function ThemeProvider({ children }: { children: React.ReactNode }) {
  const [theme, setTheme] = useState<Theme>(() => {
    if (typeof window === 'undefined') return 'dark';
    const stored = localStorage.getItem(STORAGE_KEY) as Theme | null;
    return (
      stored ?? (window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark')
    );
  });

  // Track whether the user has explicitly chosen a theme
  const userExplicit = useRef(false);

  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  // Listen for OS-level preference changes when user hasn't explicitly set one
  useEffect(() => {
    const mql = window.matchMedia('(prefers-color-scheme: light)');
    function handleChange(e: MediaQueryListEvent) {
      if (userExplicit.current) return;
      const next: Theme = e.matches ? 'light' : 'dark';
      setTheme(next);
      applyTheme(next);
    }
    mql.addEventListener('change', handleChange);
    return () => mql.removeEventListener('change', handleChange);
  }, []);

  const toggleTheme = useCallback(() => {
    userExplicit.current = true;
    setTheme((prev) => {
      const next: Theme = prev === 'dark' ? 'light' : 'dark';
      applyTheme(next);
      localStorage.setItem(STORAGE_KEY, next);
      return next;
    });
  }, []);

  return <ThemeContext.Provider value={{ theme, toggleTheme }}>{children}</ThemeContext.Provider>;
}
