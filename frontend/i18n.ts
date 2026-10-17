import { getRequestConfig } from 'next-intl/server';
import { cookies } from 'next/headers';

const SUPPORTED_LOCALES = ['en', 'fr', 'es'] as const;

export default getRequestConfig(async () => {
  // Read locale from cookie or use default
  const cookieStore = await cookies();
  const rawLocale = cookieStore.get('NEXT_LOCALE')?.value;
  const locale =
    rawLocale && (SUPPORTED_LOCALES as readonly string[]).includes(rawLocale) ? rawLocale : 'en';

  return {
    locale,
    messages: (await import(`./locales/${locale}/common.json`)).default,
  };
});
