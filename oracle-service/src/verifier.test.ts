import { describe, it, beforeEach, afterEach, mock } from "node:test";
import assert from "node:assert/strict";
import Module from "node:module";
import {
  Account,
  Keypair,
  Networks,
  Operation,
  TransactionBuilder,
} from "@stellar/stellar-sdk";
import { ConsensusTracker } from "./consensus";
import type { Verifier as VerifierType } from "./verifier";
import type { OracleConfig } from "./types";

// #1554: Verifier submits fund-affecting transactions (legacy
// `verify_invoice` or registry `open_verification_round` + `submit_vote`).
// `astera-sdk` is swapped for a stub so every on-chain call can be observed
// and scripted without an RPC endpoint (or a built SDK `dist/`).

// The stub mirrors the real registry error map so the `ContractPaused`
// detection is exercised against the same codes the SDK ships.
// eslint-disable-next-line @typescript-eslint/no-var-requires
const {
  Errors: OracleRegistryErrors,
} = require("../../packages/sdk/src/generated/oracle_registry.ts");
const PAUSED_ERROR = "Simulation failed: HostError: Error(Contract, #3)";

interface FakeClient {
  invoice: { get: any; verify: any };
  oracleRegistry: { getRound: any; openRound: any; vote: any };
}

let currentClient: FakeClient;
let lastClientConfig: Record<string, unknown> | undefined;

class FakeAsteraClient {
  constructor(config: Record<string, unknown>) {
    lastClientConfig = config;
    return currentClient as unknown as FakeAsteraClient;
  }
}

const moduleInternals = Module as unknown as {
  _load: (...args: unknown[]) => unknown;
};
const originalLoad = moduleInternals._load;
moduleInternals._load = function (
  this: unknown,
  request: unknown,
  ...rest: unknown[]
) {
  if (request === "astera-sdk") {
    return { AsteraClient: FakeAsteraClient, OracleRegistryErrors };
  }
  return originalLoad.call(this, request, ...rest);
};
// eslint-disable-next-line @typescript-eslint/no-var-requires
const { Verifier } = require("./verifier") as { Verifier: typeof VerifierType };
moduleInternals._load = originalLoad;

const ORACLE = Keypair.random();
const REGISTRY = "CREGISTRYCONTRACT";

function makeConfig(overrides: Partial<OracleConfig> = {}): OracleConfig {
  return {
    rpcUrl: "https://rpc.example.invalid",
    horizonUrl: "https://horizon.example.invalid",
    networkPassphrase: Networks.TESTNET,
    oracleSecretKey: ORACLE.secret(),
    invoiceContractId: "CINVOICECONTRACT",
    autoVerifyDelayMs: 0,
    listenerCursorPath: "/dev/null",
    ...overrides,
  };
}

function makeClient(invoice: Record<string, unknown> = {}): FakeClient {
  return {
    invoice: {
      get: mock.fn(async () => ({
        metadata_uri: "",
        verification_hash: "hash-1",
        ...invoice,
      })),
      verify: mock.fn(async () => "legacy-tx"),
    },
    oracleRegistry: {
      getRound: mock.fn(async () => null),
      openRound: mock.fn(async () => "open-tx"),
      vote: mock.fn(async () => "vote-tx"),
    },
  };
}

