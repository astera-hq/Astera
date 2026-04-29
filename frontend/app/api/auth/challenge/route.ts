import { NextResponse } from 'next/server';
import { Keypair, TransactionBuilder, Networks, BASE_FEE, Operation } from '@stellar/stellar-sdk';

export const dynamic = 'force-dynamic';

const NETWORK_PASSPHRASE =
  process.env.NEXT_PUBLIC_STELLAR_NETWORK === 'mainnet' ? Networks.PUBLIC : Networks.TESTNET;

const SERVER_SEED = process.env.SEP10_SERVER_SEED;

function randomNonce(len = 24) {
  const chars = 'abcdefghijklmnopqrstuvwxyz0123456789';
  let out = '';
  for (let i = 0; i < len; i++) out += chars[Math.floor(Math.random() * chars.length)];
  return out;
}

export async function POST(request: Request) {
  if (!SERVER_SEED) {
    return NextResponse.json({ error: 'Server not configured' }, { status: 500 });
  }

  const body = await request.json().catch(() => ({}));
  const account = String(body?.account ?? '').trim();
  if (!account || !account.startsWith('G')) {
    return NextResponse.json({ error: 'invalid_account' }, { status: 400 });
  }

  try {
    const serverKey = Keypair.fromSecret(SERVER_SEED);

    // Build a challenge transaction per SEP-0010: server -> client ManageData
    const serverAccount = { accountId: serverKey.publicKey(), sequence: '0' } as any;

    const tx = new TransactionBuilder(serverAccount, {
      fee: BASE_FEE,
      networkPassphrase: NETWORK_PASSPHRASE,
      timebounds: { minTime: 0, maxTime: Math.floor(Date.now() / 1000) + 300 },
    })
      .addOperation(
        Operation.manageData({
          name: `sep0010_auth_${randomNonce(8)}`,
          value: Buffer.from(randomNonce(16)).toString('base64'),
          source: account,
        }),
      )
      .setTimeout(300)
      .build();

    // Sign with server key so the client can present their signature later
    tx.sign(serverKey);

    const envelope = tx.toEnvelope().toXDR('base64');

    return NextResponse.json({
      transaction: envelope,
      network_passphrase: NETWORK_PASSPHRASE,
      server_account: serverKey.publicKey(),
    });
  } catch (err: unknown) {
    console.error('[SEP-0010] challenge error', err);
    return NextResponse.json({ error: 'server_error' }, { status: 500 });
  }
}
