interface Props {
  checked: boolean;
  onToggle: () => void;
  className?: string;
}

export function BulkCheckbox({ checked, onToggle, className = '' }: Props) {
  return (
    <label
      onClick={(e) => e.stopPropagation()}
      className={`cursor-pointer ${className}`}
    >
      <input type="checkbox" checked={checked} onChange={onToggle} className="sr-only" />
      <span
        className={`flex items-center justify-center w-4 h-4 rounded-[3px] border shadow-[0_1px_2px_rgba(0,0,0,0.4)] ${
          checked
            ? 'bg-[var(--accent)] border-[var(--accent)]'
            : 'bg-[var(--panel)]/90 border-[var(--line-strong)] hover:border-[var(--accent)]'
        }`}
      >
        {checked && (
          <svg viewBox="0 0 16 16" className="w-2.5 h-2.5" fill="none">
            <path
              d="M3 8l3 3 7-7"
              stroke="var(--accent-ink)"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        )}
      </span>
    </label>
  );
}
