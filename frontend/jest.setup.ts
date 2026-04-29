import '@testing-library/jest-dom';

// jsdom lacks TextEncoder/TextDecoder which some Stellar SDK internals (and some Next.js
// internals) require at module-evaluation time. Patch globals before any other imports.
// (Node's util types don't perfectly line up with the DOM lib types — we
// assign as `unknown` to skip the structural mismatch.)
import { TextDecoder, TextEncoder } from 'util';

const mg = globalThis as unknown as { TextEncoder?: unknown; TextDecoder?: unknown };
if (typeof mg.TextEncoder === 'undefined') mg.TextEncoder = TextEncoder;
if (typeof mg.TextDecoder === 'undefined') mg.TextDecoder = TextDecoder;
