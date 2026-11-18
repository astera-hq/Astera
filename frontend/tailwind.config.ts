import type { Config } from 'tailwindcss';

const config: Config = {
  darkMode: 'class',
  content: ['./app/**/*.{ts,tsx}', './components/**/*.{ts,tsx}', './lib/**/*.{ts,tsx}'],
  theme: {
    extend: {
      colors: {
        brand: {
          gold: 'var(--gold)',
          amber: 'var(--amber)',
          dark: 'var(--bg)',
          navy: 'var(--bg-navy)',
          card: 'var(--card)',
          border: 'var(--border)',
          muted: 'var(--muted)',
          text: 'var(--text-primary)',
          'text-secondary': 'var(--text-secondary)',
        },
      },
      fontFamily: {
        sans: ['Inter', 'system-ui', 'sans-serif'],
      },
      backgroundImage: {
        'grid-pattern': 'radial-gradient(circle at 1px 1px, #1F2A3C 1px, transparent 0)',
      },
      backgroundSize: {
        grid: '40px 40px',
      },
    },
  },
  plugins: [],
};

export default config;
