import type { Metadata } from 'next';
import { glossary } from '@/lib/glossary';

export const metadata: Metadata = {
  title: 'Glossary — Astera',
  description: 'Plain-language explanations of the Stellar and Soroban terms used across Astera.',
  // #783: canonical tag, resolved against metadataBase in the root layout.
  alternates: { canonical: '/glossary' },
};

export default function GlossaryPage() {
  return (
    <div className="min-h-screen pt-24 pb-16 px-6">
      <div className="max-w-2xl mx-auto">
        <div className="mb-8">
          <h1 className="text-3xl font-bold mb-1">Glossary</h1>
          <p className="text-brand-muted">
            Astera runs on Stellar and Soroban, which come with their own vocabulary. Here&apos;s
            what the terms you&apos;ll see around the app actually mean.
          </p>
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
