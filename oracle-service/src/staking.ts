import { Keypair, TransactionBuilder } from '@stellar/stellar-sdk';
import { AsteraClient } from 'astera-sdk';
import { OracleConfig } from './types';

export interface SlashRecord {
  bps: number;
  slashAmount: string;
  admin: string;
  occurredAt?: string;
}

const MAX_RECENT_SLASHES = 20;

/**
 * #979: tracks `slash_oracle` events (from `slashed` events streamed by
 * `listener.ts`) for this node's own oracle address, so the metrics endpoint
 * can report recent slashes without a persisted database — mirrors
 * `ConsensusTracker`'s approach of deriving in-memory state purely from the
 * event stream.
 */
export class StakingMetricsTracker {
  private recentSlashes: SlashRecord[] = [];

  constructor(private readonly oraclePublicKey: string) {}

  /** Call with the decoded `(topic1, topic2)` pair, event value, and (if
   * available) the effect's `created_at` timestamp for every event emitted
   * under the registry contract's "ORACLE" topic namespace. */
  handleEvent(topic2: string, value: unknown, occurredAt?: string): void {
    if (topic2 !== 'slashed') {
      return;
    }
    const [operator, bps, slashAmount] = Array.isArray(value) ? value : [value];
    if (String(operator) !== this.oraclePublicKey) {
      return;
    }
    const admin = Array.isArray(value) ? value[3] : undefined;
    this.recentSlashes.unshift({
      bps: Number(bps),
      slashAmount: String(slashAmount),
      admin: String(admin),
      occurredAt,
    });
    this.recentSlashes.length = Math.min(this.recentSlashes.length, MAX_RECENT_SLASHES);
  }

  list(): SlashRecord[] {
    return [...this.recentSlashes];
  }
}

export interface DeregisterCooldown {
  requestedAt: number;
  cooldownSecs: number;
  readyAt: number;
  remainingSecs: number;
}

export interface StakingMetrics {
  oracle: string;
  registered: boolean;
  stakeAmount: string;
  totalVerifications: number;
  totalSlashes: number;
  deregisterCooldown: DeregisterCooldown | null;
  recentSlashes: SlashRecord[];
}

/**
 * #979: exposes this operator's own stake/cooldown/slash status — the same
 * data `checkStakeOnStartup` fetches once at boot — as an on-demand snapshot
 * for the health/metrics endpoint, so operators don't have to query the chain
 * directly to check on their node.
 */
export async function getStakingMetrics(
  client: AsteraClient,
  oraclePublicKey: string,
  tracker: StakingMetricsTracker,
): Promise<StakingMetrics> {
  const info = await client.oracleRegistry.getOracleInfo(oraclePublicKey);
  if (!info) {
    return {
      oracle: oraclePublicKey,
      registered: false,
      stakeAmount: '0',
      totalVerifications: 0,
      totalSlashes: 0,
      deregisterCooldown: null,
      recentSlashes: tracker.list(),
    };
  }

  let deregisterCooldown: DeregisterCooldown | null = null;
  if (info.deregisterRequestedAt !== undefined) {
    const registryConfig = await client.oracleRegistry.getRegistryConfig();
    const readyAt = info.deregisterRequestedAt + registryConfig.deregisterCooldownSecs;
    const nowSecs = Math.floor(Date.now() / 1000);
    deregisterCooldown = {
      requestedAt: info.deregisterRequestedAt,
      cooldownSecs: registryConfig.deregisterCooldownSecs,
      readyAt,
      remainingSecs: Math.max(0, readyAt - nowSecs),
    };
  }

  return {
    oracle: oraclePublicKey,
    registered: info.isActive,
    stakeAmount: info.stakeAmount.toString(),
    totalVerifications: info.totalVerifications,
    totalSlashes: info.totalSlashes,
    deregisterCooldown,
    recentSlashes: tracker.list(),
  };
}

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
