import './globals.css';
import type { ReactNode } from 'react';
import { TooltipProvider } from '@/components/ui/tooltip';
import ShellNav from '@/components/shell-nav';

type RootLayoutProps = {
  children: ReactNode;
};

export const metadata = {
  title: 'agentd shell',
  description: 'Agent management console for agentd daemon',
};

export default function RootLayout({ children }: RootLayoutProps) {
  return (
    <html lang="en" className="dark">
      <body className="min-h-screen antialiased">
        <TooltipProvider>
          <div className="mx-auto max-w-[1200px] p-5">
            <ShellNav />
            <main className="mt-4">{children}</main>
          </div>
        </TooltipProvider>
      </body>
    </html>
  );
}
