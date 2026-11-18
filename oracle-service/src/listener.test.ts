import { describe, it, beforeEach, afterEach, mock } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { Address, Keypair, nativeToScVal, xdr } from "@stellar/stellar-sdk";
import { Listener } from "./listener";
import type { OracleConfig } from "./types";

// #1554: Listener parses live Horizon effects and decides which ones trigger
// a fund-affecting verification. These tests drive `handleEffect` (and the
// stream wiring in `start`) with real base64 ScVal XDR, without a network.

const INVOICE_CONTRACT = "CINVOICECONTRACT";
const REGISTRY_CONTRACT = "CREGISTRYCONTRACT";
const OWNER = Keypair.random().publicKey();

function symbol(name: string): string {
  return xdr.ScVal.scvSymbol(name).toXDR("base64");
}

function createdPayload(id: bigint): string {
  return xdr.ScVal.scvVec([
    nativeToScVal(id, { type: "u64" }),
    new Address(OWNER).toScVal(),
    nativeToScVal(1_000_000n, { type: "i128" }),
    nativeToScVal("ipfs://doc", { type: "string" }),
  ]).toXDR("base64");
}

function effect(fields: Record<string, unknown>): Record<string, unknown> {
  return { type: "contract_event", contract_id: INVOICE_CONTRACT, ...fields };
}

interface Harness {
  listener: Listener;
  verifyInvoice: ReturnType<typeof mock.fn>;
  consensusEvent: ReturnType<typeof mock.fn>;
  stakingEvent: ReturnType<typeof mock.fn>;
  handle: (e: Record<string, unknown>) => void;
}

function makeHarness(overrides: Partial<OracleConfig> = {}): Harness {
  const config: OracleConfig = {
    rpcUrl: "https://rpc.example.invalid",
    horizonUrl: "https://horizon.example.invalid",
    networkPassphrase: "Test SDF Network ; September 2015",
    oracleSecretKey: Keypair.random().secret(),
    invoiceContractId: INVOICE_CONTRACT,
    autoVerifyDelayMs: 0,
    listenerCursorPath: path.join(os.tmpdir(), "unused-cursor"),
    ...overrides,
  };
  const verifyInvoice = mock.fn(async (_id: bigint) => {});
  const consensusEvent = mock.fn((_topic: string, _value: unknown) => {});
  const stakingEvent = mock.fn(
    (_topic: string, _value: unknown, _at?: string) => {},
  );
  const listener = new Listener(
    config,
    { verifyInvoice } as any,
    { handleEvent: consensusEvent } as any,
    { handleEvent: stakingEvent } as any,
  );
  return {
    listener,
    verifyInvoice,
    consensusEvent,
    stakingEvent,
    handle: (e) => (listener as any).handleEffect(e),
  };
}

