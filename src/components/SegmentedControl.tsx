interface Option<T extends string> {
  value: T;
  label: string;
}

interface Props<T extends string> {
  /** Group name for screen readers (e.g. "Kind"). */
  label: string;
  options: Option<T>[];
  value: T;
  onChange: (value: T) => void;
  disabled?: boolean;
}

/** Custom segmented control (no native control), same look as Dashboard|List. */
export function SegmentedControl<T extends string>({ label, options, value, onChange, disabled = false }: Props<T>) {
  return (
    <div role="group" aria-label={label} className="flex w-max p-0.5 gap-0.5 border border-[var(--line)] rounded-[8px] bg-[var(--panel-2)]">
      {options.map((option) => {
        const active = option.value === value;
        return (
          <button
            key={option.value}
            type="button"
            aria-pressed={active}
            disabled={disabled}
            onClick={() => {
              if (!active) onChange(option.value);
            }}
            className={`h-8 px-3.5 rounded-[6px] text-[12.5px] font-semibold cursor-pointer disabled:cursor-not-allowed disabled:opacity-50 ${
              active ? 'bg-[var(--panel)] text-[var(--ink)] shadow-[var(--shadow)]' : 'text-[var(--ink-3)] hover:text-[var(--ink)]'
            }`}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}
