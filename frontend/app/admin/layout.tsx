'use client';

import { useEffect, useState } from 'react';
import { useStore } from '@/lib/store';
import { getPoolConfig } from '@/lib/contracts';
import AdminNav from '@/components/AdminNav';

export default function AdminLayout({ children }: { children: React.ReactNode }) {
  const { wallet, poolConfig, setPoolConfig } = useStore();
  const [loading, setLoading] = useState(true);
  const [authorized, setAuthorized] = useState<boolean | null>(null);

  useEffect(() => {
    async function checkAuth() {
      setLoading(true);
      try {
        let currentPoolConfig = poolConfig;
        if (!currentPoolConfig) {
          currentPoolConfig = await getPoolConfig();
          setPoolConfig(currentPoolConfig);
        }

        if (wallet.connected && wallet.address && currentPoolConfig) {
          try {
            // Prefer cryptographic verification via SEP-0010 JWT
            const { verifyToken, getToken } = await import('@/lib/auth');
            const token = getToken();
            const v = await verifyToken(token);
            if (v?.authenticated && v?.account === currentPoolConfig.admin) {
              setAuthorized(true);
            } else if (wallet.address === currentPoolConfig.admin) {
              // fallback to raw address match (non-cryptographic)
              setAuthorized(true);
            } else {
              setAuthorized(false);
            }
          } catch (e) {
            console.warn('Auth verification failed, falling back to address match', e);
            setAuthorized(wallet.address === currentPoolConfig.admin);
          }
        } else if (!wallet.connected) {
          setAuthorized(null);
        }
      } catch (e) {
        console.error('Failed to get pool admin:', e);
        setAuthorized(false);
      } finally {
        setLoading(false);
      }
    }

    checkAuth();
  }, [wallet.connected, wallet.address, poolConfig, setPoolConfig]);

  if (loading) {
    return (
      <div className="min-h-screen flex items-center justify-center bg-brand-dark pt-16">
        <div className="w-8 h-8 border-2 border-brand-gold border-t-transparent rounded-full animate-spin" />
      </div>
    );
  }

  if (authorized === null) {
    return (
      <div className="min-h-screen flex flex-col items-center justify-center bg-brand-dark pt-16 text-center px-6">
        <div className="text-4xl mb-6">◈</div>
        <h1 className="text-2xl font-bold mb-2">Admin Identification Required</h1>
        <p className="text-brand-muted max-w-sm mb-8">
          Please connect your wallet to verify administrative access to the Astera pool.
        </p>
        <div className="bg-brand-card border border-brand-border p-6 rounded-2xl max-w-md w-full">
          <p className="text-sm text-brand-muted mb-4">
            Access to /admin is restricted to the pool administrator address.
          </p>
        </div>
      </div>
    );
  }

  if (!authorized) {
    return (
      <div className="min-h-screen flex flex-col items-center justify-center bg-brand-dark pt-16 text-center px-6">
        <div className="w-16 h-16 bg-red-900/20 border border-red-800/50 rounded-full flex items-center justify-center text-red-500 mb-6 font-bold text-2xl">
          !
        </div>
        <h1 className="text-2xl font-bold mb-2 text-white">Access Denied</h1>
        <p className="text-brand-muted max-w-md">
          Your connected wallet ({wallet.address}) is not authorized to access the administration
          panel.
        </p>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-brand-dark">
      <AdminNav />
      <main className="lg:pl-64 pt-16 pb-16">
        <div className="max-w-6xl mx-auto px-4 sm:px-6 pt-12">{children}</div>
      </main>
    </div>
  );
}
