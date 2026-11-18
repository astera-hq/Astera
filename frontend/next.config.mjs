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

const allowedConnectSrc = [
  "'self'",
  'https://soroban-testnet.stellar.org',
  'https://horizon-testnet.stellar.org',
  'http://localhost:3000',
  'http://127.0.0.1:3000',
].join(' ');

const securityHeaders = [
  {
    key: 'Content-Security-Policy',
    value: [
      "default-src 'self'",
      "script-src 'self' 'unsafe-eval'",
      "style-src 'self' 'unsafe-inline'",
      `connect-src ${allowedConnectSrc}`,
      "img-src 'self' data: https:",
      "frame-ancestors 'none'",
    ].join('; '),
  },
  { key: 'X-Frame-Options', value: 'DENY' },
  { key: 'X-Content-Type-Options', value: 'nosniff' },
  { key: 'Referrer-Policy', value: 'strict-origin-when-cross-origin' },
  { key: 'Permissions-Policy', value: 'camera=(), microphone=(), geolocation=()' },
];

/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
  transpilePackages: ['next-intl'],
  webpack: (config) => {
    config.resolve.fallback = {
      ...config.resolve.fallback,
      fs: false,
      net: false,
      tls: false,
    };
    return config;
  },
  async headers() {
    return [
      {
        source: '/(.*)',
        headers: securityHeaders,
      },
    ];
  },
};

const withNextIntl = createNextIntlPlugin('./i18n/request.ts');

export default withNextIntl(withPWA(nextConfig));