describe("Listener.handleEffect", () => {
  let logStub: ReturnType<typeof mock.method>;
  let errorStub: ReturnType<typeof mock.method>;

  beforeEach(() => {
    logStub = mock.method(console, "log", () => {});
    errorStub = mock.method(console, "error", () => {});
  });

  afterEach(() => {
    logStub.mock.restore();
    errorStub.mock.restore();
  });

  it("queues verification for an INVOICE/created event with the decoded invoice id", () => {
    const h = makeHarness();
    h.handle(
      effect({
        topic: [symbol("INVOICE"), symbol("created")],
        value: createdPayload(42n),
      }),
    );

    assert.equal(h.verifyInvoice.mock.callCount(), 1);
    assert.equal(h.verifyInvoice.mock.calls[0].arguments[0], 42n);
    assert.equal(h.listener.processedCount, 1);
  });

  it('accepts the legacy "contract" effect type too', () => {
    const h = makeHarness();
    h.handle(
      effect({
        type: "contract",
        topic: [symbol("INVOICE"), symbol("created")],
        value: createdPayload(7n),
      }),
    );
    assert.equal(h.verifyInvoice.mock.callCount(), 1);
    assert.equal(h.verifyInvoice.mock.calls[0].arguments[0], 7n);
  });

  it("ignores non-contract effects entirely", () => {
    const h = makeHarness();
    h.handle({
      type: "account_credited",
      contract_id: INVOICE_CONTRACT,
      topic: [symbol("INVOICE"), symbol("created")],
      value: createdPayload(1n),
    });
    assert.equal(h.verifyInvoice.mock.callCount(), 0);
    assert.equal(h.listener.processedCount, 0);
  });

  it("ignores events from unrelated contracts", () => {
    const h = makeHarness();
    h.handle(
      effect({
        contract_id: "CSOMEOTHERCONTRACT",
        topic: [symbol("INVOICE"), symbol("created")],
        value: createdPayload(1n),
      }),
    );
    assert.equal(h.verifyInvoice.mock.callCount(), 0);
    assert.equal(h.listener.processedCount, 0);
  });

  it("does not treat the registry contract as a source when none is configured", () => {
    const h = makeHarness();
    h.handle(
      effect({
        contract_id: REGISTRY_CONTRACT,
        topic: [symbol("ORACLE"), symbol("rnd_open")],
        value: xdr.ScVal.scvVec([nativeToScVal(1n, { type: "u64" })]).toXDR(
          "base64",
        ),
      }),
    );
    assert.equal(h.consensusEvent.mock.callCount(), 0);
  });

  it("ignores other INVOICE events (only `created` triggers verification)", () => {
    const h = makeHarness();
    h.handle(
      effect({
        topic: [symbol("INVOICE"), symbol("funded")],
        value: createdPayload(3n),
      }),
    );
    assert.equal(h.verifyInvoice.mock.callCount(), 0);
    // Still counted as a processed event for the health endpoint.
    assert.equal(h.listener.processedCount, 1);
  });

  it("ignores a created event whose payload is not a tuple", () => {
    const h = makeHarness();
    h.handle(
      effect({
        topic: [symbol("INVOICE"), symbol("created")],
        value: nativeToScVal(5n, { type: "u64" }).toXDR("base64"),
      }),
    );
    assert.equal(h.verifyInvoice.mock.callCount(), 0);
  });

  it("ignores effects with missing or too-short topics", () => {
    const h = makeHarness();
    h.handle(effect({ value: createdPayload(1n) }));
    h.handle(effect({ topic: "not-an-array", value: createdPayload(1n) }));
    h.handle(effect({ topic: [symbol("INVOICE")], value: createdPayload(1n) }));
    assert.equal(h.verifyInvoice.mock.callCount(), 0);
  });

  it("does not throw on malformed topic or value XDR", () => {
    const h = makeHarness();
    assert.doesNotThrow(() =>
      h.handle(effect({ topic: ["%%%not-xdr%%%", "!!!"], value: "garbage" })),
    );
    assert.doesNotThrow(() =>
      h.handle(
        effect({
          topic: [symbol("INVOICE"), symbol("created")],
          value: "garbage",
        }),
      ),
    );
    assert.equal(h.verifyInvoice.mock.callCount(), 0);
  });

  it("logs and swallows an error from a created payload with a non-integer id", () => {
    const h = makeHarness();
    const badPayload = xdr.ScVal.scvVec([
      nativeToScVal("not-a-number", { type: "string" }),
    ]).toXDR("base64");
    assert.doesNotThrow(() =>
      h.handle(
        effect({
          topic: [symbol("INVOICE"), symbol("created")],
          value: badPayload,
        }),
      ),
    );
    assert.equal(h.verifyInvoice.mock.callCount(), 0);
    assert.equal(errorStub.mock.callCount(), 1);
  });

  it("forwards ORACLE events from the registry to the consensus and staking trackers", () => {
    const h = makeHarness({ oracleRegistryContractId: REGISTRY_CONTRACT });
    const value = xdr.ScVal.scvVec([nativeToScVal(9n, { type: "u64" })]).toXDR(
      "base64",
    );
    h.handle(
      effect({
        contract_id: REGISTRY_CONTRACT,
        topic: [symbol("ORACLE"), symbol("rnd_open")],
        value,
        created_at: "2026-01-01T00:00:00Z",
      }),
    );

    assert.equal(h.consensusEvent.mock.callCount(), 1);
    assert.deepEqual(h.consensusEvent.mock.calls[0].arguments, [
      "rnd_open",
      [9n],
    ]);
    assert.equal(h.stakingEvent.mock.callCount(), 1);
    assert.deepEqual(h.stakingEvent.mock.calls[0].arguments, [
      "rnd_open",
      [9n],
      "2026-01-01T00:00:00Z",
    ]);
    assert.equal(h.verifyInvoice.mock.callCount(), 0);
  });

  it("still handles invoice events when a registry is configured", () => {
    const h = makeHarness({ oracleRegistryContractId: REGISTRY_CONTRACT });
    h.handle(
      effect({
        topic: [symbol("INVOICE"), symbol("created")],
        value: createdPayload(11n),
      }),
    );
    assert.equal(h.verifyInvoice.mock.callCount(), 1);
    assert.equal(h.consensusEvent.mock.callCount(), 0);
  });

  it("processes effects without a contract_id (filter only applies when present)", () => {
    const h = makeHarness();
    h.handle({
      type: "contract_event",
      topic: [symbol("INVOICE"), symbol("created")],
      value: createdPayload(5n),
    });
    assert.equal(h.verifyInvoice.mock.callCount(), 1);
  });
});

