const createPWA = require('@opensourceframework/next-pwa');
const createNextIntlPlugin = require('next-intl/plugin');

/** @type {import('next').NextConfig} */
const withPWA = createPWA({
  dest: 'public',
  disable: process.env.NODE_ENV === 'development',
  buildExcludes: [/middleware-manifest\.json$/],
  fallbacks: {
    document: '/offline.html',
  },
});

const withNextIntl = createNextIntlPlugin('./i18n/request.ts');

module.exports = withNextIntl(
  withPWA({
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
  }),
);