describe("Verifier", () => {
  let stubs: Array<ReturnType<typeof mock.method>>;

  beforeEach(() => {
    stubs = [
      // Zero jitter so `retryWithBackoff` retries immediately.
      mock.method(Math, "random", () => 0),
      mock.method(console, "log", () => {}),
      mock.method(console, "warn", () => {}),
      mock.method(console, "error", () => {}),
    ];
    currentClient = makeClient();
  });

  afterEach(() => {
    for (const stub of stubs) stub.mock.restore();
  });

  describe("legacy single-oracle mode", () => {
    it("builds the SDK client from config", () => {
      new Verifier(makeConfig());
      assert.equal(lastClientConfig?.invoiceContractId, "CINVOICECONTRACT");
      assert.equal(lastClientConfig?.network, Networks.TESTNET);
      assert.equal(lastClientConfig?.oracleRegistryContractId, undefined);
    });

    it("fetches the invoice and submits an approving verify_invoice", async () => {
      const verifier = new Verifier(makeConfig());
      await verifier.verifyInvoice(42n);

      assert.equal(currentClient.invoice.get.mock.callCount(), 1);
      assert.equal(currentClient.invoice.get.mock.calls[0].arguments[0], 42n);
      assert.equal(currentClient.invoice.verify.mock.callCount(), 1);
      const params = currentClient.invoice.verify.mock.calls[0].arguments[0];
      assert.equal(params.id, 42n);
      assert.equal(params.approved, true);
      assert.equal(params.oracle, ORACLE.publicKey());
      assert.equal(params.oracleHash, "hash-1");
      assert.equal(typeof params.reason, "string");
      // Never touches the registry in legacy mode.
      assert.equal(currentClient.oracleRegistry.vote.mock.callCount(), 0);
      assert.equal(currentClient.oracleRegistry.openRound.mock.callCount(), 0);
    });

    it("passes a signer that signs with the oracle key on the configured network", async () => {
      const verifier = new Verifier(makeConfig());
      await verifier.verifyInvoice(1n);
      const { signer } =
        currentClient.invoice.verify.mock.calls[0].arguments[0];

      const tx = new TransactionBuilder(new Account(ORACLE.publicKey(), "1"), {
        fee: "100",
        networkPassphrase: Networks.TESTNET,
      })
        .addOperation(Operation.bumpSequence({ bumpTo: "2" }))
        .setTimeout(30)
        .build();

      const signed = TransactionBuilder.fromXDR(
        await signer(tx.toXDR()),
        Networks.TESTNET,
      );
      assert.equal(signed.signatures.length, 1);
      assert.equal(
        ORACLE.verify(signed.hash(), signed.signatures[0].signature()),
        true,
      );
    });

    it("verifies when the invoice has a metadata document", async () => {
      currentClient = makeClient({ metadata_uri: "ipfs://doc" });
      const verifier = new Verifier(makeConfig());
      await verifier.verifyInvoice(5n);
      assert.equal(
        currentClient.invoice.verify.mock.calls[0].arguments[0].approved,
        true,
      );
    });

    it("defaults a missing verification hash to an empty string", async () => {
      currentClient = makeClient({ verification_hash: undefined });
      const verifier = new Verifier(makeConfig());
      await verifier.verifyInvoice(6n);
      assert.equal(
        currentClient.invoice.verify.mock.calls[0].arguments[0].oracleHash,
        "",
      );
    });

    it("retries a transient invoice.get failure before verifying", async () => {
      let calls = 0;
      currentClient.invoice.get = mock.fn(async () => {
        calls += 1;
        if (calls === 1) throw new Error("socket hang up");
        return { metadata_uri: "", verification_hash: "hash-1" };
      });
      const verifier = new Verifier(makeConfig());
      await verifier.verifyInvoice(7n);
      assert.equal(calls, 2);
      assert.equal(currentClient.invoice.verify.mock.callCount(), 1);
    });

    it("submits nothing when the invoice cannot be fetched", async () => {
      currentClient.invoice.get = mock.fn(async () => {
        throw new Error("Simulation failed: InvoiceNotFound");
      });
      const verifier = new Verifier(makeConfig());
      await assert.doesNotReject(verifier.verifyInvoice(8n));
      // Non-retriable: exactly one attempt, then give up without submitting.
      assert.equal(currentClient.invoice.get.mock.callCount(), 1);
      assert.equal(currentClient.invoice.verify.mock.callCount(), 0);
    });

    it("does not reject when verify_invoice itself fails", async () => {
      currentClient.invoice.verify = mock.fn(async () => {
        throw new Error("Transaction failed on-chain");
      });
      const verifier = new Verifier(makeConfig());
      await assert.doesNotReject(verifier.verifyInvoice(9n));
      assert.equal(currentClient.invoice.verify.mock.callCount(), 1);
    });
  });

  describe("consensus mode", () => {
    const config = () => makeConfig({ oracleRegistryContractId: REGISTRY });

    it("opens a round when none exists, then votes", async () => {
      const verifier = new Verifier(config());
      await verifier.verifyInvoice(10n);

      assert.equal(currentClient.oracleRegistry.openRound.mock.callCount(), 1);
      const open =
        currentClient.oracleRegistry.openRound.mock.calls[0].arguments[0];
      assert.equal(open.invoiceId, 10n);
      assert.equal(open.caller, ORACLE.publicKey());
      assert.equal(open.oracleHash, "hash-1");

      assert.equal(currentClient.oracleRegistry.vote.mock.callCount(), 1);
      const vote = currentClient.oracleRegistry.vote.mock.calls[0].arguments[0];
      assert.equal(vote.invoiceId, 10n);
      assert.equal(vote.oracle, ORACLE.publicKey());
      assert.equal(vote.approved, true);
      assert.equal(vote.evidenceHash, "hash-1");
      assert.equal(typeof vote.signer, "function");

      // Never calls the legacy direct-verify path.
      assert.equal(currentClient.invoice.verify.mock.callCount(), 0);
    });

    it("skips opening when the round is already open", async () => {
      currentClient.oracleRegistry.getRound = mock.fn(async () => ({
        status: "Open",
      }));
      const verifier = new Verifier(config());
      await verifier.verifyInvoice(11n);
      assert.equal(currentClient.oracleRegistry.openRound.mock.callCount(), 0);
      assert.equal(currentClient.oracleRegistry.vote.mock.callCount(), 1);
    });

    it("tries to open a round when getRound fails or the round is not open", async () => {
      currentClient.oracleRegistry.getRound = mock.fn(async () => {
        throw new Error("RoundNotFound");
      });
      await new Verifier(config()).verifyInvoice(12n);
      assert.equal(currentClient.oracleRegistry.openRound.mock.callCount(), 1);

      currentClient = makeClient();
      currentClient.oracleRegistry.getRound = mock.fn(async () => ({
        status: "Expired",
      }));
      await new Verifier(config()).verifyInvoice(13n);
      assert.equal(currentClient.oracleRegistry.openRound.mock.callCount(), 1);
    });

    it("still votes when opening fails because another oracle already opened it", async () => {
      currentClient.oracleRegistry.openRound = mock.fn(async () => {
        throw new Error("Simulation failed: HostError: Error(Contract, #12)");
      });
      const verifier = new Verifier(config());
      await verifier.verifyInvoice(14n);
      assert.equal(currentClient.oracleRegistry.vote.mock.callCount(), 1);
    });

    it("does not call the registry at all while the tracker reports it paused", async () => {
      const tracker = new ConsensusTracker(ORACLE.publicKey());
      tracker.markPaused();
      const verifier = new Verifier(config(), tracker);
      await verifier.verifyInvoice(15n);
      assert.equal(currentClient.oracleRegistry.getRound.mock.callCount(), 0);
      assert.equal(currentClient.oracleRegistry.openRound.mock.callCount(), 0);
      assert.equal(currentClient.oracleRegistry.vote.mock.callCount(), 0);
    });

    it("marks the tracker paused and skips the vote when openRound hits ContractPaused", async () => {
      currentClient.oracleRegistry.openRound = mock.fn(async () => {
        throw new Error(PAUSED_ERROR);
      });
      const tracker = new ConsensusTracker(ORACLE.publicKey());
      const verifier = new Verifier(config(), tracker);
      await verifier.verifyInvoice(16n);

      assert.equal(tracker.isPaused(), true);
      assert.equal(currentClient.oracleRegistry.vote.mock.callCount(), 0);
    });

    it("marks the tracker paused when the vote hits ContractPaused", async () => {
      currentClient.oracleRegistry.getRound = mock.fn(async () => ({
        status: "Open",
      }));
      currentClient.oracleRegistry.vote = mock.fn(async () => {
        throw new Error(PAUSED_ERROR);
      });
      const tracker = new ConsensusTracker(ORACLE.publicKey());
      const verifier = new Verifier(config(), tracker);
      await assert.doesNotReject(verifier.verifyInvoice(17n));

      assert.equal(tracker.isPaused(), true);
      // Paused failures are non-retriable ("Simulation failed"), so one attempt.
      assert.equal(currentClient.oracleRegistry.vote.mock.callCount(), 1);

      // Subsequent invoices are not submitted until an `unpaused` event.
      await verifier.verifyInvoice(18n);
      assert.equal(currentClient.oracleRegistry.vote.mock.callCount(), 1);
      tracker.handleEvent("unpaused", null);
      await verifier.verifyInvoice(19n);
      assert.equal(currentClient.oracleRegistry.vote.mock.callCount(), 2);
    });

    it("does not treat other contract errors as a pause", async () => {
      currentClient.oracleRegistry.getRound = mock.fn(async () => ({
        status: "Open",
      }));
      currentClient.oracleRegistry.vote = mock.fn(async () => {
        throw new Error("Simulation failed: HostError: Error(Contract, #7)");
      });
      const tracker = new ConsensusTracker(ORACLE.publicKey());
      const verifier = new Verifier(config(), tracker);
      await assert.doesNotReject(verifier.verifyInvoice(20n));
      assert.equal(tracker.isPaused(), false);
    });
  });

  describe("verification queue", () => {
    it("runs at most 3 verifications concurrently and eventually processes all", async () => {
      let inFlight = 0;
      let maxInFlight = 0;
      const releases: Array<() => void> = [];
      currentClient.invoice.get = mock.fn(async () => {
        inFlight += 1;
        maxInFlight = Math.max(maxInFlight, inFlight);
        await new Promise<void>((resolve) => releases.push(resolve));
        inFlight -= 1;
        return { metadata_uri: "", verification_hash: "h" };
      });
      const verifier = new Verifier(makeConfig());

      const pending = [1n, 2n, 3n, 4n, 5n].map((id) =>
        verifier.verifyInvoice(id),
      );
      await new Promise((resolve) => setImmediate(resolve));
      assert.equal(currentClient.invoice.get.mock.callCount(), 3);

      // Release jobs one at a time until all five have been fetched.
      for (
        let i = 0;
        i < 50 && currentClient.invoice.verify.mock.callCount() < 5;
        i++
      ) {
        releases.shift()?.();
        await new Promise((resolve) => setTimeout(resolve, 5));
      }
      await Promise.all(pending);

      assert.equal(maxInFlight, 3);
      assert.equal(currentClient.invoice.get.mock.callCount(), 5);
      const verified = currentClient.invoice.verify.mock.calls
        .map((c: any) => c.arguments[0].id)
        .sort();
      assert.deepEqual(verified, [1n, 2n, 3n, 4n, 5n]);
    });
  });
});
