import { useEffect, useRef, useState, type KeyboardEvent } from 'react';
import { ContextMenuFrame } from './ContextMenu';

export function isContextMenuKey(event: KeyboardEvent) {
  return event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10');
}

export function navigateMenu(event: KeyboardEvent) {
  if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
  const root = (event.target as HTMLElement).closest('[role="menu"]');
  const items = Array.from(root?.querySelectorAll<HTMLButtonElement>('[role^="menuitem"]') ?? [])
    .filter(item => !item.disabled && item.closest('[role="menu"]') === root);
  if (!items.length) return;
  const index = items.indexOf(document.activeElement as HTMLButtonElement);
  const next = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1
    : (index + (event.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length;
  event.preventDefault(); event.stopPropagation(); items[next].focus();
}

export interface SurfaceMenuEntry {
  label: string;
  separator?: boolean;
  action?: () => void;
  disabled?: boolean;
  danger?: boolean;
  checked?: boolean;
  shortcut?: string;
  children?: SurfaceMenuEntry[];
}
export const surfaceMenuButton = 'w-full text-left px-3 py-2 text-body text-[var(--ink)] hover:bg-[var(--panel-2)] disabled:opacity-50';

export function CatalogSurfaceMenu({ x, y, label, entries, onClose }: {
  x: number; y: number; label: string; entries: SurfaceMenuEntry[]; onClose: () => void;
}) {
  const [submenu, setSubmenu] = useState<number | null>(null);
  const trigger = useRef<HTMLButtonElement | null>(null);
  const sub = useRef<HTMLDivElement>(null);
  useEffect(() => { if (submenu !== null) sub.current?.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus(); }, [submenu]);
  const back = () => { setSubmenu(null); trigger.current?.focus(); };
  const item = (entry: SurfaceMenuEntry, index: number) => entry.separator
    ? <div key={index} role="separator" className="h-px my-1 bg-[var(--line)]" />
    : <button key={index} type="button" tabIndex={-1}
    role={entry.checked === undefined ? 'menuitem' : 'menuitemradio'} aria-checked={entry.checked}
    disabled={entry.disabled} aria-keyshortcuts={entry.shortcut}
    className={`${surfaceMenuButton} ${entry.danger ? '!text-[var(--crit)]' : ''} ${entry.checked ? 'bg-[var(--accent-soft)]' : ''}`}
    onClick={() => { onClose(); entry.action?.(); }}>{entry.label}{entry.shortcut && <span className="ml-5 text-[var(--ink-3)]">{entry.shortcut}</span>}</button>;
  return <ContextMenuFrame x={x} y={y} onClose={onClose} onEscape={submenu === null ? onClose : back}>
    <div role="menu" aria-label={label} onKeyDown={event => {
      navigateMenu(event);
      if (!event.altKey && !event.ctrlKey && !event.metaKey && !event.shiftKey) {
        const entry = entries.find(item => item.shortcut === event.key && !item.disabled);
        if (entry) { event.preventDefault(); event.stopPropagation(); onClose(); entry.action?.(); }
      }
      if (event.key === 'Tab') onClose();
    }}>
      {entries.map((entry, index) => entry.children ? <div key={index}>
        <button type="button" role="menuitem" tabIndex={-1} className={surfaceMenuButton} aria-haspopup="menu" aria-expanded={submenu === index}
          onClick={event => { trigger.current = event.currentTarget; setSubmenu(submenu === index ? null : index); }}
          onKeyDown={event => { if (event.key === 'ArrowRight') { event.preventDefault(); event.stopPropagation(); trigger.current = event.currentTarget; setSubmenu(index); } }}>{entry.label}</button>
        {submenu === index && <div ref={sub} role="menu" aria-label={entry.label} className="ml-3 border-l border-[var(--line)]" onKeyDown={event => {
          if (event.key === 'ArrowLeft' || event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); back(); }
        }}>{entry.children.map(item)}</div>}
      </div> : item(entry, index))}
    </div>
  </ContextMenuFrame>;
}
