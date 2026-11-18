import { isSafeWebhookUrl } from '@/lib/server/webhook-url';

const lookup = jest.fn<
  Promise<readonly { address: string; family: number }[]>,
  [string, { all: true }]
>();

describe('isSafeWebhookUrl', () => {
  beforeEach(() => lookup.mockReset());

  it('allows HTTPS hosts only when every resolved address is public', async () => {
    lookup.mockResolvedValue([{ address: '93.184.216.34', family: 4 }]);
    await expect(isSafeWebhookUrl('https://example.com/hook', lookup)).resolves.toBe(true);
  });

  it.each([
    'http://example.com/hook',
    'https://127.0.0.1/hook',
    'https://169.254.169.254/latest/meta-data',
    'https://metadata.google.internal/',
  ])('blocks unsafe destination %s', async (url) => {
    lookup.mockResolvedValue([{ address: '127.0.0.1', family: 4 }]);
    await expect(isSafeWebhookUrl(url, lookup)).resolves.toBe(false);
  });

  it('blocks a hostname if any DNS result is private', async () => {
    lookup.mockResolvedValue([
      { address: '93.184.216.34', family: 4 },
      { address: '10.0.0.1', family: 4 },
    ]);
    await expect(isSafeWebhookUrl('https://example.com/hook', lookup)).resolves.toBe(false);
  });

  it('fails closed on DNS errors', async () => {
    lookup.mockRejectedValue(new Error('DNS failed'));
    await expect(isSafeWebhookUrl('https://example.com/hook', lookup)).resolves.toBe(false);
  });
});
