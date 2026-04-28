'use client';

import { createContext, useContext, useEffect, useState } from 'react';

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

export default function ThemeProvider({ children }: { children: React.ReactNode }) {
  // Initialise synchronously on client; default to 'dark' for SSR
  const [theme, setTheme] = useState<Theme>(() => {
    if (typeof window === 'undefined') return 'dark';
    const stored = localStorage.getItem('astera-theme') as Theme | null;
    return (
      stored ?? (window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark')
    );
  });

  // Keep the DOM in sync whenever theme changes
  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  function toggleTheme() {
    setTheme((prev) => {
      const next: Theme = prev === 'dark' ? 'light' : 'dark';
      applyTheme(next);
      localStorage.setItem('astera-theme', next);
      return next;
    });
  }

  return <ThemeContext.Provider value={{ theme, toggleTheme }}>{children}</ThemeContext.Provider>;
}
