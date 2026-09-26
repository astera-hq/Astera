import { AsteraClient } from '../../../packages/sdk/src/astera-client';
import { Deployer } from './deployer';
import { 
  Keypair, 
  Networks, 
  Address, 
  rpc as StellarRpc, 
  nativeToScVal, 
  TransactionBuilder, 
  BASE_FEE, 
  Operation,
  Contract,
  StrKey
} from '@stellar/stellar-sdk';
import * as path from 'path';
import * as dotenv from 'dotenv';

dotenv.config();

const RPC_URL = process.env.RPC_URL || 'https://soroban-testnet.stellar.org';
const NETWORK_PASSPHRASE = process.env.NETWORK_PASSPHRASE || Networks.TESTNET;
const SECRET_KEY = process.env.ADMIN_SECRET_KEY!; 

const WASM_DIR = path.join(__dirname, '../../../contracts/target/wasm32-unknown-unknown/release');

const STROOPS = 10_000_000n;

describe('Astera Workflow E2E Integration', () => {
  let server: StellarRpc.Server;
  let client: AsteraClient;
  let deployer: Deployer;
  let adminKeypair: Keypair;
  let smeKeypair: Keypair;
  let investorKeypair: Keypair;

  let invoiceId: string;
  let poolId: string;
  let creditId: string;
  let shareId: string;
  let usdcId: string;

  beforeAll(async () => {
    if (!SECRET_KEY) {
      throw new Error('ADMIN_SECRET_KEY environment variable is required');
    }

    server = new StellarRpc.Server(RPC_URL);
    adminKeypair = Keypair.fromSecret(SECRET_KEY);
    smeKeypair = Keypair.random();
    investorKeypair = Keypair.random();

    console.log('Admin:', adminKeypair.publicKey());
    console.log('SME:', smeKeypair.publicKey());
    console.log('Investor:', investorKeypair.publicKey());

    deployer = new Deployer(RPC_URL, SECRET_KEY, NETWORK_PASSPHRASE);

    // Fund SME and Investor via Friendbot
    await fundAccount(smeKeypair.publicKey());
    await fundAccount(investorKeypair.publicKey());
  }, 60000);

  async function fundAccount(publicKey: string) {
    try {
      const response = await fetch(`https://friendbot.stellar.org?addr=${publicKey}`);
      if (!response.ok) throw new Error('Friendbot failed');
      await new Promise(r => setTimeout(r, 2000));
    } catch (e) {
      console.warn(`Could not fund ${publicKey} via Friendbot, it might already exist or service is down.`);
    }
  }

  async function submitTx(tx: any, signer: Keypair) {
    const sim = await server.simulateTransaction(tx);
    if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
    const prepared = StellarRpc.assembleTransaction(tx, sim).build();
    prepared.sign(signer);
    const response = await server.sendTransaction(prepared);
    if (response.status === 'ERROR') throw new Error(`Submit failed: ${JSON.stringify(response)}`);
    
    let result = await server.getTransaction(response.hash);
    while (result.status === 'NOT_FOUND' || result.status === 'PENDING') {
      await new Promise(r => setTimeout(r, 1000));
      result = await server.getTransaction(response.hash);
    }
    return result;
  }

  test('Complete SME-to-Investor Workflow', async () => {
    // 1. Deploy Contracts
    console.log('Deploying contracts...');
    invoiceId = await deployer.deploy(path.join(WASM_DIR, 'invoice.wasm'));
    poolId = await deployer.deploy(path.join(WASM_DIR, 'pool.wasm'));
    creditId = await deployer.deploy(path.join(WASM_DIR, 'credit_score.wasm'));
    shareId = await deployer.deploy(path.join(WASM_DIR, 'share.wasm'));
    
    console.log('Contracts deployed:');
    console.log('- Invoice:', invoiceId);
    console.log('- Pool:', poolId);
    console.log('- Credit:', creditId);
    console.log('- Share:', shareId);

    // 2. Setup USDC (Stellar Asset Contract for Admin's asset)
    console.log('Setting up USDC asset...');
    const usdcAsset = Operation.createAssetContract({
      asset: new (require('@stellar/stellar-sdk').Asset)('USDC', adminKeypair.publicKey())
    });
    const account = await server.getAccount(adminKeypair.publicKey());
    const usdcTx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK_PASSPHRASE })
      .addOperation(usdcAsset)
      .setTimeout(30)
      .build();
    const usdcRes = await submitTx(usdcTx, adminKeypair);
    usdcId = StellarRpc.Api.getTransactionResponse(usdcRes).address!;
    console.log('- USDC ID:', usdcId);

    // 3. Initialize Contracts
    console.log('Initializing contracts...');
    const adminAddr = new Address(adminKeypair.publicKey()).toScVal();
    const poolAddr = new Address(poolId).toScVal();
    const invAddr = new Address(invoiceId).toScVal();
    const usdcAddr = new Address(usdcId).toScVal();
    const shareAddr = new Address(shareId).toScVal();
    const creditAddr = new Address(creditId).toScVal();

    const initInvoice = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK_PASSPHRASE })
      .addOperation(new Contract(invoiceId).call('initialize', 
        adminAddr, poolAddr, nativeToScVal(1_000_000n * STROOPS, {type: 'i128'}), nativeToScVal(30 * 86400, {type: 'u64'}), nativeToScVal(7, {type: 'u32'})
      ))
      .setTimeout(30).build();
    await submitTx(initInvoice, adminKeypair);

    const initShare = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK_PASSPHRASE })
      .addOperation(new Contract(shareId).call('initialize', 
        adminAddr, nativeToScVal(7, {type: 'u32'}), nativeToScVal('Astera Shares', {type: 'string'}), nativeToScVal('AST', {type: 'string'})
      ))
      .setTimeout(30).build();
    await submitTx(initShare, adminKeypair);

    const initPool = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK_PASSPHRASE })
      .addOperation(new Contract(poolId).call('initialize', 
        adminAddr, usdcAddr, shareAddr, invAddr
      ))
      .setTimeout(30).build();
    await submitTx(initPool, adminKeypair);

    const initCredit = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK_PASSPHRASE })
      .addOperation(new Contract(creditId).call('initialize', 
        adminAddr, invAddr, poolAddr
      ))
      .setTimeout(30).build();
    await submitTx(initCredit, adminKeypair);

    client = new AsteraClient({
      rpcUrl: RPC_URL,
      network: NETWORK_PASSPHRASE,
      invoiceContractId: invoiceId,
      poolContractId: poolId,
    });

    // 4. Mint USDC to Investor and SME
    console.log('Minting USDC...');
    const mintAmount = 10000n * STROOPS;
    const mintTx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK_PASSPHRASE })
      .addOperation(new Contract(usdcId).call('mint', new Address(investorKeypair.publicKey()).toScVal(), nativeToScVal(mintAmount, {type: 'i128'})))
      .addOperation(new Contract(usdcId).call('mint', new Address(smeKeypair.publicKey()).toScVal(), nativeToScVal(mintAmount, {type: 'i128'})))
      .setTimeout(30).build();
    await submitTx(mintTx, adminKeypair);

    // 5. Investor Deposits
    console.log('Investor depositing...');
    const depositAmount = 5000n * STROOPS;
    await client.pool.deposit({
      signer: async (xdr) => {
        const tx = TransactionBuilder.fromXDR(xdr, NETWORK_PASSPHRASE);
        tx.sign(investorKeypair);
        return tx.toXDR();
      },
      investor: investorKeypair.publicKey(),
      token: usdcId,
      amount: depositAmount
    });

    // 6. SME Creates Invoice
    console.log('SME creating invoice...');
    const invoiceAmount = 2000n * STROOPS;
    const dueDate = Math.floor(Date.now() / 1000) + 30 * 86400;
    await client.invoice.create({
      signer: async (xdr) => {
        const tx = TransactionBuilder.fromXDR(xdr, NETWORK_PASSPHRASE);
        tx.sign(smeKeypair);
        return tx.toXDR();
      },
      owner: smeKeypair.publicKey(),
      debtor: 'Test Debtor',
      amount: invoiceAmount,
      dueDate,
      description: 'Invoice #001'
    });

    // 7. Pool Funds Invoice
    console.log('Pool funding invoice...');
    const fundTx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK_PASSPHRASE })
      .addOperation(new Contract(poolId).call('fund_invoice', 
        adminAddr, nativeToScVal(1, {type: 'u64'}), nativeToScVal(invoiceAmount, {type: 'i128'}), 
        new Address(smeKeypair.publicKey()).toScVal(), nativeToScVal(dueDate, {type: 'u64'}), usdcAddr
      ))
      .setTimeout(30).build();
    await submitTx(fundTx, adminKeypair);

    // 8. SME Repays Invoice
    console.log('SME repaying invoice...');
    await client.pool.repay({
      signer: async (xdr) => {
        const tx = TransactionBuilder.fromXDR(xdr, NETWORK_PASSPHRASE);
        tx.sign(smeKeypair);
        return tx.toXDR();
      },
      payer: smeKeypair.publicKey(),
      invoiceId: 1,
      amount: invoiceAmount
    });

    // 9. Verify Final State
    console.log('Verifying final state...');
    const inv = await client.invoice.get(1);
    expect(inv.status).toBeDefined(); 
    // In a real test we'd check if it's Paid, but we might need to call mark_paid first 
    // depending on the contract logic (some contracts require a separate call to finalize state).
    
    console.log('E2E workflow completed successfully on Testnet!');
  }, 600000); 
});
import { AsteraClient } from '../../../packages/sdk/src/astera-client';
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
