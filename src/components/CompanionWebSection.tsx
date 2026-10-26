import { useCallback, useEffect, useId, useRef, useState } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatBytes } from '../i18n/format';
import { displayName } from '../lib/displayName';
import { toAppError, type AppError } from '../lib/errors';
import { getSession, useCanManageFiles } from '../lib/webSession';
import { ErrorText } from '../diagnostics/ErrorText';
import { companionDownloadUrl, companionImageData, listWebCompanions, removeWebCompanion, restoreWebCompanion } from '../lib/api/companions';
import type { WebCompanion } from '../types';
import { Icon } from './Icon';
import { ImageLightbox, buttonClass, kindIcon, useVisibleOnce } from './CompanionSection';

interface Props {
  fileId: string;
  /** Matches the spacing of the side panel densities and of the detail page. */
  variant: 'compact' | 'comfort' | 'page';
}

// Long enough to find the button and decide (same as on the desktop).
const UNDO_DURATION_MS = 15000;

/** Starts the browser download of one companion. The session is checked first so an expired login shows the login page. */
async function startDownload(id: string) {
  const session = await getSession();
  if (!session.authenticated || session.mustChangePassword) return;
  const link = document.createElement('a');
  link.href = companionDownloadUrl(id);
  link.download = '';
  document.body.append(link);
  link.click();
  link.remove();
}

function ImageTile({ item, src, onVisible, onOpen, onDownload, onRemove, busy }: {
  item: WebCompanion; src: string | undefined; onVisible: () => void;
  onOpen: (button: HTMLButtonElement) => void; onDownload: () => void; onRemove?: () => void; busy?: boolean;
}) {
  const t = useT();
  const { language } = useLanguage();
  const ref = useVisibleOnce<HTMLButtonElement>(onVisible);
  const name = displayName(item.name);
  return (
    <li className="w-[84px] flex flex-col gap-1">
      <button
        ref={ref}
        type="button"
        disabled={!item.previewable}
        aria-label={t('companionViewImage').replace('{name}', name)}
        onClick={(event) => onOpen(event.currentTarget)}
        className="w-[84px] h-[64px] overflow-hidden rounded-[3px] border border-[var(--line-strong)] bg-[var(--plate)] grid place-items-center cursor-pointer hover:border-[var(--accent)] disabled:cursor-default"
      >
        {src
          ? <img src={src} alt="" className="w-full h-full object-cover" />
          : <span aria-hidden="true" className="text-[var(--ink-3)]"><Icon name="model" size={18} /></span>}
      </button>
      <span className="text-compact-meta text-[var(--ink-3)] break-words" style={{ overflowWrap: 'anywhere' }}>{name}</span>
      {item.sizeBytes !== null && <span className="text-compact-meta text-[var(--ink-3)] tabular-nums">{formatBytes(item.sizeBytes, language)}</span>}
      <button type="button" aria-label={t('companionDownloadNamed').replace('{name}', name)} onClick={onDownload} className={buttonClass}>
        {t('companionDownload')}
      </button>
      {onRemove && (
        <button type="button" disabled={busy} aria-label={t('companionRemoveNamed').replace('{name}', name)} onClick={onRemove} className={buttonClass}>
          {t('companionRemove')}
        </button>
      )}
    </li>
  );
}

