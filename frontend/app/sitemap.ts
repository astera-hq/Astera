import type { MetadataRoute } from 'next';
import { getAppUrl } from '@/lib/env';

// #783: static, public-facing routes only — authenticated/app routes
// (dashboard, invest, portfolio, admin, settings, etc.) aren't meant to be
// indexed and are already excluded via robots.txt.
export default function sitemap(): MetadataRoute.Sitemap {
  const baseUrl = getAppUrl();

  return [
    {
      url: baseUrl,
      lastModified: new Date(),
      changeFrequency: 'weekly',
      priority: 1,
    },
    {
      url: `${baseUrl}/analytics`,
      lastModified: new Date(),
      changeFrequency: 'daily',
      priority: 0.8,
    },
    {
      url: `${baseUrl}/glossary`,
      lastModified: new Date(),
      changeFrequency: 'monthly',
      priority: 0.6,
    },
  ];
}
