// GitHub Actions can end up with optional native deps omitted (notably
// `@parcel/watcher` via transitive tooling). Importing PWA tooling can trigger
// those natives during config load, failing the entire build. In CI we keep
// the config minimal and skip PWA wrapping.
import createNextIntlPlugin from 'next-intl/plugin';

let withPWA = (config) => config;
const IS_CI =
  process.env.CI === 'true' || process.env.CI === '1' || process.env.GITHUB_ACTIONS === 'true';

if (!IS_CI) {
  try {
    const [{ default: nextPwa }, { default: runtimeCaching }] = await Promise.all([
      import('next-pwa'),
      import('next-pwa/cache.js'),
    ]);

    withPWA = nextPwa({
      dest: 'public',
      disable: process.env.NODE_ENV === 'development',
      runtimeCaching,
      buildExcludes: [/middleware-manifest\.json$/],
      fallbacks: {
        document: '/offline.html',
      },
    });
  } catch {
    // If next-pwa can't be loaded for any reason, build without PWA.
    withPWA = (config) => config;
  }
}

/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
  webpack: (config) => {
    config.resolve.fallback = {
      ...config.resolve.fallback,
      fs: false,
      net: false,
      tls: false,
    };
    return config;
  },
};

const withNextIntl = createNextIntlPlugin('./i18n/request.ts');

export default withNextIntl(withPWA(nextConfig));
