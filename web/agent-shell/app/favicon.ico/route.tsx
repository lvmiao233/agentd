import { ImageResponse } from 'next/og';

export const runtime = 'nodejs';

export async function GET() {
  const image = new ImageResponse(
    (
      <div
        style={{
          height: '100%',
          width: '100%',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          background: '#020617',
          color: '#38bdf8',
          fontSize: 44,
          fontWeight: 700,
          borderRadius: 12,
        }}
      >
        a
      </div>
    ),
    {
      width: 64,
      height: 64,
    },
  );

  return new Response(await image.arrayBuffer(), {
    headers: {
      'Content-Type': 'image/png',
      'Cache-Control': 'public, max-age=31536000, immutable',
    },
  });
}
