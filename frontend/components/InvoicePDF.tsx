import { Document, Page, Text, View, Image, StyleSheet, pdf } from '@react-pdf/renderer';
import QRCode from 'qrcode';
import type { Invoice, InvoiceMetadata } from '@/lib/types';
import { formatAmount, formatDate, truncateAddress } from '@/lib/stellar';

const styles = StyleSheet.create({
  page: {
    padding: 40,
    fontSize: 10,
    fontFamily: 'Helvetica',
    color: '#1a1a1a',
  },
  header: {
    flexDirection: 'row',
    alignItems: 'center',
    borderBottom: '2px solid #d4af37',
    paddingBottom: 16,
    marginBottom: 20,
  },
  logo: {
    width: 40,
    height: 40,
    marginRight: 12,
  },
  title: {
    fontSize: 20,
    fontWeight: 700,
  },
  subtitle: {
    fontSize: 11,
    color: '#666666',
    marginTop: 2,
  },
  section: {
    marginBottom: 16,
  },
  sectionTitle: {
    fontSize: 9,
    textTransform: 'uppercase',
    letterSpacing: 0.5,
    color: '#888888',
    marginBottom: 4,
  },
  row: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    marginBottom: 6,
  },
  rowLabel: {
    color: '#666666',
  },
  rowValue: {
    fontWeight: 700,
  },
  mono: {
    fontFamily: 'Courier',
    fontSize: 9,
  },
  muted: {
    color: '#888888',
    fontSize: 9,
  },
  amountBox: {
    backgroundColor: '#faf6ec',
    borderRadius: 6,
    padding: 16,
    marginBottom: 20,
    alignItems: 'center',
  },
  amountLabel: {
    fontSize: 9,
    color: '#888888',
    marginBottom: 4,
  },
  amount: {
    fontSize: 28,
    fontWeight: 700,
    color: '#b8860b',
  },
  footer: {
    flexDirection: 'row',
    marginTop: 24,
    paddingTop: 16,
    borderTop: '1px solid #dddddd',
    alignItems: 'flex-start',
  },
  qr: {
    width: 72,
    height: 72,
    marginRight: 16,
  },
  proof: {
    flex: 1,
  },
});

function Row({ label, value }: { label: string; value: string }) {
  return (
    <View style={styles.row}>
      <Text style={styles.rowLabel}>{label}</Text>
      <Text style={styles.rowValue}>{value}</Text>
    </View>
  );
}

interface InvoicePDFDocumentProps {
  invoice: Invoice;
  metadata: InvoiceMetadata;
  qrCodeDataUrl: string;
  logoDataUrl?: string;
}

export function InvoicePDFDocument({
  invoice,
  metadata,
  qrCodeDataUrl,
  logoDataUrl,
}: InvoicePDFDocumentProps) {
  return (
    <Document>
      <Page size="A4" style={styles.page}>
        <View style={styles.header}>
          {/* eslint-disable-next-line jsx-a11y/alt-text -- react-pdf Image, not an HTML img */}
          {logoDataUrl ? <Image src={logoDataUrl} style={styles.logo} /> : null}
          <View>
            <Text style={styles.title}>Astera Invoice #{invoice.id}</Text>
            <Text style={styles.subtitle}>{metadata.name || 'Invoice'}</Text>
          </View>
        </View>

        <View style={styles.section}>
          <Row label="Invoice Number" value={`#${invoice.id}`} />
          <Row label="Created" value={formatDate(invoice.createdAt)} />
          <Row label="Due Date" value={formatDate(metadata.dueDate)} />
          <Row label="Status" value={metadata.status} />
        </View>

        <View style={styles.section}>
          <Text style={styles.sectionTitle}>Borrower</Text>
          <Text style={styles.mono}>{invoice.owner}</Text>
        </View>

        <View style={styles.section}>
          <Text style={styles.sectionTitle}>Buyer / Debtor</Text>
          <Text>{metadata.debtor || '—'}</Text>
        </View>

        <View style={styles.amountBox}>
          <Text style={styles.amountLabel}>Amount Due</Text>
          <Text style={styles.amount}>{formatAmount(metadata.amount, metadata.decimals)}</Text>
        </View>

        {metadata.description ? (
          <View style={styles.section}>
            <Text style={styles.sectionTitle}>Description</Text>
            <Text>{metadata.description}</Text>
          </View>
        ) : null}

        <View style={styles.section}>
          <Text style={styles.sectionTitle}>Payment Terms</Text>
          <Text>
            Payment of {formatAmount(metadata.amount, metadata.decimals)} is due in full by{' '}
            {formatDate(metadata.dueDate)}. This invoice is tokenized on the Stellar network and
            factored through the Astera lending pool — repayment is made in the invoice&apos;s
            accepted token directly on-chain to contract {truncateAddress(invoice.poolContract)}.
          </Text>
        </View>

        <View style={styles.footer}>
          {/* eslint-disable-next-line jsx-a11y/alt-text -- react-pdf Image, not an HTML img */}
          <Image src={qrCodeDataUrl} style={styles.qr} />
          <View style={styles.proof}>
            <Text style={styles.sectionTitle}>On-Chain Verification</Text>
            {invoice.verificationHash ? (
              <Text style={styles.mono}>{invoice.verificationHash}</Text>
            ) : (
              <Text style={styles.muted}>No verification hash recorded on-chain.</Text>
            )}
            <Text style={{ ...styles.muted, marginTop: 4 }}>
              Scan the QR code to view and verify this invoice on-chain.
            </Text>
          </View>
        </View>
      </Page>
    </Document>
  );
}

async function toDataUrl(url: string): Promise<string | undefined> {
  try {
    const response = await fetch(url);
    if (!response.ok) return undefined;
    const blob = await response.blob();
    return await new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onloadend = () => resolve(reader.result as string);
      reader.onerror = reject;
      reader.readAsDataURL(blob);
    });
  } catch {
    return undefined;
  }
}

/**
 * Renders the invoice as a PDF blob and triggers a browser download, named
 * `astera-invoice-{id}-{date}.pdf` per the invoice PDF export spec.
 */
export async function downloadInvoicePDF(invoice: Invoice, metadata: InvoiceMetadata) {
  const invoiceUrl =
    typeof window !== 'undefined' ? `${window.location.origin}/invoice/${invoice.id}` : '';

  const [qrCodeDataUrl, logoDataUrl] = await Promise.all([
    QRCode.toDataURL(invoiceUrl, { margin: 1, width: 200 }),
    toDataUrl('/icons/icon-512.png'),
  ]);

  const blob = await pdf(
    <InvoicePDFDocument
      invoice={invoice}
      metadata={metadata}
      qrCodeDataUrl={qrCodeDataUrl}
      logoDataUrl={logoDataUrl}
    />,
  ).toBlob();

  const dateStr = new Date().toISOString().slice(0, 10);
  const filename = `astera-invoice-${invoice.id}-${dateStr}.pdf`;

  const blobUrl = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = blobUrl;
  link.download = filename;
  document.body.appendChild(link);
  link.click();
  document.body.removeChild(link);
  URL.revokeObjectURL(blobUrl);
}
