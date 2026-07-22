import { Keypair, TransactionBuilder } from '@stellar/stellar-sdk';
import { AsteraClient } from '../../sdk/src/client';
import { OracleConfig } from './types';

/**
 * #861: on startup, checks this node's registered stake against the
 * registry's minimum and logs a warning if the node isn't registered (or has
 * been deregistered/slashed below the minimum) so an operator notices before
 * assuming their votes are actually counting toward quorum.
 */
export async function checkStakeOnStartup(
  config: OracleConfig,
  client: AsteraClient,
  oraclePublicKey: string,
): Promise<void> {
  if (!config.oracleRegistryContractId) {
    return;
  }

  try {
    const info = await client.oracleRegistry.getOracleInfo(oraclePublicKey);
    if (!info || !info.isActive) {
      console.warn(
        `[Staking] ${oraclePublicKey} is not an active registered oracle on ${config.oracleRegistryContractId}. ` +
          'Votes from this node will be rejected until it registers stake (see: npm start -- --register).',
      );
      return;
    }
    console.log(
      `[Staking] Registered with ${info.stakeAmount} staked ` +
        `(${info.totalVerifications} verifications, ${info.totalSlashes} slashes so far).`,
    );
  } catch (error) {
    console.warn(`[Staking] Could not check registration status: ${error}`);
  }
}

/**
 * #861: `npm start -- --register` performs the one-time `register_oracle`
 * call using `REGISTER_STAKE_AMOUNT`, then exits — a minimal CLI so an
 * operator doesn't need to hand-build a Soroban transaction to stand up a new
 * node. Intentionally not run automatically on every startup: staking is a
 * one-time, funds-moving action an operator should trigger deliberately.
 */
export async function registerIfRequested(
  config: OracleConfig,
  client: AsteraClient,
  oracleKeypair: Keypair,
): Promise<boolean> {
  if (!process.argv.includes('--register')) {
    return false;
  }
  if (!config.oracleRegistryContractId) {
    console.error('[Staking] --register requires ORACLE_REGISTRY_CONTRACT_ID to be set.');
    process.exit(1);
  }
  if (!config.registerStakeAmount || config.registerStakeAmount <= 0n) {
    console.error('[Staking] --register requires REGISTER_STAKE_AMOUNT to be set to a positive integer.');
    process.exit(1);
  }

  console.log(
    `[Staking] Registering ${oracleKeypair.publicKey()} with stake ${config.registerStakeAmount}...`,
  );
  const txHash = await client.oracleRegistry.register({
    signer: async (xdr) => {
      const tx = TransactionBuilder.fromXDR(xdr, config.networkPassphrase);
      tx.sign(oracleKeypair);
      return tx.toXDR();
    },
    operator: oracleKeypair.publicKey(),
    stakeAmount: config.registerStakeAmount,
  });
  console.log(`[Staking] Registered. Tx hash: ${txHash}`);
  return true;
}
