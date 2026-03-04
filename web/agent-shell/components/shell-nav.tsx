'use client';

import Link from 'next/link';
import { usePathname } from 'next/navigation';

const NAV_ITEMS = [
  { href: '/chat', label: 'Chat' },
  { href: '/dashboard', label: 'Dashboard' },
  { href: '/events', label: 'Events' },
  { href: '/usage', label: 'Usage' },
  { href: '/settings', label: 'Settings' },
];

export default function ShellNav() {
  const pathname = usePathname();

  return (
    <nav className="shell-nav" aria-label="Agent shell navigation">
      <ul className="shell-nav-list">
        {NAV_ITEMS.map((item) => {
          const active = pathname === item.href;
          return (
            <li key={item.href}>
              <Link
                href={item.href}
                className={`shell-nav-link${active ? ' active' : ''}`}
              >
                {item.label}
              </Link>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}
