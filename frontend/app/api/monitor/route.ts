import { NextResponse } from 'next/server';
import { monitorService } from '../../../lib/monitoring';
import { checkRateLimit, getClientIdentifier, getRateLimitHeaders } from '@/lib/rateLimiter';

export const dynamic = 'force-dynamic';

export async function GET(request: Request) {
  const identifier = getClientIdentifier(request);
  const rateLimitResult = checkRateLimit(identifier, { maxRequests: 30, windowMs: 60_000 });

  if (!rateLimitResult.allowed) {
    return NextResponse.json(
      { status: 'error', message: 'Too many requests' },
      { status: 429, headers: getRateLimitHeaders(rateLimitResult) },
    );
  }

  try {
    const events = await monitorService.pollEvents();

    return NextResponse.json(
      {
        status: 'success',
        events: events.length,
        timestamp: new Date().toISOString(),
      },
      { headers: getRateLimitHeaders(rateLimitResult) },
    );
  } catch (error) {
    console.error('Monitor API Error:', error);
    return NextResponse.json(
      { status: 'error', message: 'Failed to poll events' },
      { status: 500 },
    );
  }
}
