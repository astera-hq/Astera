import { scValToNative, xdr } from './stellar';

export interface ContractEvent {
  type: string;
  timestamp: number;
  contractId: string;
  topics: string[];
  value: unknown[];
}

export interface PoolDepositEvent {
  type: 'pool:deposit';
  depositor: string;
  token: string;
  amount: bigint;
  sharesMinted: bigint;
  timestamp: number;
}

export interface PoolWithdrawalEvent {
  type: 'pool:withdrawal';
  withdrawer: string;
  token: string;
  amount: bigint;
  sharesBurned: bigint;
  timestamp: number;
}

export interface PoolYieldClaimedEvent {
  type: 'pool:yield_claimed';
  claimer: string;
  token: string;
  amount: bigint;
  timestamp: number;
}

export interface ShareMintEvent {
  type: 'share:mint';
  to: string;
  amount: bigint;
  timestamp: number;
}

export interface ShareBurnEvent {
  type: 'share:burn';
  from: string;
  amount: bigint;
  timestamp: number;
}

export interface ShareTransferEvent {
  type: 'share:transfer';
  from: string;
  to: string;
  amount: bigint;
}

export interface ShareApproveEvent {
  type: 'share:approve';
  owner: string;
  spender: string;
  amount: bigint;
}

export type ContractEventType =
  | PoolDepositEvent
  | PoolWithdrawalEvent
  | PoolYieldClaimedEvent
  | ShareMintEvent
  | ShareBurnEvent
  | ShareTransferEvent
  | ShareApproveEvent;

export function parseContractEvent(event: {
  topic: string[];
  value: xdr.ScVal[];
}): ContractEventType | null {
  const [namespace, action] = event.topic;

  if (!namespace || !action) {
    return null;
  }

  try {
    if (namespace === 'pool') {
      const [depositor, token, amount, sharesMinted, timestamp] = event.value.map((v) =>
        scValToNative(v),
      );

      switch (action) {
        case 'deposit':
          return {
            type: 'pool:deposit',
            depositor: String(depositor),
            token: String(token),
            amount: BigInt(String(amount)),
            sharesMinted: BigInt(String(sharesMinted)),
            timestamp: Number(timestamp),
          } as PoolDepositEvent;

        case 'withdrawal': {
          const [withdrawer, token, amount, sharesBurned, timestamp] = event.value.map((v) =>
            scValToNative(v),
          );
          return {
            type: 'pool:withdrawal',
            withdrawer: String(withdrawer),
            token: String(token),
            amount: BigInt(String(amount)),
            sharesBurned: BigInt(String(sharesBurned)),
            timestamp: Number(timestamp),
          } as PoolWithdrawalEvent;
        }

        case 'yield_claimed': {
          const [claimer, token, amount, timestamp] = event.value.map((v) =>
            scValToNative(v),
          );
          return {
            type: 'pool:yield_claimed',
            claimer: String(claimer),
            token: String(token),
            amount: BigInt(String(amount)),
            timestamp: Number(timestamp),
          } as PoolYieldClaimedEvent;
        }
      }
    }

    if (namespace === 'share') {
      const values = event.value.map((v) => scValToNative(v));

      switch (action) {
        case 'mint': {
          const [to, amount, timestamp] = values;
          return {
            type: 'share:mint',
            to: String(to),
            amount: BigInt(String(amount)),
            timestamp: Number(timestamp),
          } as ShareMintEvent;
        }

        case 'burn': {
          const [from, amount, timestamp] = values;
          return {
            type: 'share:burn',
            from: String(from),
            amount: BigInt(String(amount)),
            timestamp: Number(timestamp),
          } as ShareBurnEvent;
        }

        case 'transfer': {
          const [from, to, amount] = values;
          return {
            type: 'share:transfer',
            from: String(from),
            to: String(to),
            amount: BigInt(String(amount)),
          } as ShareTransferEvent;
        }

        case 'approve': {
          const [owner, spender, amount] = values;
          return {
            type: 'share:approve',
            owner: String(owner),
            spender: String(spender),
            amount: BigInt(String(amount)),
          } as ShareApproveEvent;
        }
      }
    }

    return null;
  } catch (error) {
    console.error('Failed to parse contract event:', error);
    return null;
  }
}
