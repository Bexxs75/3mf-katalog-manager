import type { ReactNode } from 'react';

/**
 * Shared bottom-right container for persistent notices. Stacking them in normal
 * flow keeps a newer notice from covering the close button of an older one. The
 * container lets clicks through to the page; each notice sets
 * `pointer-events-auto` itself.
 */
export function NoticeStack({ children }: { children: ReactNode }) {
  return (
    <div data-testid="notice-stack" className="fixed bottom-4 right-4 z-50 flex flex-col gap-2 items-end pointer-events-none">
      {children}
    </div>
  );
}
