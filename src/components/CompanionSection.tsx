import { useCallback, useEffect, useId, useRef, useState, type KeyboardEvent, type ReactNode, type RefObject } from 'react';
import { createPortal } from 'react-dom';
import { invoke } from '@tauri-apps/api/core';
import { useLanguage, useT, useFormatCount } from '../i18n/LanguageContext';
import { formatBytes } from '../i18n/format';
import { displayName } from '../lib/displayName';
import { toAppError, type AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';
import { useImportLock } from '../hooks/ImportLockContext';
import { useRuntimeEnvironment } from '../hooks/useRuntimeEnvironment';
import { useModalDialog } from '../hooks/useModalDialog';
import {
  addCompanion, companionImageData, companionOpenInfo, listCompanions, openCompanion,
  pickCompanionFiles, removeCompanion, restoreCompanion,
} from '../lib/api/companions';
import { trashDaysLeft } from '../lib/trashRetention';
import type { Companion } from '../types';
import { Icon } from './Icon';

interface Props {
  fileId: string;
  /** Matches the spacing of the side panel densities and of the detail page. */
  variant: 'compact' | 'comfort' | 'page';
}

// Long enough to find the button, read the hint and decide (same as the move and rename notices).
const UNDO_DURATION_MS = 15000;

interface AddFailure { name: string; error: AppError }

const buttonClass = 'h-7 px-2.5 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] text-caption font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)] disabled:opacity-50 disabled:cursor-default';

const kindIcon = (item: Companion) => (item.kind === 'pdf' ? '📄' : item.kind === 'text' ? '📝' : '🖼');

function baseName(path: string) {
  return path.split(/[\\/]/).pop() || path;
}

/** Calls `onVisible` once when the node scrolls into view; at once where IntersectionObserver is missing. */
function useVisibleOnce<T extends HTMLElement>(onVisible: () => void) {
  const ref = useRef<T | null>(null);
  const latest = useRef(onVisible);
  latest.current = onVisible;
  useEffect(() => {
    const node = ref.current;
    if (!node) return;
    if (typeof IntersectionObserver === 'undefined') { latest.current(); return; }
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) { latest.current(); observer.disconnect(); }
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, []);
  return ref;
}

function ImageLightbox({ name, src, onClose, returnFocus }: {
  name: string; src: string | undefined; onClose: () => void; returnFocus: RefObject<HTMLElement | null>;
}) {
  const t = useT();
  const dialog = useModalDialog({ open: true, onClose, returnFocus, initialFocus: '[data-close]' });
  return createPortal(
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/70 p-6" onClick={onClose}>
      <div
        ref={dialog}
        role="dialog"
        aria-modal="true"
        aria-label={displayName(name)}
        tabIndex={-1}
        onClick={(event) => event.stopPropagation()}
        className="relative max-w-[min(92vw,1100px)] max-h-[88vh] flex flex-col gap-2 rounded-[6px] bg-[var(--panel)] p-3 shadow-[var(--shadow)]"
      >
        <div className="flex items-start gap-3">
          <p className="flex-1 min-w-0 text-small font-semibold break-words">{displayName(name)}</p>
          <button
            type="button"
            data-close
            onClick={onClose}
            aria-label={t('companionPreviewClose')}
            title={t('companionPreviewClose')}
            className="flex-none w-7 h-7 grid place-items-center rounded-[3px] border border-[var(--line-strong)] text-[var(--ink-2)] cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            <Icon name="close" size={14} />
          </button>
        </div>
        {src
          ? <img src={src} alt={displayName(name)} className="max-w-full max-h-[75vh] object-contain mx-auto" />
          : <p className="text-small text-[var(--ink-3)]">{t('companionPreviewUnavailable')}</p>}
      </div>
    </div>,
    document.body,
  );
}

function ActionMenu({ item, busy, canReveal, onRemove, onReveal, lockProps }: {
  item: Companion; busy: boolean; canReveal: boolean; onRemove: () => void; onReveal: () => void;
  lockProps: { disabled?: boolean; title?: string };
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const wrapper = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const menuId = useId();
  useEffect(() => {
    if (!open) return;
    wrapper.current?.querySelector<HTMLElement>('[role="menuitem"]:not(:disabled)')?.focus();
    const outside = (event: MouseEvent) => {
      if (event.target instanceof Node && !wrapper.current?.contains(event.target)) setOpen(false);
    };
    document.addEventListener('mousedown', outside);
    return () => document.removeEventListener('mousedown', outside);
  }, [open]);
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === 'Escape' && open) {
      // A nested popup: Escape closes only the menu, not the page around it.
      event.preventDefault(); event.stopPropagation();
      setOpen(false); trigger.current?.focus();
      return;
    }
    if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
    const items = Array.from(wrapper.current?.querySelectorAll<HTMLElement>('[role="menuitem"]:not(:disabled)') ?? []);
    if (!items.length) return;
    event.preventDefault();
    const at = items.indexOf(document.activeElement as HTMLElement);
    const next = event.key === 'ArrowDown' ? (at + 1) % items.length : (at - 1 + items.length) % items.length;
    items[next].focus();
  };
  const itemClass = 'block w-full text-left px-3 py-1.5 text-small whitespace-nowrap cursor-pointer hover:bg-[var(--panel-2)] disabled:opacity-50 disabled:cursor-default';
  const label = t('companionMenu').replace('{name}', displayName(item.name));
  return (
    <div ref={wrapper} className="relative flex-none" onKeyDown={onKeyDown}>
      <button
        ref={trigger}
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        aria-label={label}
        title={label}
        onClick={() => setOpen((value) => !value)}
        className="w-7 h-7 grid place-items-center rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
      >
        <span aria-hidden="true">⋯</span>
      </button>
      {open && (
        <div id={menuId} role="menu" aria-label={label} className="absolute right-0 top-8 z-20 min-w-[180px] py-1 rounded-[4px] border border-[var(--line-strong)] bg-[var(--panel)] shadow-[var(--shadow)]">
          {canReveal && !item.missing && (
            <button type="button" role="menuitem" className={itemClass} onClick={() => { setOpen(false); onReveal(); }}>
              {t('companionReveal')}
            </button>
          )}
          <button
            type="button"
            role="menuitem"
            {...lockProps}
            disabled={lockProps.disabled || busy}
            className={itemClass}
            onClick={() => { setOpen(false); onRemove(); }}
          >
            {t('companionRemove')}
          </button>
        </div>
      )}
    </div>
  );
}

