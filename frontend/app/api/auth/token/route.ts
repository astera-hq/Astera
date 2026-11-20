import { NextResponse } from 'next/server';
import { TransactionBuilder, Networks, Keypair, StrKey } from '@stellar/stellar-sdk';
import { SignJWT } from 'jose';
import { getSep10Secret, SEP10_AUDIENCE, SEP10_ISSUER, SEP10_TTL_SECONDS, sep10Key } from '@/lib/sep10-jwt';
import { checkRateLimit, getClientIdentifier, getRateLimitHeaders } from '@/lib/rateLimiter';

export const dynamic = 'force-dynamic';

const NETWORK_PASSPHRASE =
  process.env.NEXT_PUBLIC_STELLAR_NETWORK === 'mainnet' ? Networks.PUBLIC : Networks.TESTNET;

const JWT_SECRET = getSep10Secret();
const SERVER_SEED = process.env.SEP10_SERVER_SEED;

function badRequest(msg = 'invalid') {
  return NextResponse.json({ error: msg }, { status: 400 });
}

export async function POST(request: Request) {
  const identifier = getClientIdentifier(request);
  const rateLimitResult = checkRateLimit(identifier, { maxRequests: 10, windowMs: 60_000 });

  if (!rateLimitResult.allowed) {
    return NextResponse.json(
      { error: 'Too many requests' },
      { status: 429, headers: getRateLimitHeaders(rateLimitResult) },
    );
  }

  if (!JWT_SECRET || !SERVER_SEED)
    return NextResponse.json({ error: 'server_not_configured' }, { status: 500 });

  const body = await request.json().catch(() => ({}));
  const signedXdr = String(body?.signed_xdr ?? '').trim();
  if (!signedXdr) return badRequest('missing_signed_xdr');

  try {
    const tx = TransactionBuilder.fromXDR(signedXdr, NETWORK_PASSPHRASE);
    const txHash = tx.hash();

    const ops = tx.operations || [];
    let clientAccount: string | null = null;
    for (const op of ops) {
      if (op.type === 'manageData' && op.source) {
        clientAccount = op.source;
        break;
      }
    }
    if (!clientAccount || !StrKey.isValidEd25519PublicKey(clientAccount))
      return badRequest('invalid_challenge');

    const clientKeypair = Keypair.fromPublicKey(clientAccount);
    const sigs = tx.signatures || [];
    let clientSigned = false;
    for (const s of sigs) {
      try {
        if (clientKeypair.verify(txHash, s.signature())) {
          clientSigned = true;
          break;
        }
      } catch (e) {
        // ignore
      }
    }

    if (!clientSigned) return NextResponse.json({ error: 'invalid_signature' }, { status: 401 });

    const now = Math.floor(Date.now() / 1000);
    const exp = now + SEP10_TTL_SECONDS;

    const algSecret = sep10Key(JWT_SECRET);
    const token = await new SignJWT({ sub: clientAccount })
      .setProtectedHeader({ alg: 'HS256' })
      .setIssuer(SEP10_ISSUER)
      .setAudience(SEP10_AUDIENCE)
      .setJti(crypto.randomUUID())
      .setIssuedAt(now)
      .setExpirationTime(exp)
      .sign(algSecret);

    return NextResponse.json(
      { token, expires_at: exp },
      { headers: getRateLimitHeaders(rateLimitResult) },
    );
  } catch (err: unknown) {
    console.error('[SEP-0010] token error', err);
    return NextResponse.json({ error: 'invalid_request' }, { status: 400 });
  }
}
