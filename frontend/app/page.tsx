import type { Metadata } from 'next';
import Link from 'next/link';
import { getTranslations } from 'next-intl/server';
import GlossaryTerm from '@/components/GlossaryTerm';

// #783: canonical tag for the landing page — resolved against
// metadataBase (set in the root layout) into an absolute URL.
export async function generateMetadata(): Promise<Metadata> {
  const t = await getTranslations('Landing');
  return {
    title: t('metadata.title'),
    description: t('metadata.description'),
    alternates: { canonical: '/' },
  };
}

export default async function Home() {
  const t = await getTranslations('Landing');

  // Numeric values and glyph icons are intentionally kept as-is; only the
  // user-facing copy is translated.
  const stats = [
    { label: t('stats.gapLabel'), value: '$1.7T', sub: t('stats.gapSub') },
    { label: t('stats.settlementLabel'), value: '3-5s', sub: t('stats.settlementSub') },
    { label: t('stats.apyLabel'), value: '8%', sub: t('stats.apySub') },
    { label: t('stats.feeLabel'), value: '<$0.01', sub: t('stats.feeSub') },
  ];

  const features = [
    {
      icon: '◈',
      title: t('features.tokenizeTitle'),
      description: t('features.tokenizeDesc'),
    },
    {
      icon: '◉',
      title: t('features.liquidityTitle'),
      description: t('features.liquidityDesc'),
    },
    {
      icon: '◎',
      title: t('features.escrowTitle'),
      description: t('features.escrowDesc'),
    },
    {
      icon: '◐',
      title: t('features.creditTitle'),
      description: t('features.creditDesc'),
    },
    {
      icon: '◑',
      title: t('features.usdcTitle'),
      description: t('features.usdcDesc'),
    },
    {
      icon: '◒',
      title: t('features.diasporaTitle'),
      description: t('features.diasporaDesc'),
    },
  ];

  const steps = [
    {
      n: '01',
      role: t('steps.smeRole'),
      title: t('steps.smeTitle'),
      desc: t('steps.smeDesc'),
    },
    {
      n: '02',
      role: t('steps.poolRole'),
      title: t('steps.poolTitle'),
      desc: t('steps.poolDesc'),
    },
    {
      n: '03',
      role: t('steps.investorRole'),
      title: t('steps.investorTitle'),
      desc: t('steps.investorDesc'),
    },
  ];

  return (
    <div className="overflow-x-hidden">
      {/* Hero */}
      <section className="relative min-h-screen flex flex-col items-center justify-center px-6 pt-24 pb-20 text-center">
        {/* Grid background */}
        <div
          className="absolute inset-0 opacity-20"
          style={{
            backgroundImage: 'radial-gradient(circle at 1px 1px, #1F2A3C 1px, transparent 0)',
            backgroundSize: '40px 40px',
          }}
        />
        {/* Radial glow */}
        <div className="absolute top-1/3 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[600px] h-[600px] bg-brand-gold/5 rounded-full blur-3xl pointer-events-none" />

        <div className="relative z-10 max-w-4xl mx-auto">
          <div className="inline-flex items-center gap-2 px-3 py-1 rounded-full border border-brand-gold/30 bg-brand-gold/10 text-brand-gold text-sm font-medium mb-8">
            <span className="w-1.5 h-1.5 rounded-full bg-brand-gold animate-pulse" />
            {t('hero.badge')}
          </div>

          <h1 className="text-5xl md:text-7xl font-bold leading-tight mb-6">
            {t('hero.heading1')} <span className="gradient-text">{t('hero.heading2')}</span>
            <br />
            {t('hero.heading3')}
          </h1>

          <p className="text-xl text-brand-muted max-w-2xl mx-auto mb-12 leading-relaxed">
            {t('hero.description')}
          </p>

          <div className="flex flex-col sm:flex-row gap-4 justify-center">
            <Link
              href="/dashboard"
              className="px-8 py-4 bg-brand-gold text-brand-dark font-semibold rounded-xl hover:bg-brand-amber transition-colors gold-glow text-lg"
            >
              {t('hero.ctaTokenize')}
            </Link>
            <Link
              href="/invest"
              className="px-8 py-4 bg-brand-card border border-brand-border text-white font-semibold rounded-xl hover:border-brand-gold/50 transition-colors text-lg"
            >
              {t('hero.ctaInvest')}
            </Link>
          </div>
        </div>
      </section>

      {/* Stats */}
      <section className="py-16 border-y border-brand-border bg-brand-navy/50">
        <div className="max-w-6xl mx-auto px-6 grid grid-cols-2 md:grid-cols-4 gap-8">
          {stats.map((s) => (
            <div key={s.label} className="text-center">
              <div className="text-3xl md:text-4xl font-bold gradient-text mb-1">{s.value}</div>
              <div className="text-sm text-brand-muted">{s.label}</div>
              <div className="text-xs text-brand-muted/60 mt-0.5">{s.sub}</div>
            </div>
          ))}
        </div>
      </section>

      {/* Features */}
      <section className="py-24 px-6">
        <div className="max-w-6xl mx-auto">
          <div className="text-center mb-16">
            <h2 className="text-3xl md:text-4xl font-bold mb-4">
              {t('features.headingPrefix')}{' '}
              <span className="gradient-text">{t('features.headingAccent')}</span>
            </h2>
            <p className="text-brand-muted max-w-xl mx-auto">{t('features.intro')}</p>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
            {features.map((f) => (
              <div
                key={f.title}
                className="p-6 bg-brand-card border border-brand-border rounded-2xl hover:border-brand-gold/30 transition-colors group"
              >
                <div className="text-2xl text-brand-gold mb-4 group-hover:scale-110 transition-transform inline-block">
                  {f.icon}
                </div>
                <h3 className="font-semibold text-lg mb-2">{f.title}</h3>
                <p className="text-brand-muted text-sm leading-relaxed">{f.description}</p>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* How it works */}
      <section className="py-24 px-6 bg-brand-navy/30">
        <div className="max-w-4xl mx-auto">
          <div className="text-center mb-16">
            <h2 className="text-3xl md:text-4xl font-bold mb-4">{t('steps.heading')}</h2>
          </div>

          <div className="space-y-6">
            {steps.map((s) => (
              <div
                key={s.n}
                className="flex gap-6 p-6 bg-brand-card border border-brand-border rounded-2xl"
              >
                <div className="flex-shrink-0 w-12 h-12 rounded-full bg-brand-gold/10 border border-brand-gold/30 flex items-center justify-center text-brand-gold font-bold text-sm">
                  {s.n}
                </div>
                <div>
                  <div className="text-xs text-brand-gold font-medium uppercase tracking-wider mb-1">
                    {s.role}
                  </div>
                  <h3 className="font-semibold text-lg mb-1">{s.title}</h3>
                  <p className="text-brand-muted text-sm">{s.desc}</p>
                </div>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* CTA */}
      <section className="py-24 px-6 text-center">
        <div className="max-w-2xl mx-auto">
          <h2 className="text-3xl md:text-4xl font-bold mb-6">{t('cta.title')}</h2>
          <p className="text-brand-muted mb-10">{t('cta.description')}</p>
          <Link
            href="/dashboard"
            className="inline-block px-10 py-4 bg-brand-gold text-brand-dark font-semibold rounded-xl hover:bg-brand-amber transition-colors gold-glow text-lg"
          >
            {t('cta.button')}
          </Link>
        </div>
      </section>

      {/* Footer */}
      <footer className="border-t border-brand-border py-8 px-6 text-center text-brand-muted text-sm">
        <div className="max-w-6xl mx-auto flex flex-col md:flex-row items-center justify-between gap-4">
          <div className="font-semibold text-white">Astera</div>
          <div>
            {t('footer.builtOnPrefix')} <GlossaryTerm id="soroban" /> {t('footer.builtOnSuffix')}
          </div>
          <div className="flex gap-6">
            <Link href="/dashboard" className="hover:text-white transition-colors">
              {t('footer.dashboard')}
            </Link>
            <Link href="/invest" className="hover:text-white transition-colors">
              {t('footer.invest')}
            </Link>
            <Link href="/glossary" className="hover:text-white transition-colors">
              {t('footer.glossary')}
            </Link>
          </div>
        </div>
      </footer>
    </div>
  );
}
