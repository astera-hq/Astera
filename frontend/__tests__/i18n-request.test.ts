import { normalizeLocale, SUPPORTED_LOCALES } from '@/i18n/request';

describe('i18n request normalization (#1529)', () => {
  it('includes en, fr, and es in SUPPORTED_LOCALES', () => {
    expect(SUPPORTED_LOCALES).toContain('en');
    expect(SUPPORTED_LOCALES).toContain('fr');
    expect(SUPPORTED_LOCALES).toContain('es');
  });

  it('normalizes valid locales cleanly', () => {
    expect(normalizeLocale('en')).toBe('en');
    expect(normalizeLocale('fr')).toBe('fr');
    expect(normalizeLocale('es')).toBe('es');
  });

  it('falls back to en for undefined or unsupported locales', () => {
    expect(normalizeLocale(undefined)).toBe('en');
    expect(normalizeLocale('')).toBe('en');
    expect(normalizeLocale('de')).toBe('en');
    expect(normalizeLocale('es-ES')).toBe('en');
    expect(normalizeLocale('invalid')).toBe('en');
  });
});
