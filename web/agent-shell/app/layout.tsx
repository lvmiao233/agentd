import './globals.css';
import type { ReactNode } from 'react';
import ShellNav from '@/components/shell-nav';

type RootLayoutProps = {
  children: ReactNode;
};

export default function RootLayout({ children }: RootLayoutProps) {
  return (
    <html lang="en">
      <body>
        <div className="shell-root">
          <ShellNav />
          <div className="shell-content">{children}</div>
        </div>
      </body>
    </html>
  );
}
