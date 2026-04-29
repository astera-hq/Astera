import { AsteraClient } from '../../../sdk/src/client';
import { Deployer } from './deployer';
import { 
  Keypair, 
  Networks, 
  Address, 
  rpc as StellarRpc, 
  nativeToScVal, 
  TransactionBuilder, 
  BASE_FEE, 
  Contract,
} from '@stellar/stellar-sdk';
import * as path from 'path';
import * as dotenv from 'dotenv';
import { Verifier } from '../../../oracle-service/src/verifier';

dotenv.config();

const RPC_URL = process.env.RPC_URL || 'http://localhost:8000/soroban/rpc';
const NETWORK_PASSPHRASE = process.env.NETWORK_PASSPHRASE || 'Standalone Network ; February 2017';
const SECRET_KEY = process.env.ADMIN_SECRET_KEY || 'SAAPYAPTTRZMC66XGSA76A7E6ASKSADSA'; // Mock for standalone

const WASM_DIR = path.join(__dirname, '../../../contracts/target/wasm32-unknown-unknown/release');

const STROOPS = 10_000_000n;

describe('Oracle Verification E2E', () => {
  let server: StellarRpc.Server;
  let client: AsteraClient;
  let deployer: Deployer;
  let adminKeypair: Keypair;
  let smeKeypair: Keypair;
  let oracleKeypair: Keypair;

  let invoiceId: string;

  beforeAll(async () => {
    server = new StellarRpc.Server(RPC_URL);
    adminKeypair = Keypair.fromSecret(SECRET_KEY);
    smeKeypair = Keypair.random();
    oracleKeypair = Keypair.random();

    deployer = new Deployer(RPC_URL, SECRET_KEY, NETWORK_PASSPHRASE);
    
    // In standalone mode, we might need to fund these
    // But since it's a test environment, we assume the deployer can do it or they are funded.
  }, 60000);

  test('Invoice Creation to Oracle Verification Flow', async () => {
    // 1. Deploy
    invoiceId = await deployer.deploy(path.join(WASM_DIR, 'invoice.wasm'));
    const dummyPoolId = Keypair.random().publicKey();

    // 2. Initialize
    const account = await server.getAccount(adminKeypair.publicKey());
    const initInvoice = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK_PASSPHRASE })
      .addOperation(new Contract(invoiceId).call('initialize', 
        new Address(adminKeypair.publicKey()).toScVal(), 
        new Address(dummyPoolId).toScVal(), 
        nativeToScVal(1_000_000n * STROOPS, {type: 'i128'}), 
        nativeToScVal(30 * 86400, {type: 'u64'}), 
        nativeToScVal(7, {type: 'u32'})
      ))
      .setTimeout(30).build();
    initInvoice.sign(adminKeypair);
    await server.sendTransaction(initInvoice);

    // 3. Set Oracle
    const setOracle = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK_PASSPHRASE })
      .addOperation(new Contract(invoiceId).call('set_oracle', 
        new Address(adminKeypair.publicKey()).toScVal(), 
        new Address(oracleKeypair.publicKey()).toScVal()
      ))
      .setTimeout(30).build();
    setOracle.sign(adminKeypair);
    await server.sendTransaction(setOracle);

    client = new AsteraClient({
      rpcUrl: RPC_URL,
      network: NETWORK_PASSPHRASE,
      invoiceContractId: invoiceId,
      poolContractId: dummyPoolId,
    });

    // 4. SME Creates Invoice
    const amount = 1000n * STROOPS;
    const dueDate = Math.floor(Date.now() / 1000) + 3600;
    const vHash = 'hash123';
    
    await client.invoice.create({
      signer: async (xdr) => {
        const tx = TransactionBuilder.fromXDR(xdr, NETWORK_PASSPHRASE);
        tx.sign(smeKeypair);
        return tx.toXDR();
      },
      owner: smeKeypair.publicKey(),
      debtor: 'Test Debtor',
      amount,
      dueDate,
      description: 'Test Oracle Flow',
      verificationHash: vHash
    });

    // 5. Verify status is AwaitingVerification
    let inv = await client.invoice.get(1);
    expect(inv.status).toBe('AwaitingVerification');

    // 6. Trigger Oracle Verification (using the Verifier class)
    const verifier = new Verifier({
      rpcUrl: RPC_URL,
      horizonUrl: RPC_URL.replace('/soroban/rpc', ''), // Mock horizon for standalone
      networkPassphrase: NETWORK_PASSPHRASE,
      oracleSecretKey: oracleKeypair.secret(),
      invoiceContractId: invoiceId,
      autoVerifyDelayMs: 100
    });

    await verifier.verifyInvoice(1n);

    // 7. Check final status
    inv = await client.invoice.get(1);
    expect(inv.status).toBe('Verified');
    expect(inv.oracle_verified).toBe(true);
  }, 120000);
});
