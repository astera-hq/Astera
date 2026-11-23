import { NextResponse } from 'next/server';
import { monitorService } from '../../../lib/monitoring';
import { checkRateLimit, getClientIdentifier, getRateLimitHeaders } from '@/lib/rateLimiter';

export const dynamic = 'force-dynamic';

const CACHE_TTL_MS = 5 * 60 * 1000;
let cachedResponse: { data: unknown; timestamp: number } | null = null;

function verifyCronSecret(request: Request): boolean {
  const authHeader = request.headers.get('Authorization');
  const cronSecret = process.env.CRON_SECRET;
  
  if (!cronSecret) {
    console.warn('[monitor] CRON_SECRET not configured — polling endpoint is public');
    return false;
  }
  
  return authHeader === `Bearer ${cronSecret}`;
}

export async function GET(request: Request) {
  const isAuthorized = verifyCronSecret(request);
  
  if (!isAuthorized) {
    return NextResponse.json(
      { status: 'error', message: 'Unauthorized' },
      { status: 401 },
    );
  }

  const identifier = getClientIdentifier(request);
  const rateLimitResult = checkRateLimit(identifier, { maxRequests: 10, windowMs: 60_000 });

  if (!rateLimitResult.allowed) {
    return NextResponse.json(
      { status: 'error', message: 'Too many requests' },
      { status: 429, headers: getRateLimitHeaders(rateLimitResult) },
    );
  }

  const now = Date.now();
  if (cachedResponse && now - cachedResponse.timestamp < CACHE_TTL_MS) {
    return NextResponse.json(cachedResponse.data, {
      headers: {
        ...getRateLimitHeaders(rateLimitResult),
        'X-Cache': 'HIT',
      },
    });
  }

  try {
    const events = await monitorService.pollEvents();

    const responseData = {
      status: 'success',
      events: events.length,
      timestamp: new Date().toISOString(),
    };

    cachedResponse = { data: responseData, timestamp: now };

    return NextResponse.json(responseData, {
      headers: {
        ...getRateLimitHeaders(rateLimitResult),
        'X-Cache': 'MISS',
      },
    });
  } catch (error) {
    console.error('Monitor API Error:', error);
    return NextResponse.json(
      { status: 'error', message: 'Failed to poll events' },
      { status: 500 },
    );
  }
}
