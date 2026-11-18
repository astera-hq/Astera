import { NextResponse } from 'next/server';
import { TransactionBuilder, Networks, Keypair, StrKey } from '@stellar/stellar-sdk';
import { jwtVerify, SignJWT } from 'jose';

export const dynamic = 'force-dynamic';

const NETWORK_PASSPHRASE =
  process.env.NEXT_PUBLIC_STELLAR_NETWORK === 'mainnet' ? Networks.PUBLIC : Networks.TESTNET;

const JWT_SECRET = process.env.SEP10_JWT_SECRET || process.env.JWT_SECRET;
const SERVER_SEED = process.env.SEP10_SERVER_SEED;

function badRequest(msg = 'invalid') {
  return NextResponse.json({ error: msg }, { status: 400 });
}

export async function POST(request: Request) {
  if (!JWT_SECRET || !SERVER_SEED)
    return NextResponse.json({ error: 'server_not_configured' }, { status: 500 });

  const body = await request.json().catch(() => ({}));
  const signedXdr = String(body?.signed_xdr ?? '').trim();
  if (!signedXdr) return badRequest('missing_signed_xdr');

  try {
    // Parse transaction envelope
    const tx = TransactionBuilder.fromXDR(signedXdr, NETWORK_PASSPHRASE);
    const txHash = tx.hash();

    // Verify signatures: must contain a signature from the client pubkey
    // Extract all potential signers (signatures) and try verifying with claimed public key
    // The client public key is in the operation source (manageData.source)
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

    // Verify at least one signature matches clientAccount
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

    // Issue JWT
    const now = Math.floor(Date.now() / 1000);
    const exp = now + 60 * 60 * 24; // 24h

    const algSecret = new TextEncoder().encode(JWT_SECRET);
    const token = await new SignJWT({ sub: clientAccount })
      .setProtectedHeader({ alg: 'HS256' })
      .setIssuedAt(now)
      .setExpirationTime(exp)
      .sign(algSecret);

    return NextResponse.json({ token, expires_at: exp });
  } catch (err: unknown) {
    console.error('[SEP-0010] token error', err);
    return NextResponse.json({ error: 'invalid_request' }, { status: 400 });
  }
}
