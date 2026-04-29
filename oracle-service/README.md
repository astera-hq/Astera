# Astera Oracle Integration Service

This service provides a push-based notification system for invoice verification. It listens for `INVOICE:created` events on the Stellar network and automatically triggers a verification flow.

## Features
- **Real-time Event Listening**: Uses Stellar Horizon's streaming API to detect new invoices instantly.
- **Auto-Verification**: Mock verifier that automatically approves invoices after a configurable delay (for development).
- **Pluggable Logic**: Easily replace the mock verifier with real document verification logic (e.g., OCR, third-party API).

## Setup

1. **Install Dependencies**:
   ```bash
   npm install
   ```

2. **Configure Environment**:
   Copy `.env.example` to `.env` and fill in the required values:
   - `ORACLE_SECRET_KEY`: The private key of the authorized oracle account.
   - `INVOICE_CONTRACT_ID`: The ID of the deployed invoice contract.

3. **Run the Service**:
   ```bash
   npm run dev
   ```

## Integration

### Plugging in Real Verification
To implement real verification logic, modify `src/verifier.ts`. You can integrate with external services to verify the document hash or content before calling `this.client.invoice.verify`.

### Local Development with Docker
This service is integrated into the root `docker-compose.yml`. Running `docker-compose up` will start the oracle service along with the rest of the Astera ecosystem.
