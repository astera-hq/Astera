import { ImageResponse } from 'next/og';

// #765: shared renderer for the OpenGraph/Twitter Card images. Kept in one
// place so the global default (app/opengraph-image.tsx equivalent — see
// the static /public/og-image.png) and per-invoice dynamic images
// (app/invoice/[id]/opengraph-image.tsx) stay visually consistent.

export const OG_IMAGE_SIZE = { width: 1200, height: 630 };

const COLORS = {
  gradientFrom: '#0A0E1A',
  gradientTo: '#0D1526',
  gold: '#F5A623',
  amber: '#E8920A',
  muted: '#6B7A99',
};

export function renderBrandOgImage(title: string, subtitle: string) {
  return new ImageResponse(
    <div
      style={{
        width: '100%',
        height: '100%',
        display: 'flex',
        flexDirection: 'column',
        justifyContent: 'center',
        padding: '80px',
        background: `linear-gradient(135deg, ${COLORS.gradientFrom} 0%, ${COLORS.gradientTo} 100%)`,
      }}
    >
      <div style={{ display: 'flex', alignItems: 'center', gap: 16, marginBottom: 48 }}>
        <div
          style={{
            display: 'flex',
            width: 56,
            height: 56,
            borderRadius: 14,
            background: `linear-gradient(135deg, ${COLORS.gold} 0%, ${COLORS.amber} 100%)`,
          }}
        />
        <span style={{ fontSize: 36, fontWeight: 700, color: '#FFFFFF' }}>Astera</span>
      </div>
      <div
        style={{
          display: 'flex',
          fontSize: 60,
          fontWeight: 700,
          color: '#FFFFFF',
          maxWidth: 980,
          lineHeight: 1.15,
        }}
      >
        {title}
      </div>
      <div
        style={{
          display: 'flex',
          fontSize: 30,
          color: COLORS.muted,
          marginTop: 24,
          maxWidth: 980,
        }}
      >
        {subtitle}
      </div>
    </div>,
    { ...OG_IMAGE_SIZE },
  );
}
