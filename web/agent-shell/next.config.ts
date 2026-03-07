import type { NextConfig } from 'next';

const nextConfig: NextConfig = {
  reactStrictMode: true,
  transpilePackages: [
    'streamdown',
    'mermaid',
    'tailwind-merge',
    '@streamdown/cjk',
    '@streamdown/code',
    '@streamdown/math',
    '@streamdown/mermaid',
  ],
};

export default nextConfig;
