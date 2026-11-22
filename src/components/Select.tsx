import { Children, isValidElement, useCallback, useEffect, useId, useRef, useState, type ButtonHTMLAttributes, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { useAnchoredPopup } from '../hooks/useAnchoredPopup';

interface Props extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'value' | 'onChange' | 'children'> {
  value: string | number;
  onValueChange: (value: string) => void;
  /** Declarative option children; never mounted as native controls. */
  children: ReactNode;
}
interface Option { value: string; text: string; disabled: boolean }
function textOf(node: ReactNode): string {
  return Children.toArray(node).map(child => isValidElement<{ children?: ReactNode }>(child) ? textOf(child.props.children) : String(child)).join('');
}

/** Text-only counterpart of SpoolPicker, sharing its portal and modal registration. */
export function Select({ value, onValueChange, children, disabled, className = '', onKeyDown, onClick, onBlur, ...props }: Props) {
  const options: Option[] = Children.toArray(children).flatMap(child => {
    if (!isValidElement<{ value: string | number; children: ReactNode; disabled?: boolean }>(child)) return [];
    return [{ value: String(child.props.value), text: textOf(child.props.children), disabled: !!child.props.disabled }];
  });
  const uid = useId();
  const triggerId = props.id ?? uid;
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);
  const search = useRef({ text: '', time: 0 });
  const button = useRef<HTMLButtonElement>(null);
  const close = useCallback(() => setOpen(false), []);
  const { popupRef, style } = useAnchoredPopup<HTMLButtonElement, HTMLUListElement>(button, open, close, 0, false);
  const selected = options.findIndex(option => option.value === String(value));
  const enabled = options.map((option, index) => option.disabled ? -1 : index).filter(index => index >= 0);
  const activeIndex = enabled.includes(active) ? active : (enabled[0] ?? -1);
  const blocked = disabled || props['aria-disabled'] === true || props['aria-disabled'] === 'true';
  const show = () => {
    if (blocked) return;
    button.current?.focus({ preventScroll: true });
    search.current = { text: '', time: 0 };
    setActive(enabled.includes(selected) ? selected : (enabled[0] ?? -1));
    setOpen(true);
  };
  useEffect(() => { if (blocked) setOpen(false); }, [blocked]);
  useEffect(() => {
    if (open && activeIndex >= 0) document.getElementById(`${uid}-option-${activeIndex}`)?.scrollIntoView?.({ block: 'nearest' });
  }, [open, activeIndex, uid]);
  const choose = (index: number) => {
    const option = options[index];
    if (blocked || !option || option.disabled) return;
    onValueChange(option.value);
    setOpen(false);
  };
  // Clamp independently of list length: the shared hook flips short popups;
  // long lists scroll in the larger of the spaces above/below the trigger.
  const rect = button.current?.getBoundingClientRect();
  const below = rect ? Math.max(0, window.innerHeight - 8 - rect.bottom - 4) : 0;
  const above = rect ? Math.max(0, rect.top - 12) : 0;
  const downward = below >= Math.min(256, options.length * 36 + 8) || below >= above;
  const maxHeight = Math.min(256, downward ? below : above);
  const vertical = downward
    ? { top: Math.max(8, Math.min(window.innerHeight - 8, rect?.bottom ?? 4) + 4) }
    : { top: undefined, bottom: Math.max(8, window.innerHeight - (rect?.top ?? 12) + 4) };
  return <>
    <button {...props} id={triggerId} ref={button} type="button" role="combobox" disabled={disabled}
      aria-haspopup="listbox" aria-expanded={open} aria-controls={open ? `${uid}-list` : undefined}
      aria-activedescendant={open && activeIndex >= 0 ? `${uid}-option-${activeIndex}` : undefined}
      value={value}
      className={`relative truncate max-w-full min-w-[4rem] min-h-[1.5em] pr-6 text-left text-[var(--ink)] bg-[var(--panel)] disabled:opacity-50 focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)] ${className}`}
      onClick={event => { onClick?.(event); if (!event.defaultPrevented) { if (open) close(); else show(); } }}
      onBlur={event => { onBlur?.(event); close(); }}
      onKeyDown={event => {
        onKeyDown?.(event);
        if (event.defaultPrevented || blocked) return;
        const key = event.key;
        if (key === 'Escape' && open) {
          event.preventDefault(); event.stopPropagation(); close(); button.current?.focus(); return;
        }
        if (key === 'Tab') { close(); return; }
        if (['ArrowDown', 'ArrowUp', 'Home', 'End', 'Enter', ' '].includes(key)) {
          event.preventDefault(); event.stopPropagation();
          if (!open) {
            show();
            if (key === 'Home') setActive(enabled[0] ?? -1);
            if (key === 'End') setActive(enabled[enabled.length - 1] ?? -1);
          } else if (key === 'Enter' || key === ' ') choose(activeIndex);
          else if (key === 'Home') setActive(enabled[0] ?? -1);
          else if (key === 'End') setActive(enabled[enabled.length - 1] ?? -1);
          else setActive(enabled[Math.max(0, Math.min(enabled.length - 1, enabled.indexOf(activeIndex) + (key === 'ArrowDown' ? 1 : -1)))] ?? -1);
          return;
        }
        if (key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
          event.preventDefault(); event.stopPropagation();
          if (!open) show();
          const now = Date.now();
          const query = (now - search.current.time < 700 ? search.current.text : '') + key.toLocaleLowerCase();
          search.current = { text: query, time: now };
          const prefix = [...query].every(char => char === query[0]) ? query[0] : query;
          const start = open ? activeIndex : selected;
          const order = [...enabled.filter(index => index > start), ...enabled.filter(index => index <= start)];
          const match = order.find(index => options[index].text.trim().toLocaleLowerCase().startsWith(prefix));
          if (match !== undefined) setActive(match);
        }
      }}>{options[selected]?.text || '\u00a0'}
      {/* Pure-CSS chevron so the trigger reads as a select in both themes. */}
      <span aria-hidden="true" className="pointer-events-none absolute right-2 top-1/2 -mt-[5px] h-2 w-2 rotate-45 border-b-2 border-r-2 border-current" />
    </button>
    {open && style && createPortal(<ul ref={popupRef} id={`${uid}-list`} role="listbox"
      aria-label={props['aria-label']} aria-labelledby={props['aria-label'] ? undefined : triggerId}
      style={{ ...style, left: Math.max(8, Math.min(style.left, window.innerWidth - 8 - style.width)), ...vertical, maxHeight, overflowY: 'auto' }}
      className="z-[110] rounded-md border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] shadow-[var(--shadow)] py-1"
      onMouseDown={event => event.preventDefault()}>
      {options.map((option, index) => <li key={option.value} id={`${uid}-option-${index}`} role="option"
        aria-selected={index === selected} aria-disabled={option.disabled || undefined} data-value={option.value}
        onMouseMove={() => { if (!option.disabled) setActive(index); }} onClick={() => choose(index)}
        className={`px-2 py-1.5 text-small break-words ${option.disabled ? 'opacity-50' : 'cursor-pointer'} ${index === activeIndex ? 'bg-[var(--panel-2)]' : ''}`}>
        {option.text}
      </li>)}
    </ul>, document.body)}
  </>;
}
