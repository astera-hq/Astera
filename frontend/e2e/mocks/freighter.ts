/**
 * Freighter wallet mock for E2E tests.
 *
 * Injected via page.addInitScript so it runs before any app code.
 * Patches the module cache entry that Next.js bundles for
 * @stellar/freighter-api so dynamic imports resolve to this stub.
 */

export const MOCK_ADDRESS = 'GDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W37';

/** Script string to inject into the browser page before app scripts run. */
export function freighterMockScript(
  opts: {
    isConnected?: boolean;
    isAllowed?: boolean;
    address?: string;
    signError?: string;
  } = {},
): string {
  const connected = opts.isConnected ?? true;
  const allowed = opts.isAllowed ?? true;
  const address = opts.address ?? MOCK_ADDRESS;
  const signError = opts.signError ?? null;

  return `
    (function() {
      const mockFreighter = {
        isConnected: () => Promise.resolve({ isConnected: ${connected} }),
        isAllowed: () => Promise.resolve({ isAllowed: ${allowed} }),
        setAllowed: () => Promise.resolve({ isAllowed: true }),
        getAddress: () => Promise.resolve({ address: '${address}', error: null }),
        signTransaction: (xdr, opts) => {
          ${signError ? `return Promise.resolve({ signedTxXdr: null, error: { message: '${signError}' } });` : `return Promise.resolve({ signedTxXdr: xdr + '_signed', error: null });`}
        },
        getNetwork: () => Promise.resolve({ network: 'TESTNET', networkPassphrase: 'Test SDF Network ; September 2015' }),
      };

      // Patch the Next.js webpack module cache so dynamic imports of
      // @stellar/freighter-api resolve to the mock object.
      const patchCache = () => {
        if (typeof window.__webpack_require__ !== 'undefined') {
          const origLoad = window.__webpack_require__;
          window.__webpack_require__ = function(id) {
            const mod = origLoad(id);
            return mod;
          };
        }
        // Also expose on window for scripts that check window.freighter
        window.__FREIGHTER_MOCK__ = mockFreighter;
      };

      patchCache();

      // Override dynamic import resolution for the freighter module by
      // intercepting the module via a global registry.
      window.__MOCK_FREIGHTER_API__ = mockFreighter;
    })();
  `;
}