/** Companion list for the web build: name, kind, size, download and a picture preview. With the file-management right, a file can also be removed (and brought back). */
export function CompanionWebSection({ fileId, variant }: Props) {
  const t = useT();
  const { language } = useLanguage();
  const canManage = useCanManageFiles();
  const headingId = useId();
  const headingRef = useRef<HTMLHeadingElement>(null);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState('');
  const [undoTarget, setUndoTarget] = useState<{ id: string; name: string } | null>(null);
  const [restoreError, setRestoreError] = useState<AppError | null>(null);
  const [items, setItems] = useState<WebCompanion[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [loadError, setLoadError] = useState<AppError | null>(null);
  const [actionError, setActionError] = useState<AppError | null>(null);
  const [images, setImages] = useState<Record<string, string>>({});
  const [preview, setPreview] = useState<{ id: string; name: string } | null>(null);
  const opener = useRef<HTMLElement | null>(null);
  const requestedImages = useRef(new Set<string>());

  useEffect(() => {
    let current = true;
    setItems([]); setLoaded(false); setLoadError(null); setActionError(null); setImages({}); setPreview(null);
    setNotice(''); setUndoTarget(null); setRestoreError(null);
    requestedImages.current = new Set();
    listWebCompanions(fileId, canManage).then((list) => {
      if (current) setItems(Array.isArray(list) ? list : []);
    }).catch((e) => {
      if (current) setLoadError(toAppError(e));
    }).finally(() => {
      if (current) setLoaded(true);
    });
    return () => { current = false; };
  }, [fileId, canManage]);

  // The undo offer is short-lived, like on the desktop.
  useEffect(() => {
    if (!undoTarget) return;
    const timer = window.setTimeout(() => { setUndoTarget(null); setNotice(''); }, UNDO_DURATION_MS);
    return () => window.clearTimeout(timer);
  }, [undoTarget]);

  const reload = useCallback(async (): Promise<void> => {
    try {
      const list = await listWebCompanions(fileId, canManage);
      setItems(Array.isArray(list) ? list : []);
      setLoadError(null);
    } catch (e) { setLoadError(toAppError(e)); }
  }, [fileId, canManage]);

  // One action at a time; afterwards the list is read again and the focus moves to the heading,
  // because the button that was used is gone.
  const act = async (run: () => Promise<void>) => {
    if (busy) return;
    setBusy(true); setActionError(null); setRestoreError(null);
    try { await run(); } finally {
      await reload();
      setBusy(false);
      headingRef.current?.focus();
    }
  };
  const remove = (item: WebCompanion) => act(async () => {
    try {
      await removeWebCompanion(item.id);
      setNotice(t('companionRemoved').replace('{name}', displayName(item.name)));
      setUndoTarget({ id: item.id, name: item.name });
    } catch (e) { setActionError(toAppError(e)); }
  });
  const restore = (item: { id: string; name: string }) => act(async () => {
    try {
      await restoreWebCompanion(item.id);
      setNotice(t('companionRestored').replace('{name}', displayName(item.name)));
      setUndoTarget(null);
    } catch (e) { setRestoreError(toAppError(e)); }
  });

  // Pictures are fetched one by one when their tile shows up, never with the list.
  const loadImage = useCallback((item: WebCompanion) => {
    if (!item.previewable || requestedImages.current.has(item.id)) return;
    requestedImages.current.add(item.id);
    companionImageData(item.id).then((data) => {
      if (typeof data === 'string' && data.startsWith('data:image/')) setImages((previous) => ({ ...previous, [item.id]: data }));
    }).catch(() => { /* the tile keeps its placeholder; the file can still be downloaded */ });
  }, []);

  const download = (item: WebCompanion) => {
    setActionError(null);
    startDownload(item.id).catch((e) => setActionError(toAppError(e)));
  };

  const headingClass = variant === 'compact' ? 'text-compact-meta ui-label text-[var(--ink-3)]' : 'ui-label text-[var(--ink-3)]';
  const headingStyle = variant === 'comfort' ? { fontSize: 'var(--font-size-label)' } : undefined;
  const wrapperClass = variant === 'compact' ? 'px-4 pt-3.5 pb-3 border-t border-[var(--line)]'
    : variant === 'comfort' ? 'px-[18px] pt-3.5 pb-4 border-t border-[var(--line)]'
    : '';
  const live = items.filter((item) => !item.deletedAt);
  const removed = canManage ? items.filter((item) => item.deletedAt) : [];
  const imageItems = live.filter((item) => item.kind === 'image' && !item.missing);
  const rowItems = live.filter((item) => item.kind !== 'image' || item.missing);

  return (
    <section aria-labelledby={headingId} className={wrapperClass}>
      <h3 id={headingId} ref={headingRef} tabIndex={-1} className={`pb-2 outline-none ${headingClass}`} style={headingStyle}>
        {t('companionsHeading')}{live.length > 0 && ` (${live.length})`}
      </h3>

      {loaded && !loadError && live.length === 0 && (
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
              onVisible={() => loadImage(item)}
              onOpen={(button) => { opener.current = button; loadImage(item); setPreview({ id: item.id, name: item.name }); }}
              onDownload={() => download(item)}
              onRemove={canManage ? () => void remove(item) : undefined}
              busy={busy}
            />
          ))}
        </ul>
      )}

      {rowItems.length > 0 && (
        <ul>
          {rowItems.map((item) => (
            <li key={item.id} className="flex items-center gap-2 py-1.5 border-b border-[var(--line)]">
              <span aria-hidden="true" className="flex-none w-5 text-center">{kindIcon(item)}</span>
              <div className="flex-1 min-w-0">
                <div className="text-small font-medium break-words" style={{ overflowWrap: 'anywhere' }}>{displayName(item.name)}</div>
                <div className="text-compact-meta text-[var(--ink-3)] tabular-nums">
                  {item.missing
                    ? <span className="text-[var(--accent)]">{t('companionMissing')}</span>
                    : item.sizeBytes !== null && formatBytes(item.sizeBytes, language)}
                </div>
              </div>
              {!item.missing && (
                <button type="button" aria-label={t('companionDownloadNamed').replace('{name}', displayName(item.name))}
                  onClick={() => download(item)} className={buttonClass}>
                  {t('companionDownload')}
                </button>
              )}
              {canManage && (
                <button type="button" disabled={busy} aria-label={t('companionRemoveNamed').replace('{name}', displayName(item.name))}
                  onClick={() => void remove(item)} className={buttonClass}>
                  {t('companionRemove')}
                </button>
              )}
            </li>
          ))}
        </ul>
      )}

      {removed.length > 0 && (
        <div className="pt-2">
          <h4 className="pb-1 text-compact-meta ui-label text-[var(--ink-3)]">{t('companionsRemovedToggle').replace('{count}', String(removed.length))}</h4>
          <ul>
            {removed.map((item) => (
              <li key={item.id} className="flex items-center gap-2 py-1.5 border-b border-[var(--line)]">
                <div className="flex-1 min-w-0">
                  <div className="text-small break-words text-[var(--ink-2)]" style={{ overflowWrap: 'anywhere' }}>{displayName(item.name)}</div>
                  {item.restorable === false && <div className="text-compact-meta text-[var(--ink-3)]">{t('companionRestoreAtDesktop')}</div>}
                </div>
                {item.restorable !== false && (
                  <button type="button" disabled={busy} aria-label={t('companionRestoreNamed').replace('{name}', displayName(item.name))}
                    onClick={() => void restore(item)} className={buttonClass}>
                    {t('companionRestore')}
                  </button>
                )}
              </li>
            ))}
          </ul>
        </div>
      )}

      {notice && (
        <p role="status" className="pt-2 text-small text-[var(--ink-2)] break-words">
          {notice}
          {undoTarget && (
            <>
              {' '}
              <button type="button" disabled={busy} onClick={() => void restore(undoTarget)}
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

      {preview && (
        <ImageLightbox name={preview.name} src={images[preview.id]} onClose={() => setPreview(null)} returnFocus={opener} />
      )}
    </section>
  );
}
