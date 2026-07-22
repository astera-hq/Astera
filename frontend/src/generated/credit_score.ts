import { Buffer } from "buffer";
import { Address } from "@stellar/stellar-sdk";
import {
  AssembledTransaction,
  Client as ContractClient,
  ClientOptions as ContractClientOptions,
  MethodOptions,
  Result,
  Spec as ContractSpec,
} from "@stellar/stellar-sdk/contract";
import type {
  u32,
  i32,
  u64,
  i64,
  u128,
  i128,
  u256,
  i256,
  Option,
  Timepoint,
  Duration,
} from "@stellar/stellar-sdk/contract";
export * from "@stellar/stellar-sdk";
export * as contract from "@stellar/stellar-sdk/contract";
export * as rpc from "@stellar/stellar-sdk/rpc";

if (typeof window !== "undefined") {
  //@ts-ignore Buffer exists
  window.Buffer = window.Buffer || Buffer;
}




export type DataKey = {tag: "CreditScore", values: readonly [string]} | {tag: "PaymentHistory", values: readonly [string]} | {tag: "PaymentHistoryStart", values: readonly [string]} | {tag: "PaymentRecordIdx", values: readonly [string, u32]} | {tag: "PaymentRecordScoreVersion", values: readonly [u64]} | {tag: "InvoiceProcessed", values: readonly [u64]} | {tag: "ScoringConfig", values: void} | {tag: "Admin", values: void} | {tag: "InvoiceContract", values: void} | {tag: "PoolContract", values: void} | {tag: "Initialized", values: void} | {tag: "ScoreVersion", values: void} | {tag: "MaxPaymentHistory", values: void} | {tag: "Paused", values: void} | {tag: "ProposedWasmHash", values: void} | {tag: "UpgradeScheduledAt", values: void} | {tag: "ContractVersion", values: void} | {tag: "MigrationVersion", values: void} | {tag: "LateThreshold", values: void} | {tag: "ScoreThresholds", values: void} | {tag: "UpgradeTimelockSecs", values: void} | {tag: "MilestoneCount", values: readonly [string]} | {tag: "PendingAdmin", values: void} | {tag: "AdminChangeScheduledAt", values: void} | {tag: "SmeByIndex", values: readonly [u32]} | {tag: "SmeCount", values: void} | {tag: "Attestor", values: readonly [string]} | {tag: "ActiveAttestorList", values: void} | {tag: "AttestationCount", values: void} | {tag: "Attestation", values: readonly [u64]} | {tag: "SmeAttestations", values: readonly [string]} | {tag: "AttestationDisputeReason", values: readonly [u64]};


/**
 * #868: a single external credit signal submitted by a registered attestor
 * about an SME. `score_contribution` is the attestor's own 0–1000 normalized
 * signal — not directly the final blended score.
 */
export interface Attestation {
  attestation_type: AttestorType;
  attestor: string;
  /**
 * Off-chain document/report hash — no PII stored on-chain.
 */
evidence_hash: string;
  expires_at: u64;
  id: u64;
  score_contribution: u32;
  sme: string;
  status: AttestationStatus;
  submitted_at: u64;
}


/**
 * #868: a registered source of external credit signal.
 */
export interface AttestorInfo {
  address: string;
  attestor_type: AttestorType;
  is_active: boolean;
  registered_at: u64;
  /**
 * Basis points (1–10_000) weighting this attestor's contribution within
 * the weighted average of an SME's active attestations.
 */
weight_bps: u32;
}

/**
 * #868: category of an external attestor / attestation.
 */
export type AttestorType = {tag: "BusinessRegistry", values: void} | {tag: "CreditBureau", values: void} | {tag: "ExternalProtocol", values: void} | {tag: "Manual", values: void};


export interface PaymentRecord {
  amount: i128;
  days_late: i64;
  due_date: u64;
  invoice_id: u64;
  paid_at: u64;
  sme: string;
  status: PaymentStatus;
}

export type PaymentStatus = {tag: "PaidOnTime", values: void} | {tag: "PaidLate", values: void} | {tag: "Defaulted", values: void};


export interface ScoringConfig {
  attestation: ScoreAttestationConfig;
  averages: ScoreAverageConfig;
  bonuses: ScoreBonusConfig;
  core: ScoreCoreConfig;
}


export interface SmeScoreEntry {
  score: u32;
  sme: string;
}


export interface CreditScoreData {
  average_payment_days: i64;
  defaulted: u32;
  last_updated: u64;
  paid_late: u32;
  paid_on_time: u32;
  score: u32;
  score_version: u32;
  sme: string;
  total_invoices: u32;
  total_volume: i128;
}


export interface ScoreCoreConfig {
  base_score: u32;
  defaulted_pts: i32;
  max_score: u32;
  min_score: u32;
  paid_late_pts: i32;
  paid_on_time_pts: i32;
  score_version: u32;
}


export interface ScoreThresholds {
  excellent: u32;
  fair: u32;
  good: u32;
  very_good: u32;
}

/**
 * #396: Typed error codes for the credit-score contract.
 * All error codes are stable — do not re-number existing entries.
 */
export const Errors = {
  /**
   * Contract has already been initialised.
   */
  1: {message:"AlreadyInitialized"},
  /**
   * Caller is not the contract admin.
   */
  2: {message:"Unauthorized"},
  /**
   * Contract is paused; state-changing calls are blocked.
   */
  3: {message:"ContractPaused"},
  /**
   * This invoice has already been recorded in the credit score.
   */
  4: {message:"InvoiceAlreadyProcessed"},
  /**
   * Score thresholds are not strictly decreasing.
   */
  5: {message:"InvalidThresholds"},
  /**
   * Late-payment threshold is outside the valid 1–365 day range.
   */
  6: {message:"InvalidLateThreshold"},
  /**
   * Payment history limit must be greater than zero.
   */
  7: {message:"PaymentHistoryLimitZero"},
  /**
   * Upgrade timelock has not yet elapsed.
   */
  8: {message:"UpgradeTimelockNotExpired"},
  /**
   * No upgrade has been proposed.
   */
  9: {message:"NoUpgradeProposed"},
  /**
   * #338: upgrade timelock value is below the allowed minimum.
   */
  10: {message:"InvalidUpgradeTimelock"},
  /**
   * #340: proposed WASM hash is all-zero (invalid).
   */
  11: {message:"InvalidWasmHash"},
  /**
   * #565: admin change already pending.
   */
  12: {message:"AdminChangePending"},
  /**
   * #565: admin change timelock has not elapsed.
   */
  13: {message:"AdminChangeTimelockNotExpired"},
  /**
   * #565: no admin change has been proposed.
   */
  14: {message:"NoAdminChangeProposed"},
  /**
   * #868: attestor address has not been registered.
   */
  15: {message:"AttestorNotFound"},
  /**
   * #868: attestor exists but has been deactivated.
   */
  16: {message:"AttestorInactive"},
  /**
   * #868: attestor weight_bps must be in (0, 10_000].
   */
  17: {message:"InvalidAttestorWeight"},
  /**
   * #868: attestation score_contribution must be in [0, 1000].
   */
  18: {message:"InvalidScoreContribution"},
  /**
   * #868: attestation expires_at must be in the future and within the max horizon.
   */
  19: {message:"InvalidAttestationExpiry"},
  /**
   * #868: no attestation exists with the given id.
   */
  20: {message:"AttestationNotFound"},
  /**
   * #868: attestation is not in the Active status required for this operation.
   */
  21: {message:"AttestationNotActive"},
  /**
   * #868: caller is neither the attested SME nor the admin.
   */
  22: {message:"UnauthorizedDisputeCaller"},
  /**
   * #868: attestation is not currently under dispute.
   */
  23: {message:"DisputeNotFound"},
  /**
   * #868: internal/external attestation blend weights are invalid (must sum to 10_000 bps).
   */
  24: {message:"InvalidAttestationConfig"}
}


export interface ScoreBonusConfig {
  inv_bonus_pts: i32;
  inv_bonus_thr1: u32;
  inv_bonus_thr2: u32;
  inv_bonus_thr3: u32;
  /**
 * #568: Minimum invoice amount that contributes to milestone counters.
 * Invoices below this threshold still update total counts/volume but do
 * not increment the milestone counter used for inv_bonus_thr1/2/3.
 */
min_milestone_volume: i128;
  vol_bonus_pts1: i32;
  vol_bonus_pts2: i32;
  vol_bonus_pts3: i32;
  vol_bonus_thr1: i128;
  vol_bonus_thr2: i128;
  vol_bonus_thr3: i128;
}

export type AttestationStatus = {tag: "Active", values: void} | {tag: "Disputed", values: void} | {tag: "Revoked", values: void} | {tag: "Expired", values: void};


/**
 * Semantic version of this credit-score contract (#237).
 */
export interface CreditScoreVersion {
  major: u32;
  minor: u32;
  patch: u32;
}


export interface ScoreAverageConfig {
  avg_days_lt3: i64;
  avg_days_lt7: i64;
  avg_lt3_pts: i32;
  avg_lt7_pts: i32;
  avg_neg_pts: i32;
  avg_over_late_pts: i32;
}


