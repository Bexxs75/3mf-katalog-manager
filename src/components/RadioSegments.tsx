import { useId } from 'react';
import { Icon } from './Icon';

interface Option<T extends string> {
  value: T;
  label: string;
}

interface Props<T extends string> {
  /** Group name read by screen readers and shown as the visible heading of the group. */
  legend: string;
  options: Option<T>[];
  value: T;
  onChange: (value: T) => void;
  disabled?: boolean;
  /** Keeps the legend for assistive technology only, when a visible heading already names the group. */
  hideLegend?: boolean;
  /** Spread the options over a two-column grid (four short names do not fit one row of a narrow panel). */
  grid?: boolean;
  className?: string;
}

/**
 * Group of mutually exclusive choices that looks like a segmented control but is made of native
 * radios: the browser provides arrow-key navigation, the checked state and the group name. The
 * selection is marked by a check mark and bold text in addition to the fill colour.
 */
export function RadioSegments<T extends string>({
  legend, options, value, onChange, disabled = false, hideLegend = false, grid = false, className = '',
}: Props<T>) {
  const name = useId();
  return (
    <fieldset disabled={disabled} className={`min-w-0 border-0 p-0 m-0 ${className}`}>
      <legend className={hideLegend ? 'sr-only' : 'p-0 mb-2 text-[length:var(--font-size-body)] font-semibold'}>{legend}</legend>
      <div className={`${grid ? 'grid grid-cols-2' : 'flex flex-wrap'} p-0.5 gap-0.5 border border-[var(--line)] rounded-[3px] bg-[var(--panel-2)]`}>
        {options.map((option) => (
          <label key={option.value} className="relative flex-1 min-w-0">
            <input
              type="radio"
              name={name}
              value={option.value}
              checked={option.value === value}
              onChange={() => onChange(option.value)}
              className="peer absolute inset-0 w-full h-full m-0 opacity-0 cursor-pointer disabled:cursor-not-allowed"
            />
            <span
              className="flex items-center justify-center gap-1 min-h-8 [@media(pointer:coarse)]:min-h-11 px-3 py-1 rounded-[2px] text-center text-[length:var(--font-size-control)] font-medium cursor-pointer transition-colors text-[var(--ink-2)] hover:text-[var(--ink)] peer-focus-visible:outline peer-focus-visible:outline-2 peer-focus-visible:outline-offset-1 peer-focus-visible:outline-[var(--accent)] peer-checked:font-semibold peer-checked:bg-[var(--accent)] peer-checked:text-[var(--accent-ink)] peer-checked:hover:text-[var(--accent-ink)] peer-disabled:opacity-50 peer-disabled:cursor-not-allowed"
            >
              {option.value === value && <Icon name="check" size={14} />}
              {option.label}
            </span>
          </label>
        ))}
      </div>
    </fieldset>
  );
}
