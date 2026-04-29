import nextPwa from 'next-pwa';
import runtimeCaching from 'next-pwa/cache.js';
import createNextIntlPlugin from 'next-intl/plugin';

const withPWA = nextPwa({
  dest: 'public',
  disable: process.env.NODE_ENV === 'development',
  runtimeCaching,
  buildExcludes: [/middleware-manifest\.json$/],
  fallbacks: {
    document: '/offline.html',
  },
});

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