/**
 * Returned by `get_credit_score`. Includes the current config version alongside the
 * stored score so callers can detect staleness in a single call without a separate
 * `get_scoring_config()` round-trip.
 * 
 * `is_stale` is true when `score_version` (the config version active when the score
 * was last computed) does not match `config_version` (the config version now active).
 * A stale flag means the stored score was computed under different scoring parameters
 * and should be treated as approximate until the SME's next payment is recorded.
 */
export interface CreditScoreResponse {
  average_payment_days: i64;
  /**
 * #868: `score` blended with the SME's active external attestations
 * (`ScoreAttestationConfig` weights). Equal to `score` when the SME has
 * zero active attestations — existing SMEs are never regressed by v2.
 */
blended_score: u32;
  /**
 * Config version currently active on the contract.
 */
config_version: u32;
  defaulted: u32;
  /**
 * True when `score_version != config_version` — the score is stale.
 */
is_stale: boolean;
  last_updated: u64;
  paid_late: u32;
  paid_on_time: u32;
  score: u32;
  /**
 * Config version that was active when this score was last computed.
 */
score_version: u32;
  sme: string;
  total_invoices: u32;
  total_volume: i128;
}


/**
 * #868: blend ratio between the internal (payment-history-derived) score and
 * the external-attestation-derived component. Versioned alongside the rest
 * of `ScoringConfig` (see `core.score_version`) so a re-weighting cannot
 * silently reinterpret previously-computed scores.
 */
export interface ScoreAttestationConfig {
  external_weight_bps: u32;
  internal_weight_bps: u32;
}

export interface Client {
  /**
   * Construct and simulate a pause transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  pause: ({admin}: {admin: string}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a unpause transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  unpause: ({admin}: {admin: string}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a version transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the semantic version of this deployed credit-score contract (#237).
   */
  version: (options?: MethodOptions) => Promise<AssembledTransaction<CreditScoreVersion>>

