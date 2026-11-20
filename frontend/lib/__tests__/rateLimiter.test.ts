import { checkRateLimit, getRateLimitHeaders, getClientIdentifier } from '../rateLimiter';

describe('rateLimiter', () => {
  beforeEach(() => {
    jest.useFakeTimers();
  });

  afterEach(() => {
    jest.useRealTimers();
  });

  describe('checkRateLimit', () => {
    it('allows requests within limit', () => {
      const result1 = checkRateLimit('test-key', { maxRequests: 3, windowMs: 60_000 });
      expect(result1.allowed).toBe(true);
      expect(result1.remaining).toBe(2);

      const result2 = checkRateLimit('test-key', { maxRequests: 3, windowMs: 60_000 });
      expect(result2.allowed).toBe(true);
      expect(result2.remaining).toBe(1);

      const result3 = checkRateLimit('test-key', { maxRequests: 3, windowMs: 60_000 });
      expect(result3.allowed).toBe(true);
      expect(result3.remaining).toBe(0);
    });

    it('blocks requests exceeding limit', () => {
      checkRateLimit('test-key-2', { maxRequests: 2, windowMs: 60_000 });
      checkRateLimit('test-key-2', { maxRequests: 2, windowMs: 60_000 });

      const result = checkRateLimit('test-key-2', { maxRequests: 2, windowMs: 60_000 });
      expect(result.allowed).toBe(false);
      expect(result.remaining).toBe(0);
      expect(result.retryAfter).toBeDefined();
    });

    it('resets after window expires', () => {
      checkRateLimit('test-key-3', { maxRequests: 1, windowMs: 60_000 });
      const blocked = checkRateLimit('test-key-3', { maxRequests: 1, windowMs: 60_000 });
      expect(blocked.allowed).toBe(false);

      jest.advanceTimersByTime(61_000);

      const allowed = checkRateLimit('test-key-3', { maxRequests: 1, windowMs: 60_000 });
      expect(allowed.allowed).toBe(true);
    });

    it('isolates different identifiers', () => {
      const result1 = checkRateLimit('user-a', { maxRequests: 1, windowMs: 60_000 });
      expect(result1.allowed).toBe(true);

      const result2 = checkRateLimit('user-b', { maxRequests: 1, windowMs: 60_000 });
      expect(result2.allowed).toBe(true);
    });
  });

  describe('getRateLimitHeaders', () => {
    it('includes standard rate limit headers', () => {
      const result = checkRateLimit('test-key-4', { maxRequests: 10, windowMs: 60_000 });
      const headers = getRateLimitHeaders(result);

      expect(headers['X-RateLimit-Limit']).toBe('10');
      expect(headers['X-RateLimit-Remaining']).toBe('9');
      expect(headers['X-RateLimit-Reset']).toBeDefined();
    });

    it('includes Retry-After when blocked', () => {
      checkRateLimit('test-key-5', { maxRequests: 1, windowMs: 60_000 });
      const blocked = checkRateLimit('test-key-5', { maxRequests: 1, windowMs: 60_000 });
      const headers = getRateLimitHeaders(blocked);

      expect(headers['Retry-After']).toBeDefined();
      expect(Number(headers['Retry-After'])).toBeGreaterThan(0);
    });
  });

  describe('getClientIdentifier', () => {
    it('extracts IP from x-forwarded-for header', () => {
      const request = new Request('http://localhost', {
        headers: { 'x-forwarded-for': '192.168.1.1, 10.0.0.1' },
      });

      const identifier = getClientIdentifier(request);
      expect(identifier).toBe('ip:192.168.1.1');
    });

    it('returns unknown when no forwarded header', () => {
      const request = new Request('http://localhost');

      const identifier = getClientIdentifier(request);
      expect(identifier).toBe('ip:unknown');
    });
  });
});
