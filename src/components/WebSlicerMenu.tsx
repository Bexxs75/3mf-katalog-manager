import { useEffect, useId, useLayoutEffect, useRef, useState, type KeyboardEvent } from 'react';
import { useT } from '../i18n/LanguageContext';
import { createSlicerLink } from '../lib/api/slicer';
import { buildSlicerUrl, fileTypeOfName, slicersFor, type WebSlicer } from '../lib/slicerLinks';
import { useCanOpenInSlicerLink } from '../lib/webSession';
import { messageOf } from '../lib/errors';
import { Icon } from './Icon';

/** Web counterpart of the desktop "open in slicer": hands the slicer a short-lived signed download link. */
/** `fileType` is the catalog's own type; the name's ending is only a fallback when it is missing. */
export function WebSlicerMenu({ fileId, fileName, fileType, className }: {
  fileId: string; fileName: string; fileType?: string; className?: string;
}) {
  const t = useT();
  const enabled = useCanOpenInSlicerLink();
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [alignRight, setAlignRight] = useState(false);
  const request = useRef(0);
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const menuId = useId();
  const hintId = useId();
  useEffect(() => {
    request.current++; setOpen(false); setBusy(false); setError('');
    return () => { request.current++; };
  }, [fileId]);
  // Opens towards the free side so the popover is never cut off by the panel or the window edge.
  useLayoutEffect(() => {
    if (!open || !trigger.current) return;
    const rect = trigger.current.getBoundingClientRect();
    setAlignRight(rect.left + 256 > window.innerWidth - 8 && rect.right - 256 >= 8);
  }, [open]);
  useEffect(() => {
    if (!open) return;
    menu.current?.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
    const close = (event: Event) => {
      if (event.type === 'keydown') {
        if ((event as KeyboardEvent<Element> & globalThis.KeyboardEvent).key !== 'Escape') return;
        // Marks the key as handled so window-level Escape shortcuts (closing the detail panel) leave it alone.
        event.preventDefault();
        setOpen(false); trigger.current?.focus();
      } else if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', close); document.addEventListener('keydown', close);
    return () => { document.removeEventListener('mousedown', close); document.removeEventListener('keydown', close); };
  }, [open]);
  const navigate = (event: KeyboardEvent<HTMLDivElement>) => {
    const items = Array.from(menu.current?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? []);
    const at = items.indexOf(document.activeElement as HTMLElement);
    const target = event.key === 'ArrowDown' ? (at + 1) % items.length
      : event.key === 'ArrowUp' ? (at <= 0 ? items.length - 1 : at - 1)
      : event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 : -1;
    if (target < 0 || items.length === 0) return;
    event.preventDefault(); items[target].focus();
  };
  if (!enabled) return null;
  const slicers = slicersFor(fileType ?? fileTypeOfName(fileName) ?? '');
  if (slicers.length === 0) return null;
  const launch = async (slicer: WebSlicer) => {
    const current = ++request.current;
    setBusy(true); setError('');
    try {
      const link = await createSlicerLink(fileId);
      if (request.current !== current) return;
      const anchor = document.createElement('a');
      anchor.href = buildSlicerUrl(slicer, window.location.origin, link.path);
      document.body.append(anchor);
      anchor.click(); anchor.remove();
      setOpen(false);
    } catch (e) {
      if (request.current === current) setError(`${t('webSlicerError')} ${messageOf(e)}`.trim());
    } finally {
      if (request.current === current) setBusy(false);
    }
  };
  return <div ref={root} className="relative flex-none">
    <button ref={trigger} aria-haspopup="menu" aria-expanded={open} aria-controls={open ? menuId : undefined} disabled={busy}
      className={className ?? 'h-8 px-3 whitespace-nowrap rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-small font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)] disabled:opacity-50'}
      onClick={() => setOpen(value => !value)}>
      {t('openInSlicer')} <Icon name="external" size={14} />
    </button>
    {open && <div ref={menu} id={menuId} role="menu" aria-label={t('openInSlicer')} aria-describedby={hintId} onKeyDown={navigate}
      className={`absolute bottom-full ${alignRight ? 'right-0' : 'left-0'} z-20 mb-1 w-64 max-w-[calc(100vw-1rem)] rounded-md border border-[var(--line-strong)] bg-[var(--panel)] p-1 shadow-lg`}>
      {slicers.map(slicer => <button key={slicer.id} role="menuitem" tabIndex={-1} disabled={busy}
        className="block w-full px-3 py-1.5 text-left text-small text-[var(--ink)] hover:bg-[var(--panel-2)] hover:text-[var(--accent)] disabled:opacity-50"
        onClick={() => void launch(slicer)}>{slicer.name}</button>)}
      <p id={hintId} className="px-3 py-1.5 text-caption text-[var(--ink-3)]">{t('webSlicerHint')} {t('webSlicerNotInstalled')}</p>
    </div>}
    {error && <p role="alert" className="text-small text-[var(--accent)]">{error}</p>}
  </div>;
}
