'use client';

import { useTranslations } from 'next-intl';
import { useAdminGuard } from '@/hooks/useAdminGuard';
import AdminNav from '@/components/AdminNav';

export default function AdminLayout({ children }: { children: React.ReactNode }) {
  const t = useTranslations('Admin.layout');
  const { status } = useAdminGuard();

  if (status === 'loading') {
    return (
      <div className="min-h-screen flex items-center justify-center bg-brand-dark pt-16">
        <div className="w-8 h-8 border-2 border-brand-gold border-t-transparent rounded-full animate-spin" />
      </div>
    );
  }

  if (status === 'unauthenticated') {
    return (
      <div className="min-h-screen flex flex-col items-center justify-center bg-brand-dark pt-16 text-center px-6">
        <div className="text-4xl mb-6">◈</div>
        <h1 className="text-2xl font-bold mb-2">{t('accessDeniedTitle')}</h1>
        <p className="text-brand-muted max-w-sm mb-8">{t('accessDeniedDesc')}</p>
        <div className="bg-brand-card border border-brand-border p-6 rounded-2xl max-w-md w-full">
          <p className="text-sm text-brand-muted mb-4">{t('accessDeniedNote')}</p>
        </div>
      </div>
    );
  }

  // A denied wallet is being redirected to /dashboard; render nothing so admin
  // controls never flash for unauthorized users.
  if (status !== 'authorized') {
    return (
      <div className="min-h-screen flex items-center justify-center bg-brand-dark pt-16">
        <div className="w-8 h-8 border-2 border-brand-gold border-t-transparent rounded-full animate-spin" />
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-brand-dark">
      <AdminNav />
      <main className="md:pl-16 lg:pl-64 pt-16 pb-16">
        <div className="max-w-6xl mx-auto px-4 sm:px-6 pt-12">{children}</div>
      </main>
    </div>
  );
}
