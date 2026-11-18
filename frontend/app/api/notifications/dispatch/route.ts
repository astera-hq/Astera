import { NextResponse } from 'next/server';
import {
  deliverNotification,
  type DeliveryEvent,
  type DeliveryPreferences,
  type NotificationPayload,
} from '@/lib/server/notification-delivery';

export const dynamic = 'force-dynamic';

export async function POST(request: Request) {
  if (request.headers.get('authorization') !== `Bearer ${process.env.NOTIFICATION_DISPATCH_TOKEN}`)
    return NextResponse.json({ error: 'Unauthorized' }, { status: 401 });
  try {
    const body = (await request.json()) as {
      payload: NotificationPayload;
      preferences: DeliveryPreferences;
    };
    if (!body?.payload?.event || !body.payload.recipient)
      return NextResponse.json(
        { error: 'payload.event and payload.recipient are required' },
        { status: 400 },
      );
    const supported: DeliveryEvent[] = [
      'INVOICE_FUNDED',
      'PAYMENT_DUE_7_DAYS',
      'PAYMENT_DUE_1_DAY',
      'PAYMENT_OVERDUE',
      'INVOICE_COMPLETED',
      'INVOICE_DEFAULTED',
      'COLLATERAL_SEIZED',
    ];
    if (!supported.includes(body.payload.event))
      return NextResponse.json({ error: 'Unsupported notification event' }, { status: 400 });
    return NextResponse.json({
      ok: true,
      delivered: await deliverNotification(body.payload, body.preferences),
    });
  } catch (error) {
    return NextResponse.json(
      { ok: false, error: error instanceof Error ? error.message : 'Notification delivery failed' },
      { status: 502 },
    );
  }
}