function ImageTile({ item, src, onVisible, onOpen, menu }: {
  item: Companion; src: string | undefined; onVisible: () => void;
  onOpen: (button: HTMLButtonElement) => void; menu: ReactNode;
}) {
  const t = useT();
  const ref = useVisibleOnce<HTMLButtonElement>(onVisible);
  return (
    <li className="relative w-[84px] flex flex-col gap-1">
      <button
        ref={ref}
        type="button"
        aria-label={t('companionViewImage').replace('{name}', displayName(item.name))}
        onClick={(event) => onOpen(event.currentTarget)}
        className="w-[84px] h-[64px] overflow-hidden rounded-[3px] border border-[var(--line-strong)] bg-[var(--plate)] grid place-items-center cursor-pointer hover:border-[var(--accent)]"
      >
        {src
          ? <img src={src} alt="" className="w-full h-full object-cover" />
          : <span aria-hidden="true" className="text-[var(--ink-3)]"><Icon name="model" size={18} /></span>}
      </button>
      <span className="text-compact-meta text-[var(--ink-3)] break-words" style={{ overflowWrap: 'anywhere' }}>{displayName(item.name)}</span>
      <div className="absolute right-0.5 top-0.5">{menu}</div>
    </li>
  );
}

export function CompanionSection({ fileId, variant }: Props) {
  const t = useT();
  const formatCount = useFormatCount();
  const { language } = useLanguage();
  const { lockProps } = useImportLock();
  const { container } = useRuntimeEnvironment();
  const headingId = useId();
  const [items, setItems] = useState<Companion[]>([]);
  const [removed, setRemoved] = useState<Companion[]>([]);
  const [showRemoved, setShowRemoved] = useState(false);
  // The entry just removed, while the "Undo" button of its notice is on offer.
  const [undoTarget, setUndoTarget] = useState<{ id: string; name: string } | null>(null);
  const removedId = useId();
  const [loaded, setLoaded] = useState(false);
  const [loadError, setLoadError] = useState<AppError | null>(null);
  const [actionError, setActionError] = useState<AppError | null>(null);
  const [restoreError, setRestoreError] = useState<AppError | null>(null);
  const [failures, setFailures] = useState<AddFailure[]>([]);
  const [notice, setNotice] = useState('');
  const [busy, setBusy] = useState(false);
  const [images, setImages] = useState<Record<string, string>>({});
  const [preview, setPreview] = useState<{ id: string; name: string } | null>(null);
  const opener = useRef<HTMLElement | null>(null);
  const addButton = useRef<HTMLButtonElement>(null);
  const request = useRef(0);
  const requestedImages = useRef(new Set<string>());
  const busyRef = useRef(false);

  const reload = useCallback(async () => {
    const current = ++request.current;
    try {
      const all = await listCompanions(fileId);
      if (request.current !== current) return;
      const list = Array.isArray(all) ? all : [];
      setItems(list.filter((item) => !item.deletedAt));
      setRemoved(list.filter((item) => !!item.deletedAt));
      setLoadError(null);
    } catch (e) {
      if (request.current === current) setLoadError(toAppError(e));
    } finally {
      if (request.current === current) setLoaded(true);
    }
  }, [fileId]);

  useEffect(() => {
    setItems([]); setRemoved([]); setShowRemoved(false); setUndoTarget(null); setLoaded(false); setLoadError(null); setActionError(null); setRestoreError(null); setFailures([]);
    setNotice(''); setImages({}); setPreview(null);
    requestedImages.current = new Set();
    void reload();
    return () => { request.current++; };
  }, [fileId, reload]);

  // The button goes away after a while; the text stays, and the "Removed" list still offers the way back.
  useEffect(() => {
    if (!undoTarget) return;
    const timer = setTimeout(() => setUndoTarget(null), UNDO_DURATION_MS);
    return () => clearTimeout(timer);
  }, [undoTarget]);

  // Pictures are fetched one by one when their tile shows up, never with the list.
  const loadImage = useCallback((id: string) => {
    if (requestedImages.current.has(id)) return;
    requestedImages.current.add(id);
    companionImageData(id).then((data) => {
      if (typeof data === 'string') setImages((previous) => ({ ...previous, [id]: data }));
    }).catch(() => { /* the tile keeps its placeholder; the file can still be opened */ });
  }, []);

  // One action at a time. The buttons stay enabled meanwhile so focus is not dropped by `disabled`.
  const guarded = async (action: () => Promise<void>) => {
    if (busyRef.current) return;
    busyRef.current = true; setBusy(true);
    setActionError(null); setRestoreError(null); setNotice(''); setUndoTarget(null);
    try { await action(); } finally { busyRef.current = false; setBusy(false); }
  };

  const add = () => guarded(async () => {
    let paths: string[];
    try { paths = await pickCompanionFiles(); } catch (e) { setActionError(toAppError(e)); return; }
    setFailures([]);
    if (!Array.isArray(paths) || paths.length === 0) return;
    const failed: AddFailure[] = [];
    let added = 0;
    for (const path of paths) {
      try { await addCompanion(fileId, path); added++; }
      catch (e) { failed.push({ name: baseName(path), error: toAppError(e) }); }
    }
    setFailures(failed);
    if (added > 0) setNotice(formatCount(t('companionsAdded'), added));
    await reload();
    addButton.current?.focus();
  });

  const remove = (item: Companion) => guarded(async () => {
    try {
      await removeCompanion(item.id);
      setNotice(t('companionRemoved').replace('{name}', displayName(item.name)));
      setUndoTarget({ id: item.id, name: item.name });
    } catch (e) { setActionError(toAppError(e)); }
    await reload();
    // The removed entry's controls are gone, so focus goes to the stable add button.
    addButton.current?.focus();
  });

  const restore = (item: { id: string; name: string }) => guarded(async () => {
    try {
      await restoreCompanion(item.id);
      setNotice(t('companionRestored').replace('{name}', displayName(item.name)));
    } catch (e) { setRestoreError(toAppError(e)); }
    await reload();
    // The entry's button is gone after a restore, so focus goes to the stable add button.
    addButton.current?.focus();
  });

  const open = (item: Companion) => guarded(async () => {
    try { await openCompanion(item.id); } catch (e) { setActionError(toAppError(e)); }
  });

  const reveal = (item: Companion) => guarded(async () => {
    try {
      const info = await companionOpenInfo(item.id);
      await invoke('open_in_file_manager', { path: info.path });
    } catch (e) { setActionError(toAppError(e)); }
  });

  const headingClass = variant === 'compact' ? 'text-compact-meta ui-label text-[var(--ink-3)]' : 'ui-label text-[var(--ink-3)]';
  const headingStyle = variant === 'comfort' ? { fontSize: 'var(--font-size-label)' } : undefined;
  const wrapperClass = variant === 'compact' ? 'px-4 pt-3.5 pb-3 border-t border-[var(--line)]'
    : variant === 'comfort' ? 'px-[18px] pt-3.5 pb-4 border-t border-[var(--line)]'
    : '';
  const imageItems = items.filter((item) => item.kind === 'image' && !item.missing);
  const rowItems = items.filter((item) => item.kind !== 'image' || item.missing);

  const menuFor = (item: Companion) => (
    <ActionMenu
      item={item}
      busy={busy}
      canReveal={!container}
      lockProps={lockProps}
      onRemove={() => void remove(item)}
      onReveal={() => void reveal(item)}
    />
  );

  return (
    <section aria-labelledby={headingId} className={wrapperClass}>
      <div className="flex items-center gap-2 pb-2">
        <h3 id={headingId} className={`flex-1 ${headingClass}`} style={headingStyle}>
          {t('companionsHeading')}{items.length > 0 && ` (${items.length})`}
        </h3>
        <button
          ref={addButton}
          type="button"
          {...lockProps}
          aria-disabled={busy || undefined}
          onClick={() => void add()}
          className={buttonClass}
        >
          {t('companionsAdd')}
        </button>
      </div>

      {loaded && !loadError && items.length === 0 && (
        <p className="text-small text-[var(--ink-3)]">{t('companionsEmpty')}</p>
      )}
      {loadError && (
        <p role="alert" className="text-small text-[var(--accent)] break-words">
          {t('companionsLoadFailed')} <ErrorText error={loadError} />
        </p>
      )}

      {imageItems.length > 0 && (
        <ul className="flex flex-wrap gap-2 pb-2">
          {imageItems.map((item) => (
            <ImageTile
              key={item.id}
              item={item}
              src={images[item.id]}
              onVisible={() => loadImage(item.id)}
              onOpen={(button) => { opener.current = button; loadImage(item.id); setPreview({ id: item.id, name: item.name }); }}
              menu={menuFor(item)}
            />
          ))}
        </ul>
      )}

      {rowItems.length > 0 && (
        <ul>
          {rowItems.map((item) => (
            <li key={item.id} className="flex items-center gap-2 py-1.5 border-b border-[var(--line)]">
              <span aria-hidden="true" className="flex-none w-5 text-center">
                {kindIcon(item)}
              </span>
              <div className="flex-1 min-w-0">
                <div className="text-small font-medium break-words" style={{ overflowWrap: 'anywhere' }}>{displayName(item.name)}</div>
                <div className="text-compact-meta text-[var(--ink-3)] tabular-nums">
                  {item.missing
                    ? <span className="text-[var(--accent)]">{t('companionMissing')}</span>
                    : item.sizeBytes !== null && formatBytes(item.sizeBytes, language)}
                </div>
              </div>
              {item.missing ? (
                <button type="button" {...lockProps} onClick={() => void remove(item)}
                  aria-label={`${t('companionRemove')}: ${displayName(item.name)}`} className={buttonClass}>
                  {t('companionRemove')}
                </button>
              ) : (
                <>
                  <button type="button" aria-label={t('companionOpenNamed').replace('{name}', displayName(item.name))}
                    onClick={() => void open(item)} className={buttonClass}>
                    {t('companionOpen')}
                  </button>
                  {menuFor(item)}
                </>
              )}
            </li>
          ))}
        </ul>
      )}

      {removed.length > 0 && (
        <div className="pt-2">
          <button
            type="button"
            aria-expanded={showRemoved}
            aria-controls={removedId}
            onClick={() => setShowRemoved((value) => !value)}
            className="text-small font-medium text-[var(--accent)] cursor-pointer hover:underline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)]"
          >
            {t('companionsRemovedToggle').replace('{count}', String(removed.length))}{' '}
            <span aria-hidden="true" className={`inline-block align-middle ${showRemoved ? 'rotate-180' : ''}`}><Icon name="chevron" size={12} /></span>
          </button>
          <div id={removedId} hidden={!showRemoved}>
            {showRemoved && (
              <>
                <ul>
                  {removed.map((item) => {
                    const days = trashDaysLeft(item.deletedAt);
                    return (
                      <li key={item.id} className="flex items-center gap-2 py-1.5 border-b border-[var(--line)]">
                        <span aria-hidden="true" className="flex-none w-5 text-center opacity-60">{kindIcon(item)}</span>
                        <div className="flex-1 min-w-0">
                          <div className="text-small text-[var(--ink-3)] break-words" style={{ overflowWrap: 'anywhere' }}>{displayName(item.name)}</div>
                          <div className="text-compact-meta text-[var(--ink-3)] tabular-nums">
                            {days === null ? t('companionRemovedExpiresSoon') : formatCount(t('companionRemovedDaysLeft'), days)}
                          </div>
                        </div>
                        <button type="button" {...lockProps} onClick={() => void restore(item)}
                          aria-label={t('companionRestoreNamed').replace('{name}', displayName(item.name))} className={buttonClass}>
                          {t('companionRestore')}
                        </button>
                      </li>
                    );
                  })}
                </ul>
                <p className="pt-2 text-compact-meta text-[var(--ink-3)] break-words">{t('companionsRemovedHint')}</p>
              </>
            )}
          </div>
        </div>
      )}

      {notice && (
        <p role="status" className="pt-2 text-small text-[var(--ink-2)] break-words">
          {notice}
          {undoTarget && (
            <>
              {' '}
              <button type="button" {...lockProps} onClick={() => void restore(undoTarget)}
                className="font-semibold text-[var(--accent)] underline cursor-pointer disabled:opacity-50 disabled:cursor-default focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)]">
                {t('undoButton')}
              </button>
            </>
          )}
        </p>
      )}
      {restoreError && (
        <p role="alert" className="pt-2 text-small text-[var(--accent)] break-words">
          {t('companionRestoreFailed')} <ErrorText error={restoreError} />
        </p>
      )}
      {actionError && (
        <p role="alert" className="pt-2 text-small text-[var(--accent)] break-words"><ErrorText error={actionError} /></p>
      )}
      {failures.length > 0 && (
        <div role="alert" className="pt-2 flex flex-col gap-1 text-small text-[var(--accent)]">
          {failures.map((failure, index) => (
            <p key={index} className="break-words">
              {t('companionAddFailed').replace('{name}', displayName(failure.name))} <ErrorText error={failure.error} />
            </p>
          ))}
        </div>
      )}

      {preview && (
        <ImageLightbox
          name={preview.name}
          src={images[preview.id]}
          onClose={() => setPreview(null)}
          returnFocus={opener}
        />
      )}
    </section>
  );
}
