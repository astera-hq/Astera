import { NextResponse } from 'next/server';
import { TransactionBuilder, Networks, Keypair, StrKey, Utils } from '@stellar/stellar-sdk';
import { SignJWT } from 'jose';
import { getSep10Secret, SEP10_AUDIENCE, SEP10_ISSUER, SEP10_TTL_SECONDS, sep10Key } from '@/lib/sep10-jwt';
import { checkRateLimit, getClientIdentifier, getRateLimitHeaders } from '@/lib/rateLimiter';
import { markNonceUsed } from '@/lib/nonce-store';

export const dynamic = 'force-dynamic';

const NETWORK_PASSPHRASE =
  process.env.NEXT_PUBLIC_STELLAR_NETWORK === 'mainnet' ? Networks.PUBLIC : Networks.TESTNET;

const JWT_SECRET = getSep10Secret();
const SERVER_SEED = process.env.SEP10_SERVER_SEED;
const HOME_DOMAIN = process.env.NEXT_PUBLIC_APP_URL 
  ? new URL(process.env.NEXT_PUBLIC_APP_URL).hostname 
  : 'localhost';

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
    const serverKeypair = Keypair.fromSecret(SERVER_SEED);
    
    if (tx.source !== serverKeypair.publicKey()) {
      return badRequest('invalid_source_account');
    }
    
    if (tx.sequence !== '0') {
      return badRequest('invalid_sequence');
    }
    
    const now = Math.floor(Date.now() / 1000);
    const timeBounds = tx.timeBounds;
    if (!timeBounds || timeBounds.maxTime === '0' || Number(timeBounds.maxTime) < now) {
      return badRequest('challenge_expired');
    }
    
    const txHash = tx.hash();
    let serverSigned = false;
    for (const sig of tx.signatures || []) {
      try {
        if (serverKeypair.verify(txHash, sig.signature())) {
          serverSigned = true;
          break;
        }
      } catch (e) {
        // ignore
      }
    }
    if (!serverSigned) {
      return badRequest('server_signature_missing');
    }

    const ops = tx.operations || [];
    if (ops.length !== 1 || ops[0].type !== 'manageData') {
      return badRequest('invalid_operations');
    }
    
    const op = ops[0];
    if (!op.source || !StrKey.isValidEd25519PublicKey(op.source)) {
      return badRequest('invalid_client_account');
    }
    
    if (op.name !== `${HOME_DOMAIN} auth`) {
      return badRequest('invalid_operation_name');
    }
    
    const nonce = op.value?.toString('base64');
    if (!nonce || !markNonceUsed(nonce)) {
      return badRequest('nonce_already_used');
    }

    const clientAccount = op.source;
    const clientKeypair = Keypair.fromPublicKey(clientAccount);
    let clientSigned = false;
    for (const sig of tx.signatures || []) {
      try {
        if (clientKeypair.verify(txHash, sig.signature())) {
          clientSigned = true;
          break;
        }
      } catch (e) {
        // ignore
      }
    }

    if (!clientSigned) return NextResponse.json({ error: 'invalid_signature' }, { status: 401 });

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