  /**
   * Construct and simulate a is_paused transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  is_paused: (options?: MethodOptions) => Promise<AssembledTransaction<boolean>>

  /**
   * Construct and simulate a get_config transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_config: (options?: MethodOptions) => Promise<AssembledTransaction<readonly [string, string, string]>>

  /**
   * Construct and simulate a initialize transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  initialize: ({admin, invoice_contract, pool_contract}: {admin: string, invoice_contract: string, pool_contract: string}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a run_migration transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Run pending storage migrations after a WASM upgrade (#397).
   * 
   * Admin-only and idempotent: once the contract has reached
   * `CURRENT_MIGRATION_VERSION` further calls are a no-op. Each migration
   * step transforms the persistent storage layout for one schema version
   * and is meant to be invoked manually after `execute_upgrade`.
   */
  run_migration: ({admin}: {admin: string}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a get_score_band transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_score_band: ({score}: {score: u32}, options?: MethodOptions) => Promise<AssembledTransaction<string>>

  /**
   * Construct and simulate a record_default transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  record_default: ({_caller, invoice_id, sme, amount, due_date}: {_caller: string, invoice_id: u64, sme: string, amount: i128, due_date: u64}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a record_funding transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Record that the borrower's invoice has been successfully funded (#534).
   * Being funded is a positive demand signal — lenders deployed capital,
   * which is evidence of creditworthiness. The weight is intentionally light
   * so funding events alone cannot override a poor repayment history.
   * 
   * Idempotent: duplicate calls for the same invoice_id are silently ignored.
   */
  record_funding: ({_caller, invoice_id, sme, amount}: {_caller: string, invoice_id: u64, sme: string, amount: i128}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a record_payment transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  record_payment: ({_caller, invoice_id, sme, amount, due_date, paid_at}: {_caller: string, invoice_id: u64, sme: string, amount: i128, due_date: u64, paid_at: u64}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a execute_upgrade transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  execute_upgrade: ({admin}: {admin: string}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a get_attestation transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_attestation: ({id}: {id: u64}, options?: MethodOptions) => Promise<AssembledTransaction<Option<Attestation>>>

  /**
   * Construct and simulate a propose_upgrade transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  propose_upgrade: ({admin, wasm_hash}: {admin: string, wasm_hash: Buffer}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a get_credit_score transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_credit_score: ({sme}: {sme: string}, options?: MethodOptions) => Promise<AssembledTransaction<CreditScoreResponse>>

  /**
   * Construct and simulate a get_attestor_info transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_attestor_info: ({address}: {address: string}, options?: MethodOptions) => Promise<AssembledTransaction<Option<AttestorInfo>>>

  /**
   * Construct and simulate a migration_version transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the applied storage-schema migration level (#397).
   */
  migration_version: (options?: MethodOptions) => Promise<AssembledTransaction<u32>>

  /**
   * Construct and simulate a register_attestor transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Register (or re-register/reactivate) an attestor. Admin-only.
   */
  register_attestor: ({admin, address, attestor_type, weight_bps}: {admin: string, address: string, attestor_type: AttestorType, weight_bps: u32}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a set_pool_contract transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_pool_contract: ({admin, pool_contract}: {admin: string, pool_contract: string}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a get_late_threshold transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the current late-payment threshold in days (default 30).
   */
  get_late_threshold: (options?: MethodOptions) => Promise<AssembledTransaction<i64>>

  /**
   * Construct and simulate a get_payment_record transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_payment_record: ({sme, index}: {sme: string, index: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Option<PaymentRecord>>>

  /**
   * Construct and simulate a get_scoring_config transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_scoring_config: (options?: MethodOptions) => Promise<AssembledTransaction<ScoringConfig>>

  /**
   * Construct and simulate a list_all_sme_stats transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  list_all_sme_stats: ({page, page_size}: {page: u32, page_size: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Array<SmeScoreEntry>>>

  /**
   * Construct and simulate a set_late_threshold transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Set the late-payment threshold (in days) used in score calculation.
   * Default is 30 days. Valid range: 1–365.
   */
  set_late_threshold: ({admin, days}: {admin: string, days: i64}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a set_scoring_config transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_scoring_config: ({admin, config}: {admin: string, config: ScoringConfig}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a submit_attestation transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Submit an attestation about `sme`. Caller must be a registered, active
   * attestor. Returns the new attestation id.
   */
  submit_attestation: ({attestor, sme, attestation_type, score_contribution, evidence_hash, expires_at}: {attestor: string, sme: string, attestation_type: AttestorType, score_contribution: u32, evidence_hash: string, expires_at: u64}, options?: MethodOptions) => Promise<AssembledTransaction<u64>>

  /**
   * Construct and simulate a cancel_admin_change transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Cancel a pending admin key rotation (#565).
   */
  cancel_admin_change: ({admin}: {admin: string}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a deactivate_attestor transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Deactivate a registered attestor. Admin-only. Does not retroactively
   * invalidate attestations the attestor already submitted — only new
   * `submit_attestation` calls from them are blocked going forward.
   */
  deactivate_attestor: ({admin, address}: {admin: string, address: string}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a dispute_attestation transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * File a dispute against an attestation. Callable by the attested SME or
   * the admin. Immediately excludes the attestation from blended scoring.
   */
  dispute_attestation: ({caller, attestation_id, reason_hash}: {caller: string, attestation_id: u64, reason_hash: string}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a get_payment_history transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_payment_history: ({sme}: {sme: string}, options?: MethodOptions) => Promise<AssembledTransaction<Array<PaymentRecord>>>

  /**
   * Construct and simulate a get_score_thresholds transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_score_thresholds: (options?: MethodOptions) => Promise<AssembledTransaction<ScoreThresholds>>

  /**
   * Construct and simulate a get_upgrade_timelock transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the configured upgrade timelock in seconds (#338).
   */
  get_upgrade_timelock: (options?: MethodOptions) => Promise<AssembledTransaction<u64>>

  /**
   * Construct and simulate a is_invoice_processed transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  is_invoice_processed: ({invoice_id}: {invoice_id: u64}, options?: MethodOptions) => Promise<AssembledTransaction<boolean>>

  /**
   * Construct and simulate a propose_admin_change transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Propose an admin key rotation (#565).
   */
  propose_admin_change: ({admin, new_admin}: {admin: string, new_admin: string}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a set_invoice_contract transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_invoice_contract: ({admin, invoice_contract}: {admin: string, invoice_contract: string}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a set_score_thresholds transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_score_thresholds: ({admin, thresholds}: {admin: string, thresholds: ScoreThresholds}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a set_upgrade_timelock transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Set the upgrade timelock duration in seconds (#338).
   * Minimum: 3,600 s (1 h). Default: 86,400 s (24 h).
   */
  set_upgrade_timelock: ({admin, secs}: {admin: string, secs: u64}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a finalize_admin_change transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Finalize a previously proposed admin key rotation (#565).
   * Only callable by the current admin after the 48-hour timelock has elapsed.
   */
  finalize_admin_change: ({admin}: {admin: string}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a list_active_attestors transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  list_active_attestors: (options?: MethodOptions) => Promise<AssembledTransaction<Array<AttestorInfo>>>

  /**
   * Construct and simulate a list_sme_attestations transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  list_sme_attestations: ({sme}: {sme: string}, options?: MethodOptions) => Promise<AssembledTransaction<Array<Attestation>>>

  /**
   * Construct and simulate a get_max_payment_history transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_max_payment_history: (options?: MethodOptions) => Promise<AssembledTransaction<u32>>

  /**
   * Construct and simulate a set_max_payment_history transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_max_payment_history: ({admin, max_history}: {admin: string, max_history: u32}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a get_payment_history_length transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_payment_history_length: ({sme}: {sme: string}, options?: MethodOptions) => Promise<AssembledTransaction<u32>>

  /**
   * Construct and simulate a resolve_attestation_dispute transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Resolve a pending attestation dispute. Admin-only. If the attestation
   * is upheld (`upheld = true`) it returns to `Active`; otherwise it is
   * permanently `Revoked`.
   */
  resolve_attestation_dispute: ({admin, attestation_id, upheld}: {admin: string, attestation_id: u64, upheld: boolean}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a get_attestation_dispute_reason transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_attestation_dispute_reason: ({id}: {id: u64}, options?: MethodOptions) => Promise<AssembledTransaction<Option<string>>>

  /**
   * Construct and simulate a get_payment_record_score_version transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_payment_record_score_version: ({invoice_id}: {invoice_id: u64}, options?: MethodOptions) => Promise<AssembledTransaction<Option<u32>>>

  /**
   * Construct and simulate a simulate_score_with_attestations transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Preview the blended score as if `hypothetical` additional attestations
   * (weight_bps, score_contribution) existed, without persisting anything.
   * Powers a "what would my score be if I verified X" frontend flow.
   */
  simulate_score_with_attestations: ({sme, hypothetical}: {sme: string, hypothetical: Array<readonly [u32, u32]>}, options?: MethodOptions) => Promise<AssembledTransaction<u32>>

}
export class Client extends ContractClient {
  static async deploy<T = Client>(
    /** Options for initializing a Client as well as for calling a method, with extras specific to deploying. */
    options: MethodOptions &
      Omit<ContractClientOptions, "contractId"> & {
        /** The hash of the Wasm blob, which must already be installed on-chain. */
        wasmHash: Buffer | string;
        /** Salt used to generate the contract's ID. Passed through to {@link Operation.createCustomContract}. Default: random. */
        salt?: Buffer | Uint8Array;
        /** The format used to decode `wasmHash`, if it's provided as a string. */
        format?: "hex" | "base64";
      }
  ): Promise<AssembledTransaction<T>> {
    return ContractClient.deploy(null, options)
  }
  constructor(public readonly options: ContractClientOptions) {
    super(
      new ContractSpec([ "AAAAAAAAAAAAAAAFcGF1c2UAAAAAAAABAAAAAAAAAAVhZG1pbgAAAAAAABMAAAAA",
        "AAAAAAAAAAAAAAAHdW5wYXVzZQAAAAABAAAAAAAAAAVhZG1pbgAAAAAAABMAAAAA",
        "AAAAAAAAAEtSZXR1cm5zIHRoZSBzZW1hbnRpYyB2ZXJzaW9uIG9mIHRoaXMgZGVwbG95ZWQgY3JlZGl0LXNjb3JlIGNvbnRyYWN0ICgjMjM3KS4AAAAAB3ZlcnNpb24AAAAAAAAAAAEAAAfQAAAAEkNyZWRpdFNjb3JlVmVyc2lvbgAA",
        "AAAAAAAAAAAAAAAJaXNfcGF1c2VkAAAAAAAAAAAAAAEAAAAB",
        "AAAAAgAAAAAAAAAAAAAAB0RhdGFLZXkAAAAAIAAAAAEAAAAAAAAAC0NyZWRpdFNjb3JlAAAAAAEAAAATAAAAAQAAAEFOdW1iZXIgb2YgcmV0YWluZWQgcmVjb3JkcyBpbiB0aGUgcm9sbGluZyBwYXltZW50IGhpc3Rvcnkgd2luZG93LgAAAAAAAA5QYXltZW50SGlzdG9yeQAAAAAAAQAAABMAAAABAAAAAAAAABNQYXltZW50SGlzdG9yeVN0YXJ0AAAAAAEAAAATAAAAAQAAAAAAAAAQUGF5bWVudFJlY29yZElkeAAAAAIAAAATAAAABAAAAAEAAAAAAAAAGVBheW1lbnRSZWNvcmRTY29yZVZlcnNpb24AAAAAAAABAAAABgAAAAEAAAAAAAAAEEludm9pY2VQcm9jZXNzZWQAAAABAAAABgAAAAAAAAAAAAAADVNjb3JpbmdDb25maWcAAAAAAAAAAAAAAAAAAAVBZG1pbgAAAAAAAAAAAAAAAAAAD0ludm9pY2VDb250cmFjdAAAAAAAAAAAAAAAAAxQb29sQ29udHJhY3QAAAAAAAAAAAAAAAtJbml0aWFsaXplZAAAAAAAAAAAAAAAAAxTY29yZVZlcnNpb24AAAAAAAAAPFNpemUgb2YgdGhlIHJvbGxpbmcgcGF5bWVudC1oaXN0b3J5IHdpbmRvdyByZXRhaW5lZCBwZXIgU01FLgAAABFNYXhQYXltZW50SGlzdG9yeQAAAAAAAAAAAAAAAAAABlBhdXNlZAAAAAAAAAAAAAAAAAAQUHJvcG9zZWRXYXNtSGFzaAAAAAAAAAAAAAAAElVwZ3JhZGVTY2hlZHVsZWRBdAAAAAAAAAAAADNTZW1hbnRpYyB2ZXJzaW9uIHN0b3JlZCBkdXJpbmcgaW5pdGlhbGl6ZSgpICgjMjM3KS4AAAAAD0NvbnRyYWN0VmVyc2lvbgAAAAAAAAAALkFwcGxpZWQgc3RvcmFnZS1zY2hlbWEgbWlncmF0aW9uIGxldmVsICgjMzk3KS4AAAAAABBNaWdyYXRpb25WZXJzaW9uAAAAAAAAADNDb25maWd1cmFibGUgbGF0ZS1wYXltZW50IHRocmVzaG9sZCBpbiBkYXlzICgjNDMwKS4AAAAADUxhdGVUaHJlc2hvbGQAAAAAAAAAAAAARiM0Mjg6IENvbmZpZ3VyYWJsZSBzY29yZSB0aHJlc2hvbGRzIChFeGNlbGxlbnQsIFZlcnkgR29vZCwgR29vZCwgRmFpcikAAAAAAA9TY29yZVRocmVzaG9sZHMAAAAAAAAAADcjMzM4OiBjb25maWd1cmFibGUgdXBncmFkZSB0aW1lbG9jayBkdXJhdGlvbiBpbiBzZWNvbmRzAAAAABNVcGdyYWRlVGltZWxvY2tTZWNzAAAAAAEAAADAIzU2ODogQ291bnQgb2YgaW52b2ljZXMgcGVyIFNNRSB3aXRoIGFtb3VudCA+PSBtaW5fbWlsZXN0b25lX3ZvbHVtZSwKZXhjbHVkaW5nIGRlZmF1bHRzLiBVc2VkIGFzIHRoZSBpbnB1dCB0byBtaWxlc3RvbmUgYm9udXMgdGhyZXNob2xkcyBzbwp0aGF0IG1pY3JvLWludm9pY2UgY3ljbGluZyBjYW5ub3QgaW5mbGF0ZSB0aGUgc2NvcmUuAAAADk1pbGVzdG9uZUNvdW50AAAAAAABAAAAEwAAAAAAAAAhIzU2NTogYWRtaW4ga2V5IHJvdGF0aW9uIHRpbWVsb2NrAAAAAAAADFBlbmRpbmdBZG1pbgAAAAAAAAAAAAAAFkFkbWluQ2hhbmdlU2NoZWR1bGVkQXQAAAAAAAEAAAAAAAAAClNtZUJ5SW5kZXgAAAAAAAEAAAAEAAAAAAAAAAAAAAAIU21lQ291bnQAAAABAAAAOiM4Njg6IHJlZ2lzdGVyZWQgYXR0ZXN0b3IgaW5mbywga2V5ZWQgYnkgYXR0ZXN0b3IgYWRkcmVzcy4AAAAAAAhBdHRlc3RvcgAAAAEAAAATAAAAAAAAADIjODY4OiBhZGRyZXNzZXMgb2YgYWxsIGN1cnJlbnRseS1hY3RpdmUgYXR0ZXN0b3JzLgAAAAAAEkFjdGl2ZUF0dGVzdG9yTGlzdAAAAAAAAAAAACcjODY4OiBtb25vdG9uaWMgYXR0ZXN0YXRpb24gaWQgY291bnRlci4AAAAAEEF0dGVzdGF0aW9uQ291bnQAAAABAAAAJiM4Njg6IGF0dGVzdGF0aW9uIHJlY29yZCwga2V5ZWQgYnkgaWQuAAAAAAALQXR0ZXN0YXRpb24AAAAAAQAAAAYAAAABAAAAOiM4Njg6IGlkcyBvZiBhbGwgYXR0ZXN0YXRpb25zIGV2ZXIgc3VibWl0dGVkIGFib3V0IGFuIFNNRS4AAAAAAA9TbWVBdHRlc3RhdGlvbnMAAAAAAQAAABMAAAABAAAATSM4Njg6IHJlYXNvbiBoYXNoIHN1cHBsaWVkIHRvIGBkaXNwdXRlX2F0dGVzdGF0aW9uYCwga2V5ZWQgYnkgYXR0ZXN0YXRpb24gaWQuAAAAAAAAGEF0dGVzdGF0aW9uRGlzcHV0ZVJlYXNvbgAAAAEAAAAG",
        "AAAAAAAAAAAAAAAKZ2V0X2NvbmZpZwAAAAAAAAAAAAEAAAPtAAAAAwAAABMAAAATAAAAEw==",
        "AAAAAAAAAAAAAAAKaW5pdGlhbGl6ZQAAAAAAAwAAAAAAAAAFYWRtaW4AAAAAAAATAAAAAAAAABBpbnZvaWNlX2NvbnRyYWN0AAAAEwAAAAAAAAANcG9vbF9jb250cmFjdAAAAAAAABMAAAAA",
        "AAAAAAAAAT1SdW4gcGVuZGluZyBzdG9yYWdlIG1pZ3JhdGlvbnMgYWZ0ZXIgYSBXQVNNIHVwZ3JhZGUgKCMzOTcpLgoKQWRtaW4tb25seSBhbmQgaWRlbXBvdGVudDogb25jZSB0aGUgY29udHJhY3QgaGFzIHJlYWNoZWQKYENVUlJFTlRfTUlHUkFUSU9OX1ZFUlNJT05gIGZ1cnRoZXIgY2FsbHMgYXJlIGEgbm8tb3AuIEVhY2ggbWlncmF0aW9uCnN0ZXAgdHJhbnNmb3JtcyB0aGUgcGVyc2lzdGVudCBzdG9yYWdlIGxheW91dCBmb3Igb25lIHNjaGVtYSB2ZXJzaW9uCmFuZCBpcyBtZWFudCB0byBiZSBpbnZva2VkIG1hbnVhbGx5IGFmdGVyIGBleGVjdXRlX3VwZ3JhZGVgLgAAAAAAAA1ydW5fbWlncmF0aW9uAAAAAAAAAQAAAAAAAAAFYWRtaW4AAAAAAAATAAAAAA==",
        "AAAAAQAAAMYjODY4OiBhIHNpbmdsZSBleHRlcm5hbCBjcmVkaXQgc2lnbmFsIHN1Ym1pdHRlZCBieSBhIHJlZ2lzdGVyZWQgYXR0ZXN0b3IKYWJvdXQgYW4gU01FLiBgc2NvcmVfY29udHJpYnV0aW9uYCBpcyB0aGUgYXR0ZXN0b3IncyBvd24gMOKAkzEwMDAgbm9ybWFsaXplZApzaWduYWwg4oCUIG5vdCBkaXJlY3RseSB0aGUgZmluYWwgYmxlbmRlZCBzY29yZS4AAAAAAAAAAAALQXR0ZXN0YXRpb24AAAAACQAAAAAAAAAQYXR0ZXN0YXRpb25fdHlwZQAAB9AAAAAMQXR0ZXN0b3JUeXBlAAAAAAAAAAhhdHRlc3RvcgAAABMAAAA6T2ZmLWNoYWluIGRvY3VtZW50L3JlcG9ydCBoYXNoIOKAlCBubyBQSUkgc3RvcmVkIG9uLWNoYWluLgAAAAAADWV2aWRlbmNlX2hhc2gAAAAAAAAQAAAAAAAAAApleHBpcmVzX2F0AAAAAAAGAAAAAAAAAAJpZAAAAAAABgAAAAAAAAASc2NvcmVfY29udHJpYnV0aW9uAAAAAAAEAAAAAAAAAANzbWUAAAAAEwAAAAAAAAAGc3RhdHVzAAAAAAfQAAAAEUF0dGVzdGF0aW9uU3RhdHVzAAAAAAAAAAAAAAxzdWJtaXR0ZWRfYXQAAAAG",
        "AAAAAAAAAAAAAAAOZ2V0X3Njb3JlX2JhbmQAAAAAAAEAAAAAAAAABXNjb3JlAAAAAAAABAAAAAEAAAAQ",
        "AAAAAAAAAAAAAAAOcmVjb3JkX2RlZmF1bHQAAAAAAAUAAAAAAAAAB19jYWxsZXIAAAAAEwAAAAAAAAAKaW52b2ljZV9pZAAAAAAABgAAAAAAAAADc21lAAAAABMAAAAAAAAABmFtb3VudAAAAAAACwAAAAAAAAAIZHVlX2RhdGUAAAAGAAAAAA==",
        "AAAAAAAAAWRSZWNvcmQgdGhhdCB0aGUgYm9ycm93ZXIncyBpbnZvaWNlIGhhcyBiZWVuIHN1Y2Nlc3NmdWxseSBmdW5kZWQgKCM1MzQpLgpCZWluZyBmdW5kZWQgaXMgYSBwb3NpdGl2ZSBkZW1hbmQgc2lnbmFsIOKAlCBsZW5kZXJzIGRlcGxveWVkIGNhcGl0YWwsCndoaWNoIGlzIGV2aWRlbmNlIG9mIGNyZWRpdHdvcnRoaW5lc3MuIFRoZSB3ZWlnaHQgaXMgaW50ZW50aW9uYWxseSBsaWdodApzbyBmdW5kaW5nIGV2ZW50cyBhbG9uZSBjYW5ub3Qgb3ZlcnJpZGUgYSBwb29yIHJlcGF5bWVudCBoaXN0b3J5LgoKSWRlbXBvdGVudDogZHVwbGljYXRlIGNhbGxzIGZvciB0aGUgc2FtZSBpbnZvaWNlX2lkIGFyZSBzaWxlbnRseSBpZ25vcmVkLgAAAA5yZWNvcmRfZnVuZGluZwAAAAAABAAAAAAAAAAHX2NhbGxlcgAAAAATAAAAAAAAAAppbnZvaWNlX2lkAAAAAAAGAAAAAAAAAANzbWUAAAAAEwAAAAAAAAAGYW1vdW50AAAAAAALAAAAAA==",
        "AAAAAAAAAAAAAAAOcmVjb3JkX3BheW1lbnQAAAAAAAYAAAAAAAAAB19jYWxsZXIAAAAAEwAAAAAAAAAKaW52b2ljZV9pZAAAAAAABgAAAAAAAAADc21lAAAAABMAAAAAAAAABmFtb3VudAAAAAAACwAAAAAAAAAIZHVlX2RhdGUAAAAGAAAAAAAAAAdwYWlkX2F0AAAAAAYAAAAA",
        "AAAAAQAAADQjODY4OiBhIHJlZ2lzdGVyZWQgc291cmNlIG9mIGV4dGVybmFsIGNyZWRpdCBzaWduYWwuAAAAAAAAAAxBdHRlc3RvckluZm8AAAAFAAAAAAAAAAdhZGRyZXNzAAAAABMAAAAAAAAADWF0dGVzdG9yX3R5cGUAAAAAAAfQAAAADEF0dGVzdG9yVHlwZQAAAAAAAAAJaXNfYWN0aXZlAAAAAAAAAQAAAAAAAAANcmVnaXN0ZXJlZF9hdAAAAAAAAAYAAAB9QmFzaXMgcG9pbnRzICgx4oCTMTBfMDAwKSB3ZWlnaHRpbmcgdGhpcyBhdHRlc3RvcidzIGNvbnRyaWJ1dGlvbiB3aXRoaW4KdGhlIHdlaWdodGVkIGF2ZXJhZ2Ugb2YgYW4gU01FJ3MgYWN0aXZlIGF0dGVzdGF0aW9ucy4AAAAAAAAKd2VpZ2h0X2JwcwAAAAAABA==",
        "AAAAAgAAADUjODY4OiBjYXRlZ29yeSBvZiBhbiBleHRlcm5hbCBhdHRlc3RvciAvIGF0dGVzdGF0aW9uLgAAAAAAAAAAAAAMQXR0ZXN0b3JUeXBlAAAABAAAAAAAAAAAAAAAEEJ1c2luZXNzUmVnaXN0cnkAAAAAAAAAAAAAAAxDcmVkaXRCdXJlYXUAAAAAAAAAAAAAABBFeHRlcm5hbFByb3RvY29sAAAAAAAAAAAAAAAGTWFudWFsAAA=",
        "AAAAAAAAAAAAAAAPZXhlY3V0ZV91cGdyYWRlAAAAAAEAAAAAAAAABWFkbWluAAAAAAAAEwAAAAA=",
        "AAAAAAAAAAAAAAAPZ2V0X2F0dGVzdGF0aW9uAAAAAAEAAAAAAAAAAmlkAAAAAAAGAAAAAQAAA+gAAAfQAAAAC0F0dGVzdGF0aW9uAA==",
        "AAAAAAAAAAAAAAAPcHJvcG9zZV91cGdyYWRlAAAAAAIAAAAAAAAABWFkbWluAAAAAAAAEwAAAAAAAAAJd2FzbV9oYXNoAAAAAAAD7gAAACAAAAAA",
        "AAAAAQAAAAAAAAAAAAAADVBheW1lbnRSZWNvcmQAAAAAAAAHAAAAAAAAAAZhbW91bnQAAAAAAAsAAAAAAAAACWRheXNfbGF0ZQAAAAAAAAcAAAAAAAAACGR1ZV9kYXRlAAAABgAAAAAAAAAKaW52b2ljZV9pZAAAAAAABgAAAAAAAAAHcGFpZF9hdAAAAAAGAAAAAAAAAANzbWUAAAAAEwAAAAAAAAAGc3RhdHVzAAAAAAfQAAAADVBheW1lbnRTdGF0dXMAAAA=",
        "AAAAAgAAAAAAAAAAAAAADVBheW1lbnRTdGF0dXMAAAAAAAADAAAAAAAAAAAAAAAKUGFpZE9uVGltZQAAAAAAAAAAAAAAAAAIUGFpZExhdGUAAAAAAAAAAAAAAAlEZWZhdWx0ZWQAAAA=",
        "AAAAAQAAAAAAAAAAAAAADVNjb3JpbmdDb25maWcAAAAAAAAEAAAAAAAAAAthdHRlc3RhdGlvbgAAAAfQAAAAFlNjb3JlQXR0ZXN0YXRpb25Db25maWcAAAAAAAAAAAAIYXZlcmFnZXMAAAfQAAAAElNjb3JlQXZlcmFnZUNvbmZpZwAAAAAAAAAAAAdib251c2VzAAAAB9AAAAAQU2NvcmVCb251c0NvbmZpZwAAAAAAAAAEY29yZQAAB9AAAAAPU2NvcmVDb3JlQ29uZmlnAA==",
        "AAAAAQAAAAAAAAAAAAAADVNtZVNjb3JlRW50cnkAAAAAAAACAAAAAAAAAAVzY29yZQAAAAAAAAQAAAAAAAAAA3NtZQAAAAAT",
        "AAAAAAAAAAAAAAAQZ2V0X2NyZWRpdF9zY29yZQAAAAEAAAAAAAAAA3NtZQAAAAATAAAAAQAAB9AAAAATQ3JlZGl0U2NvcmVSZXNwb25zZQA=",
        "AAAAAAAAAAAAAAARZ2V0X2F0dGVzdG9yX2luZm8AAAAAAAABAAAAAAAAAAdhZGRyZXNzAAAAABMAAAABAAAD6AAAB9AAAAAMQXR0ZXN0b3JJbmZv",
        "AAAAAAAAADpSZXR1cm5zIHRoZSBhcHBsaWVkIHN0b3JhZ2Utc2NoZW1hIG1pZ3JhdGlvbiBsZXZlbCAoIzM5NykuAAAAAAARbWlncmF0aW9uX3ZlcnNpb24AAAAAAAAAAAAAAQAAAAQ=",
        "AAAAAAAAAD1SZWdpc3RlciAob3IgcmUtcmVnaXN0ZXIvcmVhY3RpdmF0ZSkgYW4gYXR0ZXN0b3IuIEFkbWluLW9ubHkuAAAAAAAAEXJlZ2lzdGVyX2F0dGVzdG9yAAAAAAAABAAAAAAAAAAFYWRtaW4AAAAAAAATAAAAAAAAAAdhZGRyZXNzAAAAABMAAAAAAAAADWF0dGVzdG9yX3R5cGUAAAAAAAfQAAAADEF0dGVzdG9yVHlwZQAAAAAAAAAKd2VpZ2h0X2JwcwAAAAAABAAAAAA=",
        "AAAAAAAAAAAAAAARc2V0X3Bvb2xfY29udHJhY3QAAAAAAAACAAAAAAAAAAVhZG1pbgAAAAAAABMAAAAAAAAADXBvb2xfY29udHJhY3QAAAAAAAATAAAAAA==",
        "AAAAAQAAAAAAAAAAAAAAD0NyZWRpdFNjb3JlRGF0YQAAAAAKAAAAAAAAABRhdmVyYWdlX3BheW1lbnRfZGF5cwAAAAcAAAAAAAAACWRlZmF1bHRlZAAAAAAAAAQAAAAAAAAADGxhc3RfdXBkYXRlZAAAAAYAAAAAAAAACXBhaWRfbGF0ZQAAAAAAAAQAAAAAAAAADHBhaWRfb25fdGltZQAAAAQAAAAAAAAABXNjb3JlAAAAAAAABAAAAAAAAAANc2NvcmVfdmVyc2lvbgAAAAAAAAQAAAAAAAAAA3NtZQAAAAATAAAAAAAAAA50b3RhbF9pbnZvaWNlcwAAAAAABAAAAAAAAAAMdG90YWxfdm9sdW1lAAAACw==",
        "AAAAAQAAAAAAAAAAAAAAD1Njb3JlQ29yZUNvbmZpZwAAAAAHAAAAAAAAAApiYXNlX3Njb3JlAAAAAAAEAAAAAAAAAA1kZWZhdWx0ZWRfcHRzAAAAAAAABQAAAAAAAAAJbWF4X3Njb3JlAAAAAAAABAAAAAAAAAAJbWluX3Njb3JlAAAAAAAABAAAAAAAAAANcGFpZF9sYXRlX3B0cwAAAAAAAAUAAAAAAAAAEHBhaWRfb25fdGltZV9wdHMAAAAFAAAAAAAAAA1zY29yZV92ZXJzaW9uAAAAAAAABA==",
        "AAAAAQAAAAAAAAAAAAAAD1Njb3JlVGhyZXNob2xkcwAAAAAEAAAAAAAAAAlleGNlbGxlbnQAAAAAAAAEAAAAAAAAAARmYWlyAAAABAAAAAAAAAAEZ29vZAAAAAQAAAAAAAAACXZlcnlfZ29vZAAAAAAAAAQ=",
        "AAAAAAAAAEBSZXR1cm5zIHRoZSBjdXJyZW50IGxhdGUtcGF5bWVudCB0aHJlc2hvbGQgaW4gZGF5cyAoZGVmYXVsdCAzMCkuAAAAEmdldF9sYXRlX3RocmVzaG9sZAAAAAAAAAAAAAEAAAAH",
        "AAAAAAAAAAAAAAASZ2V0X3BheW1lbnRfcmVjb3JkAAAAAAACAAAAAAAAAANzbWUAAAAAEwAAAAAAAAAFaW5kZXgAAAAAAAAEAAAAAQAAA+gAAAfQAAAADVBheW1lbnRSZWNvcmQAAAA=",
        "AAAAAAAAAAAAAAASZ2V0X3Njb3JpbmdfY29uZmlnAAAAAAAAAAAAAQAAB9AAAAANU2NvcmluZ0NvbmZpZwAAAA==",
        "AAAAAAAAAAAAAAASbGlzdF9hbGxfc21lX3N0YXRzAAAAAAACAAAAAAAAAARwYWdlAAAABAAAAAAAAAAJcGFnZV9zaXplAAAAAAAABAAAAAEAAAPqAAAH0AAAAA1TbWVTY29yZUVudHJ5AAAA",
        "AAAAAAAAAG1TZXQgdGhlIGxhdGUtcGF5bWVudCB0aHJlc2hvbGQgKGluIGRheXMpIHVzZWQgaW4gc2NvcmUgY2FsY3VsYXRpb24uCkRlZmF1bHQgaXMgMzAgZGF5cy4gVmFsaWQgcmFuZ2U6IDHigJMzNjUuAAAAAAAAEnNldF9sYXRlX3RocmVzaG9sZAAAAAAAAgAAAAAAAAAFYWRtaW4AAAAAAAATAAAAAAAAAARkYXlzAAAABwAAAAA=",
        "AAAAAAAAAAAAAAASc2V0X3Njb3JpbmdfY29uZmlnAAAAAAACAAAAAAAAAAVhZG1pbgAAAAAAABMAAAAAAAAABmNvbmZpZwAAAAAH0AAAAA1TY29yaW5nQ29uZmlnAAAAAAAAAA==",
        "AAAAAAAAAHBTdWJtaXQgYW4gYXR0ZXN0YXRpb24gYWJvdXQgYHNtZWAuIENhbGxlciBtdXN0IGJlIGEgcmVnaXN0ZXJlZCwgYWN0aXZlCmF0dGVzdG9yLiBSZXR1cm5zIHRoZSBuZXcgYXR0ZXN0YXRpb24gaWQuAAAAEnN1Ym1pdF9hdHRlc3RhdGlvbgAAAAAABgAAAAAAAAAIYXR0ZXN0b3IAAAATAAAAAAAAAANzbWUAAAAAEwAAAAAAAAAQYXR0ZXN0YXRpb25fdHlwZQAAB9AAAAAMQXR0ZXN0b3JUeXBlAAAAAAAAABJzY29yZV9jb250cmlidXRpb24AAAAAAAQAAAAAAAAADWV2aWRlbmNlX2hhc2gAAAAAAAAQAAAAAAAAAApleHBpcmVzX2F0AAAAAAAGAAAAAQAAAAY=",
        "AAAABAAAAHgjMzk2OiBUeXBlZCBlcnJvciBjb2RlcyBmb3IgdGhlIGNyZWRpdC1zY29yZSBjb250cmFjdC4KQWxsIGVycm9yIGNvZGVzIGFyZSBzdGFibGUg4oCUIGRvIG5vdCByZS1udW1iZXIgZXhpc3RpbmcgZW50cmllcy4AAAAAAAAAEENyZWRpdFNjb3JlRXJyb3IAAAAYAAAAJkNvbnRyYWN0IGhhcyBhbHJlYWR5IGJlZW4gaW5pdGlhbGlzZWQuAAAAAAASQWxyZWFkeUluaXRpYWxpemVkAAAAAAABAAAAIUNhbGxlciBpcyBub3QgdGhlIGNvbnRyYWN0IGFkbWluLgAAAAAAAAxVbmF1dGhvcml6ZWQAAAACAAAANUNvbnRyYWN0IGlzIHBhdXNlZDsgc3RhdGUtY2hhbmdpbmcgY2FsbHMgYXJlIGJsb2NrZWQuAAAAAAAADkNvbnRyYWN0UGF1c2VkAAAAAAADAAAAO1RoaXMgaW52b2ljZSBoYXMgYWxyZWFkeSBiZWVuIHJlY29yZGVkIGluIHRoZSBjcmVkaXQgc2NvcmUuAAAAABdJbnZvaWNlQWxyZWFkeVByb2Nlc3NlZAAAAAAEAAAALVNjb3JlIHRocmVzaG9sZHMgYXJlIG5vdCBzdHJpY3RseSBkZWNyZWFzaW5nLgAAAAAAABFJbnZhbGlkVGhyZXNob2xkcwAAAAAAAAUAAAA+TGF0ZS1wYXltZW50IHRocmVzaG9sZCBpcyBvdXRzaWRlIHRoZSB2YWxpZCAx4oCTMzY1IGRheSByYW5nZS4AAAAAABRJbnZhbGlkTGF0ZVRocmVzaG9sZAAAAAYAAAAwUGF5bWVudCBoaXN0b3J5IGxpbWl0IG11c3QgYmUgZ3JlYXRlciB0aGFuIHplcm8uAAAAF1BheW1lbnRIaXN0b3J5TGltaXRaZXJvAAAAAAcAAAAlVXBncmFkZSB0aW1lbG9jayBoYXMgbm90IHlldCBlbGFwc2VkLgAAAAAAABlVcGdyYWRlVGltZWxvY2tOb3RFeHBpcmVkAAAAAAAACAAAAB1ObyB1cGdyYWRlIGhhcyBiZWVuIHByb3Bvc2VkLgAAAAAAABFOb1VwZ3JhZGVQcm9wb3NlZAAAAAAAAAkAAAA6IzMzODogdXBncmFkZSB0aW1lbG9jayB2YWx1ZSBpcyBiZWxvdyB0aGUgYWxsb3dlZCBtaW5pbXVtLgAAAAAAFkludmFsaWRVcGdyYWRlVGltZWxvY2sAAAAAAAoAAAAvIzM0MDogcHJvcG9zZWQgV0FTTSBoYXNoIGlzIGFsbC16ZXJvIChpbnZhbGlkKS4AAAAAD0ludmFsaWRXYXNtSGFzaAAAAAALAAAAIyM1NjU6IGFkbWluIGNoYW5nZSBhbHJlYWR5IHBlbmRpbmcuAAAAABJBZG1pbkNoYW5nZVBlbmRpbmcAAAAAAAwAAAAsIzU2NTogYWRtaW4gY2hhbmdlIHRpbWVsb2NrIGhhcyBub3QgZWxhcHNlZC4AAAAdQWRtaW5DaGFuZ2VUaW1lbG9ja05vdEV4cGlyZWQAAAAAAAANAAAAKCM1NjU6IG5vIGFkbWluIGNoYW5nZSBoYXMgYmVlbiBwcm9wb3NlZC4AAAAVTm9BZG1pbkNoYW5nZVByb3Bvc2VkAAAAAAAADgAAAC8jODY4OiBhdHRlc3RvciBhZGRyZXNzIGhhcyBub3QgYmVlbiByZWdpc3RlcmVkLgAAAAAQQXR0ZXN0b3JOb3RGb3VuZAAAAA8AAAAvIzg2ODogYXR0ZXN0b3IgZXhpc3RzIGJ1dCBoYXMgYmVlbiBkZWFjdGl2YXRlZC4AAAAAEEF0dGVzdG9ySW5hY3RpdmUAAAAQAAAAMSM4Njg6IGF0dGVzdG9yIHdlaWdodF9icHMgbXVzdCBiZSBpbiAoMCwgMTBfMDAwXS4AAAAAAAAVSW52YWxpZEF0dGVzdG9yV2VpZ2h0AAAAAAAAEQAAADojODY4OiBhdHRlc3RhdGlvbiBzY29yZV9jb250cmlidXRpb24gbXVzdCBiZSBpbiBbMCwgMTAwMF0uAAAAAAAYSW52YWxpZFNjb3JlQ29udHJpYnV0aW9uAAAAEgAAAE4jODY4OiBhdHRlc3RhdGlvbiBleHBpcmVzX2F0IG11c3QgYmUgaW4gdGhlIGZ1dHVyZSBhbmQgd2l0aGluIHRoZSBtYXggaG9yaXpvbi4AAAAAABhJbnZhbGlkQXR0ZXN0YXRpb25FeHBpcnkAAAATAAAALiM4Njg6IG5vIGF0dGVzdGF0aW9uIGV4aXN0cyB3aXRoIHRoZSBnaXZlbiBpZC4AAAAAABNBdHRlc3RhdGlvbk5vdEZvdW5kAAAAABQAAABKIzg2ODogYXR0ZXN0YXRpb24gaXMgbm90IGluIHRoZSBBY3RpdmUgc3RhdHVzIHJlcXVpcmVkIGZvciB0aGlzIG9wZXJhdGlvbi4AAAAAABRBdHRlc3RhdGlvbk5vdEFjdGl2ZQAAABUAAAA3Izg2ODogY2FsbGVyIGlzIG5laXRoZXIgdGhlIGF0dGVzdGVkIFNNRSBub3IgdGhlIGFkbWluLgAAAAAZVW5hdXRob3JpemVkRGlzcHV0ZUNhbGxlcgAAAAAAABYAAAAxIzg2ODogYXR0ZXN0YXRpb24gaXMgbm90IGN1cnJlbnRseSB1bmRlciBkaXNwdXRlLgAAAAAAAA9EaXNwdXRlTm90Rm91bmQAAAAAFwAAAFcjODY4OiBpbnRlcm5hbC9leHRlcm5hbCBhdHRlc3RhdGlvbiBibGVuZCB3ZWlnaHRzIGFyZSBpbnZhbGlkIChtdXN0IHN1bSB0byAxMF8wMDAgYnBzKS4AAAAAGEludmFsaWRBdHRlc3RhdGlvbkNvbmZpZwAAABg=",
        "AAAAAQAAAAAAAAAAAAAAEFNjb3JlQm9udXNDb25maWcAAAALAAAAAAAAAA1pbnZfYm9udXNfcHRzAAAAAAAABQAAAAAAAAAOaW52X2JvbnVzX3RocjEAAAAAAAQAAAAAAAAADmludl9ib251c190aHIyAAAAAAAEAAAAAAAAAA5pbnZfYm9udXNfdGhyMwAAAAAABAAAAMsjNTY4OiBNaW5pbXVtIGludm9pY2UgYW1vdW50IHRoYXQgY29udHJpYnV0ZXMgdG8gbWlsZXN0b25lIGNvdW50ZXJzLgpJbnZvaWNlcyBiZWxvdyB0aGlzIHRocmVzaG9sZCBzdGlsbCB1cGRhdGUgdG90YWwgY291bnRzL3ZvbHVtZSBidXQgZG8Kbm90IGluY3JlbWVudCB0aGUgbWlsZXN0b25lIGNvdW50ZXIgdXNlZCBmb3IgaW52X2JvbnVzX3RocjEvMi8zLgAAAAAUbWluX21pbGVzdG9uZV92b2x1bWUAAAALAAAAAAAAAA52b2xfYm9udXNfcHRzMQAAAAAABQAAAAAAAAAOdm9sX2JvbnVzX3B0czIAAAAAAAUAAAAAAAAADnZvbF9ib251c19wdHMzAAAAAAAFAAAAAAAAAA52b2xfYm9udXNfdGhyMQAAAAAACwAAAAAAAAAOdm9sX2JvbnVzX3RocjIAAAAAAAsAAAAAAAAADnZvbF9ib251c190aHIzAAAAAAAL",
        "AAAAAAAAACtDYW5jZWwgYSBwZW5kaW5nIGFkbWluIGtleSByb3RhdGlvbiAoIzU2NSkuAAAAABNjYW5jZWxfYWRtaW5fY2hhbmdlAAAAAAEAAAAAAAAABWFkbWluAAAAAAAAEwAAAAA=",
        "AAAAAAAAAMhEZWFjdGl2YXRlIGEgcmVnaXN0ZXJlZCBhdHRlc3Rvci4gQWRtaW4tb25seS4gRG9lcyBub3QgcmV0cm9hY3RpdmVseQppbnZhbGlkYXRlIGF0dGVzdGF0aW9ucyB0aGUgYXR0ZXN0b3IgYWxyZWFkeSBzdWJtaXR0ZWQg4oCUIG9ubHkgbmV3CmBzdWJtaXRfYXR0ZXN0YXRpb25gIGNhbGxzIGZyb20gdGhlbSBhcmUgYmxvY2tlZCBnb2luZyBmb3J3YXJkLgAAABNkZWFjdGl2YXRlX2F0dGVzdG9yAAAAAAIAAAAAAAAABWFkbWluAAAAAAAAEwAAAAAAAAAHYWRkcmVzcwAAAAATAAAAAA==",
        "AAAAAAAAAIxGaWxlIGEgZGlzcHV0ZSBhZ2FpbnN0IGFuIGF0dGVzdGF0aW9uLiBDYWxsYWJsZSBieSB0aGUgYXR0ZXN0ZWQgU01FIG9yCnRoZSBhZG1pbi4gSW1tZWRpYXRlbHkgZXhjbHVkZXMgdGhlIGF0dGVzdGF0aW9uIGZyb20gYmxlbmRlZCBzY29yaW5nLgAAABNkaXNwdXRlX2F0dGVzdGF0aW9uAAAAAAMAAAAAAAAABmNhbGxlcgAAAAAAEwAAAAAAAAAOYXR0ZXN0YXRpb25faWQAAAAAAAYAAAAAAAAAC3JlYXNvbl9oYXNoAAAAABAAAAAA",
        "AAAAAAAAAAAAAAATZ2V0X3BheW1lbnRfaGlzdG9yeQAAAAABAAAAAAAAAANzbWUAAAAAEwAAAAEAAAPqAAAH0AAAAA1QYXltZW50UmVjb3JkAAAA",
        "AAAAAgAAAAAAAAAAAAAAEUF0dGVzdGF0aW9uU3RhdHVzAAAAAAAABAAAAAAAAAAAAAAABkFjdGl2ZQAAAAAAAAAAAAAAAAAIRGlzcHV0ZWQAAAAAAAAAAAAAAAdSZXZva2VkAAAAAAAAAAAAAAAAB0V4cGlyZWQA",
        "AAAAAAAAAAAAAAAUZ2V0X3Njb3JlX3RocmVzaG9sZHMAAAAAAAAAAQAAB9AAAAAPU2NvcmVUaHJlc2hvbGRzAA==",
        "AAAAAAAAADpSZXR1cm5zIHRoZSBjb25maWd1cmVkIHVwZ3JhZGUgdGltZWxvY2sgaW4gc2Vjb25kcyAoIzMzOCkuAAAAAAAUZ2V0X3VwZ3JhZGVfdGltZWxvY2sAAAAAAAAAAQAAAAY=",
        "AAAAAAAAAAAAAAAUaXNfaW52b2ljZV9wcm9jZXNzZWQAAAABAAAAAAAAAAppbnZvaWNlX2lkAAAAAAAGAAAAAQAAAAE=",
        "AAAAAAAAACVQcm9wb3NlIGFuIGFkbWluIGtleSByb3RhdGlvbiAoIzU2NSkuAAAAAAAAFHByb3Bvc2VfYWRtaW5fY2hhbmdlAAAAAgAAAAAAAAAFYWRtaW4AAAAAAAATAAAAAAAAAAluZXdfYWRtaW4AAAAAAAATAAAAAA==",
        "AAAAAAAAAAAAAAAUc2V0X2ludm9pY2VfY29udHJhY3QAAAACAAAAAAAAAAVhZG1pbgAAAAAAABMAAAAAAAAAEGludm9pY2VfY29udHJhY3QAAAATAAAAAA==",
        "AAAAAAAAAAAAAAAUc2V0X3Njb3JlX3RocmVzaG9sZHMAAAACAAAAAAAAAAVhZG1pbgAAAAAAABMAAAAAAAAACnRocmVzaG9sZHMAAAAAB9AAAAAPU2NvcmVUaHJlc2hvbGRzAAAAAAA=",
        "AAAAAAAAAGZTZXQgdGhlIHVwZ3JhZGUgdGltZWxvY2sgZHVyYXRpb24gaW4gc2Vjb25kcyAoIzMzOCkuCk1pbmltdW06IDMsNjAwIHMgKDEgaCkuIERlZmF1bHQ6IDg2LDQwMCBzICgyNCBoKS4AAAAAABRzZXRfdXBncmFkZV90aW1lbG9jawAAAAIAAAAAAAAABWFkbWluAAAAAAAAEwAAAAAAAAAEc2VjcwAAAAYAAAAA",
        "AAAAAQAAADZTZW1hbnRpYyB2ZXJzaW9uIG9mIHRoaXMgY3JlZGl0LXNjb3JlIGNvbnRyYWN0ICgjMjM3KS4AAAAAAAAAAAASQ3JlZGl0U2NvcmVWZXJzaW9uAAAAAAADAAAAAAAAAAVtYWpvcgAAAAAAAAQAAAAAAAAABW1pbm9yAAAAAAAABAAAAAAAAAAFcGF0Y2gAAAAAAAAE",
        "AAAAAQAAAAAAAAAAAAAAElNjb3JlQXZlcmFnZUNvbmZpZwAAAAAABgAAAAAAAAAMYXZnX2RheXNfbHQzAAAABwAAAAAAAAAMYXZnX2RheXNfbHQ3AAAABwAAAAAAAAALYXZnX2x0M19wdHMAAAAABQAAAAAAAAALYXZnX2x0N19wdHMAAAAABQAAAAAAAAALYXZnX25lZ19wdHMAAAAABQAAAAAAAAARYXZnX292ZXJfbGF0ZV9wdHMAAAAAAAAF",
        "AAAAAAAAAIRGaW5hbGl6ZSBhIHByZXZpb3VzbHkgcHJvcG9zZWQgYWRtaW4ga2V5IHJvdGF0aW9uICgjNTY1KS4KT25seSBjYWxsYWJsZSBieSB0aGUgY3VycmVudCBhZG1pbiBhZnRlciB0aGUgNDgtaG91ciB0aW1lbG9jayBoYXMgZWxhcHNlZC4AAAAVZmluYWxpemVfYWRtaW5fY2hhbmdlAAAAAAAAAQAAAAAAAAAFYWRtaW4AAAAAAAATAAAAAA==",
        "AAAAAAAAAAAAAAAVbGlzdF9hY3RpdmVfYXR0ZXN0b3JzAAAAAAAAAAAAAAEAAAPqAAAH0AAAAAxBdHRlc3RvckluZm8=",
        "AAAAAAAAAAAAAAAVbGlzdF9zbWVfYXR0ZXN0YXRpb25zAAAAAAAAAQAAAAAAAAADc21lAAAAABMAAAABAAAD6gAAB9AAAAALQXR0ZXN0YXRpb24A",
        "AAAAAQAAAg9SZXR1cm5lZCBieSBgZ2V0X2NyZWRpdF9zY29yZWAuIEluY2x1ZGVzIHRoZSBjdXJyZW50IGNvbmZpZyB2ZXJzaW9uIGFsb25nc2lkZSB0aGUKc3RvcmVkIHNjb3JlIHNvIGNhbGxlcnMgY2FuIGRldGVjdCBzdGFsZW5lc3MgaW4gYSBzaW5nbGUgY2FsbCB3aXRob3V0IGEgc2VwYXJhdGUKYGdldF9zY29yaW5nX2NvbmZpZygpYCByb3VuZC10cmlwLgoKYGlzX3N0YWxlYCBpcyB0cnVlIHdoZW4gYHNjb3JlX3ZlcnNpb25gICh0aGUgY29uZmlnIHZlcnNpb24gYWN0aXZlIHdoZW4gdGhlIHNjb3JlCndhcyBsYXN0IGNvbXB1dGVkKSBkb2VzIG5vdCBtYXRjaCBgY29uZmlnX3ZlcnNpb25gICh0aGUgY29uZmlnIHZlcnNpb24gbm93IGFjdGl2ZSkuCkEgc3RhbGUgZmxhZyBtZWFucyB0aGUgc3RvcmVkIHNjb3JlIHdhcyBjb21wdXRlZCB1bmRlciBkaWZmZXJlbnQgc2NvcmluZyBwYXJhbWV0ZXJzCmFuZCBzaG91bGQgYmUgdHJlYXRlZCBhcyBhcHByb3hpbWF0ZSB1bnRpbCB0aGUgU01FJ3MgbmV4dCBwYXltZW50IGlzIHJlY29yZGVkLgAAAAAAAAAAE0NyZWRpdFNjb3JlUmVzcG9uc2UAAAAADQAAAAAAAAAUYXZlcmFnZV9wYXltZW50X2RheXMAAAAHAAAAzSM4Njg6IGBzY29yZWAgYmxlbmRlZCB3aXRoIHRoZSBTTUUncyBhY3RpdmUgZXh0ZXJuYWwgYXR0ZXN0YXRpb25zCihgU2NvcmVBdHRlc3RhdGlvbkNvbmZpZ2Agd2VpZ2h0cykuIEVxdWFsIHRvIGBzY29yZWAgd2hlbiB0aGUgU01FIGhhcwp6ZXJvIGFjdGl2ZSBhdHRlc3RhdGlvbnMg4oCUIGV4aXN0aW5nIFNNRXMgYXJlIG5ldmVyIHJlZ3Jlc3NlZCBieSB2Mi4AAAAAAAANYmxlbmRlZF9zY29yZQAAAAAAAAQAAAAwQ29uZmlnIHZlcnNpb24gY3VycmVudGx5IGFjdGl2ZSBvbiB0aGUgY29udHJhY3QuAAAADmNvbmZpZ192ZXJzaW9uAAAAAAAEAAAAAAAAAAlkZWZhdWx0ZWQAAAAAAAAEAAAAQ1RydWUgd2hlbiBgc2NvcmVfdmVyc2lvbiAhPSBjb25maWdfdmVyc2lvbmAg4oCUIHRoZSBzY29yZSBpcyBzdGFsZS4AAAAACGlzX3N0YWxlAAAAAQAAAAAAAAAMbGFzdF91cGRhdGVkAAAABgAAAAAAAAAJcGFpZF9sYXRlAAAAAAAABAAAAAAAAAAMcGFpZF9vbl90aW1lAAAABAAAAAAAAAAFc2NvcmUAAAAAAAAEAAAAQUNvbmZpZyB2ZXJzaW9uIHRoYXQgd2FzIGFjdGl2ZSB3aGVuIHRoaXMgc2NvcmUgd2FzIGxhc3QgY29tcHV0ZWQuAAAAAAAADXNjb3JlX3ZlcnNpb24AAAAAAAAEAAAAAAAAAANzbWUAAAAAEwAAAAAAAAAOdG90YWxfaW52b2ljZXMAAAAAAAQAAAAAAAAADHRvdGFsX3ZvbHVtZQAAAAs=",
        "AAAAAAAAAAAAAAAXZ2V0X21heF9wYXltZW50X2hpc3RvcnkAAAAAAAAAAAEAAAAE",
        "AAAAAAAAAAAAAAAXc2V0X21heF9wYXltZW50X2hpc3RvcnkAAAAAAgAAAAAAAAAFYWRtaW4AAAAAAAATAAAAAAAAAAttYXhfaGlzdG9yeQAAAAAEAAAAAA==",
        "AAAAAQAAAQsjODY4OiBibGVuZCByYXRpbyBiZXR3ZWVuIHRoZSBpbnRlcm5hbCAocGF5bWVudC1oaXN0b3J5LWRlcml2ZWQpIHNjb3JlIGFuZAp0aGUgZXh0ZXJuYWwtYXR0ZXN0YXRpb24tZGVyaXZlZCBjb21wb25lbnQuIFZlcnNpb25lZCBhbG9uZ3NpZGUgdGhlIHJlc3QKb2YgYFNjb3JpbmdDb25maWdgIChzZWUgYGNvcmUuc2NvcmVfdmVyc2lvbmApIHNvIGEgcmUtd2VpZ2h0aW5nIGNhbm5vdApzaWxlbnRseSByZWludGVycHJldCBwcmV2aW91c2x5LWNvbXB1dGVkIHNjb3Jlcy4AAAAAAAAAABZTY29yZUF0dGVzdGF0aW9uQ29uZmlnAAAAAAACAAAAAAAAABNleHRlcm5hbF93ZWlnaHRfYnBzAAAAAAQAAAAAAAAAE2ludGVybmFsX3dlaWdodF9icHMAAAAABA==",
        "AAAAAAAAAAAAAAAaZ2V0X3BheW1lbnRfaGlzdG9yeV9sZW5ndGgAAAAAAAEAAAAAAAAAA3NtZQAAAAATAAAAAQAAAAQ=",
        "AAAAAAAAAKBSZXNvbHZlIGEgcGVuZGluZyBhdHRlc3RhdGlvbiBkaXNwdXRlLiBBZG1pbi1vbmx5LiBJZiB0aGUgYXR0ZXN0YXRpb24KaXMgdXBoZWxkIChgdXBoZWxkID0gdHJ1ZWApIGl0IHJldHVybnMgdG8gYEFjdGl2ZWA7IG90aGVyd2lzZSBpdCBpcwpwZXJtYW5lbnRseSBgUmV2b2tlZGAuAAAAG3Jlc29sdmVfYXR0ZXN0YXRpb25fZGlzcHV0ZQAAAAADAAAAAAAAAAVhZG1pbgAAAAAAABMAAAAAAAAADmF0dGVzdGF0aW9uX2lkAAAAAAAGAAAAAAAAAAZ1cGhlbGQAAAAAAAEAAAAA",
        "AAAAAAAAAAAAAAAeZ2V0X2F0dGVzdGF0aW9uX2Rpc3B1dGVfcmVhc29uAAAAAAABAAAAAAAAAAJpZAAAAAAABgAAAAEAAAPoAAAAEA==",
        "AAAAAAAAAAAAAAAgZ2V0X3BheW1lbnRfcmVjb3JkX3Njb3JlX3ZlcnNpb24AAAABAAAAAAAAAAppbnZvaWNlX2lkAAAAAAAGAAAAAQAAA+gAAAAE",
        "AAAAAAAAAM5QcmV2aWV3IHRoZSBibGVuZGVkIHNjb3JlIGFzIGlmIGBoeXBvdGhldGljYWxgIGFkZGl0aW9uYWwgYXR0ZXN0YXRpb25zCih3ZWlnaHRfYnBzLCBzY29yZV9jb250cmlidXRpb24pIGV4aXN0ZWQsIHdpdGhvdXQgcGVyc2lzdGluZyBhbnl0aGluZy4KUG93ZXJzIGEgIndoYXQgd291bGQgbXkgc2NvcmUgYmUgaWYgSSB2ZXJpZmllZCBYIiBmcm9udGVuZCBmbG93LgAAAAAAIHNpbXVsYXRlX3Njb3JlX3dpdGhfYXR0ZXN0YXRpb25zAAAAAgAAAAAAAAADc21lAAAAABMAAAAAAAAADGh5cG90aGV0aWNhbAAAA+oAAAPtAAAAAgAAAAQAAAAEAAAAAQAAAAQ=" ]),
      options
    )
  }
  public readonly fromJSON = {
    pause: this.txFromJSON<null>,
        unpause: this.txFromJSON<null>,
        version: this.txFromJSON<CreditScoreVersion>,
        is_paused: this.txFromJSON<boolean>,
        get_config: this.txFromJSON<readonly [string, string, string]>,
        initialize: this.txFromJSON<null>,
        run_migration: this.txFromJSON<null>,
        get_score_band: this.txFromJSON<string>,
        record_default: this.txFromJSON<null>,
        record_funding: this.txFromJSON<null>,
        record_payment: this.txFromJSON<null>,
        execute_upgrade: this.txFromJSON<null>,
        get_attestation: this.txFromJSON<Option<Attestation>>,
        propose_upgrade: this.txFromJSON<null>,
        get_credit_score: this.txFromJSON<CreditScoreResponse>,
        get_attestor_info: this.txFromJSON<Option<AttestorInfo>>,
        migration_version: this.txFromJSON<u32>,
        register_attestor: this.txFromJSON<null>,
        set_pool_contract: this.txFromJSON<null>,
        get_late_threshold: this.txFromJSON<i64>,
        get_payment_record: this.txFromJSON<Option<PaymentRecord>>,
        get_scoring_config: this.txFromJSON<ScoringConfig>,
        list_all_sme_stats: this.txFromJSON<Array<SmeScoreEntry>>,
        set_late_threshold: this.txFromJSON<null>,
        set_scoring_config: this.txFromJSON<null>,
        submit_attestation: this.txFromJSON<u64>,
        cancel_admin_change: this.txFromJSON<null>,
        deactivate_attestor: this.txFromJSON<null>,
        dispute_attestation: this.txFromJSON<null>,
        get_payment_history: this.txFromJSON<Array<PaymentRecord>>,
        get_score_thresholds: this.txFromJSON<ScoreThresholds>,
        get_upgrade_timelock: this.txFromJSON<u64>,
        is_invoice_processed: this.txFromJSON<boolean>,
        propose_admin_change: this.txFromJSON<null>,
        set_invoice_contract: this.txFromJSON<null>,
        set_score_thresholds: this.txFromJSON<null>,
        set_upgrade_timelock: this.txFromJSON<null>,
        finalize_admin_change: this.txFromJSON<null>,
        list_active_attestors: this.txFromJSON<Array<AttestorInfo>>,
        list_sme_attestations: this.txFromJSON<Array<Attestation>>,
        get_max_payment_history: this.txFromJSON<u32>,
        set_max_payment_history: this.txFromJSON<null>,
        get_payment_history_length: this.txFromJSON<u32>,
        resolve_attestation_dispute: this.txFromJSON<null>,
        get_attestation_dispute_reason: this.txFromJSON<Option<string>>,
        get_payment_record_score_version: this.txFromJSON<Option<u32>>,
        simulate_score_with_attestations: this.txFromJSON<u32>
  }
}