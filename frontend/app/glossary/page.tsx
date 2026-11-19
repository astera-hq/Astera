import type { Metadata } from 'next';
import { getTranslations } from 'next-intl/server';
import { glossary } from '@/lib/glossary';

export async function generateMetadata(): Promise<Metadata> {
  const t = await getTranslations('Glossary');
  return {
    title: t('metadata.title'),
    description: t('metadata.description'),
    // #783: canonical tag, resolved against metadataBase in the root layout.
    alternates: { canonical: '/glossary' },
  };
}

export default async function GlossaryPage() {
  const t = await getTranslations('Glossary');

  return (
    <div className="min-h-screen pt-24 pb-16 px-6">
      <div className="max-w-2xl mx-auto">
        <div className="mb-8">
          <h1 className="text-3xl font-bold mb-1">{t('title')}</h1>
          <p className="text-brand-muted">{t('intro')}</p>
        </div>

        <dl className="space-y-4">
          {glossary.map((entry) => (
            <div
              key={entry.id}
              id={entry.id}
              className="p-5 bg-brand-card border border-brand-border rounded-2xl scroll-mt-24"
            >
              <dt className="font-semibold text-brand-gold mb-1">{entry.term}</dt>
              <dd className="text-sm text-brand-muted leading-relaxed">{entry.definition}</dd>
            </div>
          ))}
        </dl>
      </div>
    </div>
  );
}