describe("Listener cursor persistence", () => {
  let dir: string;
  let logStub: ReturnType<typeof mock.method>;
  let errorStub: ReturnType<typeof mock.method>;

  beforeEach(() => {
    dir = fs.mkdtempSync(path.join(os.tmpdir(), "oracle-listener-"));
    logStub = mock.method(console, "log", () => {});
    errorStub = mock.method(console, "error", () => {});
  });

  afterEach(() => {
    fs.rmSync(dir, { recursive: true, force: true });
    logStub.mock.restore();
    errorStub.mock.restore();
  });

  function stubStream(listener: Listener) {
    const cursor = mock.fn((_c: string) => builder);
    const stream = mock.fn(
      (_handlers: Record<string, (arg: any) => void>) => () => {},
    );
    const builder = { cursor, stream };
    (listener as any).horizon = { effects: () => builder };
    return { cursor, stream };
  }

  it("starts from 'now' when no cursor file exists", async () => {
    const h = makeHarness({ listenerCursorPath: path.join(dir, "cursor") });
    const { cursor } = stubStream(h.listener);
    await h.listener.start();
    assert.equal(cursor.mock.calls[0].arguments[0], "now");
  });

  it("starts from 'now' when the cursor file is empty", async () => {
    const cursorPath = path.join(dir, "cursor");
    fs.writeFileSync(cursorPath, "  \n");
    const h = makeHarness({ listenerCursorPath: cursorPath });
    const { cursor } = stubStream(h.listener);
    await h.listener.start();
    assert.equal(cursor.mock.calls[0].arguments[0], "now");
  });

  it("resumes from a previously saved cursor", async () => {
    const cursorPath = path.join(dir, "cursor");
    fs.writeFileSync(cursorPath, "123456789-1\n");
    const h = makeHarness({ listenerCursorPath: cursorPath });
    const { cursor } = stubStream(h.listener);
    await h.listener.start();
    assert.equal(cursor.mock.calls[0].arguments[0], "123456789-1");
  });

  it("handles streamed effects and persists their paging token", async () => {
    const cursorPath = path.join(dir, "cursor");
    const h = makeHarness({ listenerCursorPath: cursorPath });
    const { stream } = stubStream(h.listener);
    await h.listener.start();

    const { onmessage } = stream.mock.calls[0].arguments[0];
    onmessage(
      effect({
        topic: [symbol("INVOICE"), symbol("created")],
        value: createdPayload(77n),
        paging_token: "555-2",
      }),
    );

    assert.equal(h.verifyInvoice.mock.calls[0].arguments[0], 77n);
    assert.equal(fs.readFileSync(cursorPath, "utf-8"), "555-2");

    // Effects the listener ignores still advance the cursor, so a restart
    // does not re-scan them.
    onmessage({ type: "account_credited", paging_token: "556-1" });
    assert.equal(fs.readFileSync(cursorPath, "utf-8"), "556-1");
  });

  it("does not overwrite the cursor for effects without a paging token", async () => {
    const cursorPath = path.join(dir, "cursor");
    fs.writeFileSync(cursorPath, "100-1");
    const h = makeHarness({ listenerCursorPath: cursorPath });
    const { stream } = stubStream(h.listener);
    await h.listener.start();

    stream.mock.calls[0].arguments[0].onmessage({ type: "account_credited" });
    assert.equal(fs.readFileSync(cursorPath, "utf-8"), "100-1");
  });

  it("logs instead of crashing when the cursor cannot be written", async () => {
    const h = makeHarness({
      listenerCursorPath: path.join(dir, "missing-dir", "cursor"),
    });
    const { stream } = stubStream(h.listener);
    await h.listener.start();

    assert.doesNotThrow(() =>
      stream.mock.calls[0].arguments[0].onmessage({
        type: "other",
        paging_token: "1-1",
      }),
    );
    assert.equal(errorStub.mock.callCount(), 1);
  });

  it("logs stream errors without throwing", async () => {
    const h = makeHarness({ listenerCursorPath: path.join(dir, "cursor") });
    const { stream } = stubStream(h.listener);
    await h.listener.start();

    assert.doesNotThrow(() =>
      stream.mock.calls[0].arguments[0].onerror(new Error("connection reset")),
    );
    assert.equal(errorStub.mock.callCount(), 1);
  });
});
