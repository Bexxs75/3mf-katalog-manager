import type { ReactNode } from 'react';

/** A second level for background that most people do not need: a native disclosure, so the keyboard and screen readers work without extra code. */
export function Disclosure({ summary, children, className = '' }: { summary: string; children: ReactNode; className?: string }) {
  return (
    <details className={className}>
      <summary className="cursor-pointer text-small text-[var(--ink-2)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]">{summary}</summary>
      <div className="mt-1 text-caption text-[var(--ink-2)]">{children}</div>
    </details>
  );
}
