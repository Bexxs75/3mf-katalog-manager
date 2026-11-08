interface Props {
  checked: boolean;
  onChange: (next: boolean) => void;
  label: string;
  disabled?: boolean;
  busy?: boolean;
  /** Id of the element that explains the switch (dependencies, platform notes). */
  'aria-describedby'?: string;
}

/** Custom switch (native checkboxes ignore the dark theme). */
export function ToggleSwitch({ checked, onChange, label, disabled, busy, 'aria-describedby': describedBy }: Props) {
  const toggle = () => !disabled && !busy && onChange(!checked);
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      aria-describedby={describedBy}
      disabled={disabled}
      aria-busy={busy || undefined}
      aria-disabled={busy || undefined}
      onClick={toggle}
      onKeyDown={(e) => {
        if (e.key === ' ' || e.key === 'Enter') {
          e.preventDefault();
          toggle();
        }
      }}
      className={`${disabled || busy ? 'opacity-50' : 'hover:outline hover:outline-2 hover:outline-[var(--ink-3)]'} relative flex-none w-9 h-5 rounded-full transition-colors cursor-pointer disabled:opacity-50 focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)] ${
        checked ? 'bg-[var(--accent)]' : 'bg-[var(--line-strong)]'
      }`}
    >
      <span
        className={`absolute top-[3px] w-3.5 h-3.5 rounded-full transition-all ${
          checked ? 'right-[3px] bg-[var(--accent-ink)]' : 'left-[3px] bg-[var(--ink-3)]'
        }`}
      />
    </button>
  );
}
