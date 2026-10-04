export function DragGrip({ className = '' }: { className?: string }) {
  return (
    <span data-drag-grip aria-hidden="true" className={`pointer-events-none w-4 h-4 shrink-0 grid place-items-center rounded border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-3)] opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 ${className}`}>
      <svg viewBox="0 0 10 10" width="11" height="11" fill="currentColor">
        <circle cx="3" cy="2" r="1" /><circle cx="7" cy="2" r="1" />
        <circle cx="3" cy="5" r="1" /><circle cx="7" cy="5" r="1" />
        <circle cx="3" cy="8" r="1" /><circle cx="7" cy="8" r="1" />
      </svg>
    </span>
  );
}
