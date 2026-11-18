import nextConfig from 'eslint-config-next';
import tseslint from 'typescript-eslint';
import prettierConfig from 'eslint-config-prettier';

export default tseslint.config(
  ...nextConfig,
  ...tseslint.configs.recommended,
  prettierConfig,
  {
    rules: {
      '@typescript-eslint/no-unused-vars': ['warn', { argsIgnorePattern: '^_' }],
      '@typescript-eslint/no-explicit-any': 'warn',
    },
  },
  {
    ignores: [
      'node_modules/**',
      '.next/**',
      // Auto-generated Soroban contract bindings (see scripts/gen-bindings.sh).
      'src/generated/**',
      'public/sw.js',
      'public/sw.js.map',
      'public/workbox-*.js',
      'public/workbox-*.js.map',
    ],
  },
);
