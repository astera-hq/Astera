import { getRequestConfig } from 'next-intl/server';

export const SUPPORTED_LOCALES = ['en', 'fr', 'es'] as const;
export type SupportedLocale = (typeof SUPPORTED_LOCALES)[number];

export function normalizeLocale(locale: string | undefined): SupportedLocale {
  if (locale && (SUPPORTED_LOCALES as readonly string[]).includes(locale)) {
    return locale as SupportedLocale;
  }
  return 'en';
}

export default getRequestConfig(async ({ requestLocale }) => {
  const requested = await requestLocale;
  const locale = normalizeLocale(requested);

  return {
    locale,
    messages: (await import(`../locales/${locale}/common.json`)).default,
  };
});
