export type FreighterApi = typeof import('@stellar/freighter-api');

declare global {
  // Provided by Playwright E2E via `page.addInitScript`.

  var __MOCK_FREIGHTER_API__: (Partial<FreighterApi> & Record<string, unknown>) | undefined;
}

export async function getFreighter(): Promise<FreighterApi> {
  if (typeof window !== 'undefined') {
    const maybeMock = (window as unknown as { __MOCK_FREIGHTER_API__?: FreighterApi })
      .__MOCK_FREIGHTER_API__;
    if (maybeMock) return maybeMock;
  }

  return await import('@stellar/freighter-api');
}
