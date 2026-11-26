import { WebFileActions } from './WebFileActions';
import { WebRemoveFromCatalogButton } from './WebRemoveFromCatalogButton';
import { LicenseEditor } from './LicenseEditor';
import { modelMenuActions, type ModelMenuActionId } from '../lib/modelMenuActions';
import { DownloadFileButton } from './DownloadFileButton';
import { useCanEditMetadata } from '../lib/webSession';
import { displayName } from '../lib/displayName';
import { useRuntimeEnvironment } from '../hooks/useRuntimeEnvironment';
import { RevealFileButton } from './RevealFileButton';
import { useStlEntry } from '../hooks/useStlEntry';
import { useImportLock } from '../hooks/ImportLockContext';
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { useT } from '../i18n/LanguageContext';
import { splitFileName } from '../lib/fileName';
import { toAppError, type AppError } from '../lib/errors';
import { useFormatError } from '../diagnostics/useFormatError';
import { isSlicedOutput } from '../lib/fileKind';
import { ReportProblemLink } from '../diagnostics/ReportProblemLink';

/** Bundle entries of the file menu; an action that does not apply to the selection is left out. */
export interface BundleMenuActions {
  onCreate?: () => void;
  onAddTo?: () => void;
  onRemove?: () => void;
}

const MENU_BUTTON = 'block w-full text-left px-3 py-2 text-[length:var(--font-size-title)] text-[var(--ink)] cursor-pointer hover:bg-[var(--panel-2)]';

interface Props {
  initialView?: 'move' | 'menu' | 'rename' | 'confirmRemove' | 'confirmDelete';
  fileIds?: string[];
  favorite?: boolean | 'mixed';
  onToggleFavorite?: () => void;
  organize?: {
    tags: string[];
    collections: { id: string; name: string }[];
    onAddTag: (tag: string) => unknown;
    onRemoveTag: (tag: string) => unknown;
    onCollection: (id: string) => Promise<void>;
    onUncollect?: () => Promise<void>;
    onLicense: (license: string | null) => Promise<void>;
  };
  x: number;
  y: number;
  onClose: () => void;
  onOpenInSlicer: () => void;
  onDelete: () => void;
  onRemove?: (password?: string) => Promise<void>;
  inQueue: boolean | 'mixed';
  onToggleQueue: () => void;
  printed: boolean | 'mixed';
  onTogglePrintStatus: () => void;
  fileId: string;
  currentName: string;
  onRename: (newName: string) => Promise<void>;
  /** Opens the "Move to ..." dialog; omitted when the caller cannot move files. */
  onMove?: () => void;
  /** Opens the plate overview; omitted for files that have no plates (not a 3MF). */
  onOpenPlateOverview?: () => void;
  /** Opens the "Save as STL" dialog (kept by the caller, because the menu closes with the click); omitted where it does not apply. */
  onSaveAsStl?: () => void;
  /** Type and name of the file, for deciding whether "Save as STL" is offered. */
  stlModel?: { fileType?: string; name?: string; path?: string };
  bundleActions?: BundleMenuActions;
}

type View = 'menu' | 'confirmDelete' | 'confirmRemove' | 'rename' | 'addTag' | 'removeTag' | 'collection' | 'license';

function ModelContextMenu({
  initialView = 'menu', fileIds, favorite = false, onToggleFavorite, organize,
  x,
  y,
  onClose,
  onOpenInSlicer,
  onDelete,
  onRemove,
  inQueue,
  onToggleQueue,
  printed,
  onTogglePrintStatus,
  fileId,
  currentName,
  onRename,
  onMove,
  onOpenPlateOverview,
  onSaveAsStl,
  stlModel,
  bundleActions,
}: Props) {
  const web = import.meta.env.VITE_TARGET === 'web';
  const canEdit = useCanEditMetadata();
  const ids = fileIds ?? [fileId];
  const count = ids.length;
  const { lockProps } = useImportLock();
  const { container } = useRuntimeEnvironment();
  const t = useT();
  const stlState = useStlEntry(onSaveAsStl && stlModel ? stlModel : {});
  const formatError = useFormatError();
  const [submenu, setSubmenu] = useState<'organize' | 'bundle' | null>(null);
  const [tag, setTag] = useState('');
  const submenuTrigger = useRef<HTMLButtonElement | null>(null);
  const submenuRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => { if (submenu) submenuRef.current?.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus(); }, [submenu]);
  const closeSubmenu = () => { setSubmenu(null); submenuTrigger.current?.focus(); };
  const [view, setView] = useState<View>(web || initialView === 'move' ? 'menu' : initialView);
  const [baseNameDraft, setBaseNameDraft] = useState(initialView === 'rename' ? splitFileName(currentName).base : '');
  // Derived when the rename view opens; the extension is not editable.
  const [extension, setExtension] = useState(initialView === 'rename' ? splitFileName(currentName).extension : '');
  // Backend failures stay structured until shown, so the text follows the chosen language.
  const [menuError, setMenuError] = useState<{ text: string } | { error: AppError; renameFailed: boolean } | null>(null);
  const renameUnexpected = menuError !== null && 'error' in menuError && menuError.error.unexpected;
  const renameError = menuError === null ? null
    : 'text' in menuError ? menuError.text
    : `${menuError.renameFailed ? `${t('renameError')} ` : ''}${formatError(menuError.error)}`;
  const [renaming, setRenaming] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const shortcut = { rename: 'F2', move: 'F6', confirmRemove: 'Delete', confirmDelete: 'Shift+Delete', menu: null }[initialView];
    if (web && shortcut) ref.current?.querySelector<HTMLButtonElement>(`[aria-keyshortcuts="${shortcut}"]`)?.click();
  }, []);

  useEffect(() => {
    ref.current?.querySelector<HTMLElement>(view === 'rename' || view === 'addTag' ? 'input' : 'button:not(:disabled)')?.focus();
  }, [view]);

  const submitRename = () => {
    if (lockProps.disabled) return;
    const trimmedBase = baseNameDraft.trim();
    if (!trimmedBase) {
      setMenuError({ text: t('renameEmptyNameError') });
      return;
    }
    const fullName = `${trimmedBase}${extension}`;
    if (fullName === currentName) {
      onClose();
      return;
    }
    setRenaming(true);
    setMenuError(null);
    onRename(fullName)
      .then(() => onClose())
      .catch((e) => {
        setRenaming(false);
        setMenuError({ error: toAppError(e), renameFailed: true });
      });
  };

  const suffix = count > 1 ? ` ${t('modelMenuCount').replace('{count}', String(count))}` : '';
  const run = async (action: () => unknown) => {
    if (renaming) return;
    setRenaming(true); setMenuError(null);
    try { const result = action(); if (result instanceof Promise) await result; onClose(); }
    catch (error) { setMenuError({ error: toAppError(error), renameFailed: false }); }
    finally { setRenaming(false); }
  };
  const callbacks: Partial<Record<ModelMenuActionId, () => unknown>> = {
    ...(canEdit ? {
      favorite: onToggleFavorite, queue: onToggleQueue, printed: onTogglePrintStatus,
      addTag: organize ? () => setView('addTag') : undefined,
      removeTag: organize?.tags.length ? () => setView('removeTag') : undefined,
      collection: organize?.collections.length ? () => setView('collection') : undefined,
      uncollect: organize?.onUncollect, license: organize ? () => setView('license') : undefined,
      bundle: bundleActions?.onCreate, addBundle: bundleActions?.onAddTo, unbundle: bundleActions?.onRemove,
      remove: onRemove ? () => { setMenuError(null); setView('confirmRemove'); } : undefined,
    } : {}),
    ...(!web ? {
      slicer: !container && !(stlModel?.path && isSlicedOutput({ path: stlModel.path, fileType: stlModel.fileType })) ? onOpenInSlicer : undefined,
      stl: stlState !== 'hidden' ? onSaveAsStl : undefined,
      rename: () => { const parts = splitFileName(currentName); setBaseNameDraft(parts.base); setExtension(parts.extension); setView('rename'); },
      move: onMove, trash: () => setView('confirmDelete'),
    } : {}),
    plates: onOpenPlateOverview,
  };
  const entries = modelMenuActions({ web, count, favorite, queue: inQueue, printed });
  const localViews = new Set<ModelMenuActionId>(['rename', 'remove', 'trash', 'addTag', 'removeTag', 'collection', 'license']);
  const importLockedIds = new Set<ModelMenuActionId>(['rename', 'move', 'remove', 'trash']);
  const renderAction = (entry: typeof entries[number]) => {
    const label = t(entry.label) + suffix;
    const shortcut = entry.shortcutLabel ? t(entry.shortcutLabel) : entry.shortcut;
    const content = <><span>{label}</span>{shortcut && <span aria-hidden="true" className="ml-auto pl-5 text-[var(--ink-3)]">{shortcut}</span>}</>;
    if (entry.id === 'download') return <DownloadFileButton key={entry.id} fileId={fileId} fileIds={ids} label={label} onSuccess={onClose} className={MENU_BUTTON} />;
    if (entry.id === 'reveal') return <RevealFileButton key={entry.id} fileId={fileId} fileIds={ids} label={label} onSuccess={onClose} showIcon={false} className={MENU_BUTTON} />;
    if (web && entry.id === 'remove') return <WebRemoveFromCatalogButton key={entry.id} name={currentName} count={count} menuLabel={content} menuShortcut={entry.shortcut} onRemove={onRemove} onRemoved={onClose} className={`${MENU_BUTTON} flex items-center`} />;
    if (web && ['rename', 'move', 'trash'].includes(entry.id)) return <WebFileActions key={entry.id} fileIds={ids} name={currentName} onDone={onClose}
      menuAction={entry.id as 'rename' | 'move' | 'trash'} menuLabel={content} menuShortcut={entry.shortcut}
      menuClassName={`${MENU_BUTTON} flex items-center ${entry.id === 'trash' ? '!text-[var(--crit)]' : ''}`} />;
    const action = callbacks[entry.id];
    if (!action) return null;
    const importLocked = importLockedIds.has(entry.id);
    return <button key={entry.id} role={entry.group === 'organize' || entry.group === 'bundle' ? 'menuitem' : undefined} data-model-action={entry.id} aria-label={label} aria-keyshortcuts={entry.shortcut}
      aria-pressed={entry.checked} disabled={renaming || (importLocked && lockProps.disabled) || (entry.id === 'stl' && stlState === 'disabled')}
      title={(importLocked ? lockProps.title : undefined) ?? (entry.id === 'stl' && stlState === 'disabled' ? t('stStepNoPreview') : undefined)}
      className={`${MENU_BUTTON} flex items-center gap-1 ${entry.group === 'quick' ? '!w-auto flex-1 !px-2 !text-caption max-w-[110px]' : ''} disabled:opacity-50 ${entry.id === 'trash' ? '!text-[var(--crit)]' : ''} ${entry.checked === true ? 'bg-[var(--accent-soft)] border-b-2 border-[var(--accent)]' : entry.checked === 'mixed' ? 'bg-[var(--panel-2)] border-b-2 border-dashed border-[var(--accent)]' : ''}`}
      onClick={() => { if (localViews.has(entry.id)) { setSubmenu(null); action(); } else void run(action); }}>{content}</button>;
  };
  const renderSubmenu = (group: 'organize' | 'bundle', label: string) => {
    const actions = entries.filter(entry => entry.group === group && callbacks[entry.id]);
    if (!actions.length) return null;
    return <div>
      <button className={MENU_BUTTON} aria-haspopup="menu" aria-expanded={submenu === group}
        onKeyDown={event => { if (event.key === 'ArrowRight') { event.preventDefault(); event.stopPropagation(); submenuTrigger.current = event.currentTarget; setSubmenu(group); } }}
        onClick={event => { submenuTrigger.current = event.currentTarget; setSubmenu(submenu === group ? null : group); }}>{label}{suffix}</button>
      {submenu === group && <div ref={submenuRef} role="menu" aria-label={label} className="ml-3 border-l border-[var(--line-strong)]" onKeyDown={event => {
        if (event.key === 'ArrowLeft' || event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); closeSubmenu(); }
      }}>{actions.map(renderAction)}</div>}
    </div>;
  };

  return (
    <ContextMenuFrame x={x} y={y} onClose={onClose} onEscape={() => {
      if (renaming) return;
      if (submenu) closeSubmenu();
      else if (view === 'menu') onClose();
      else { setView('menu'); setMenuError(null); }
    }}>
    <div ref={ref} onKeyDown={event => {
      if (!ref.current?.contains(event.target as Node)) return;
      if (view !== 'menu' || event.target instanceof HTMLInputElement || event.target instanceof HTMLSelectElement) return;
      if (event.key === 'ArrowDown' || event.key === 'ArrowUp' || event.key === 'Home' || event.key === 'End') {
        const root = submenu ? submenuRef.current : ref.current;
        const buttons = Array.from(root?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? []);
        const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
        const next = event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length - 1 : (index + (event.key === 'ArrowDown' ? 1 : -1) + buttons.length) % buttons.length;
        event.preventDefault(); event.stopPropagation(); buttons[next]?.focus();
      }
      if (!event.altKey && !event.ctrlKey && !event.metaKey) {
        const shortcut = `${event.shiftKey ? 'Shift+' : ''}${event.key}`;
        const button = Array.from(ref.current?.querySelectorAll<HTMLButtonElement>('button[aria-keyshortcuts]') ?? []).find(button => button.getAttribute('aria-keyshortcuts') === shortcut);
        if (button) { event.preventDefault(); event.stopPropagation(); button.click(); }
      }
    }}>
      {view === 'confirmRemove' ? (
        <div className="px-3 py-2.5 max-w-[260px]" aria-busy={renaming}>
          <div className="text-small font-medium text-[var(--ink)] pb-1">{count > 1 ? t('modelMenuRemoveQuestion').replace('{count}', String(count)) : t('removeModelQuestion').replace('{name}', displayName(currentName))}</div>
          <div className="text-caption leading-snug text-[var(--ink-2)] pb-2">{t(web ? 'webRemoveModelHint' : 'removeModelHint')}</div>
          {renameError && <div role="alert" className="pb-1.5 font-medium tabular-nums text-compact-meta text-[var(--accent)] break-words">{renameError}{renameUnexpected && <> <ReportProblemLink /></>}</div>}
          <div className="flex gap-1.5">
            <button className="flex-1 h-7 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-caption font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)] disabled:opacity-50" disabled={renaming} onClick={() => setView('menu')}>{t('cancel')}</button>
            <button className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] flex-1 h-7 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-caption font-semibold cursor-pointer disabled:opacity-50" disabled={renaming} {...lockProps} onClick={async () => {
              if (!onRemove) return;
              setRenaming(true); setMenuError(null);
              try { await onRemove(); onClose(); }
              catch (e) { setMenuError({ error: toAppError(e), renameFailed: false }); setRenaming(false); }
            }}>{t('removeEntry')}</button>
          </div>
        </div>
      ) : view === 'confirmDelete' ? (
        <div className="px-3 py-2.5">
          <div className="text-small font-medium text-[var(--ink)] pb-2">{t('modelMenuTrashQuestion').replace('{count}', String(count))}</div>
          <div className="flex gap-1.5">
            <button
              onClick={() => setView('menu')}
              className="flex-1 h-7 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-caption font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
            >
              {t('cancel')}
            </button>
            <button {...lockProps}
              onClick={() => {
                void run(onDelete);
              }}
              className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] flex-1 h-7 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-caption font-semibold cursor-pointer"
            >
              {t('modelMenuTrash')}
            </button>
          </div>
        </div>
      ) : view === 'rename' ? (
        <div className="px-3 py-2.5 w-[260px] max-w-[calc(100vw-24px)] [overflow-wrap:anywhere]">
          <div className="flex items-center gap-1 mb-1.5">
            <input
              value={baseNameDraft}
              onChange={(e) => setBaseNameDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') submitRename();
                if (e.key === 'Escape') {
                  e.stopPropagation();
                  setView('menu');
                  setMenuError(null);
                }
              }}
              autoFocus
              disabled={renaming}
              className="flex-1 min-w-0 h-7 px-2 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-none text-small"
            />
            {extension && (
              <span
                title={t('renameExtensionLockedHint')}
                className="flex-none font-medium tabular-nums text-small text-[var(--ink-3)]"
              >
                {displayName(extension)}
              </span>
            )}
          </div>
          {renameError && (
            <div className="pb-1.5 font-medium tabular-nums text-compact-meta text-[var(--accent)] break-words">
              {renameError}{renameUnexpected && <> <ReportProblemLink /></>}
            </div>
          )}
          <div className="flex gap-1.5">
            <button
              onClick={() => {
                setView('menu');
                setMenuError(null);
              }}
              disabled={renaming}
              className="flex-1 h-7 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-caption font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
            >
              {t('cancel')}
            </button>
            <button
              onClick={submitRename}
              disabled={renaming} {...lockProps}
              className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] flex-1 h-7 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-caption font-semibold cursor-pointer"
            >
              {t('confirmSlicerName')}
            </button>
          </div>
        </div>
      ) : view !== 'menu' ? (
        <div className="px-3 py-2 max-w-[320px]">
          <button className={MENU_BUTTON} disabled={renaming} onClick={() => setView('menu')}>{t('cancel')}</button>
          {view === 'addTag' && <form onSubmit={event => { event.preventDefault(); if (tag.trim() && organize) void run(() => organize.onAddTag(tag.trim())); }}>
            <label>{t('bulkAddTagLabel')}<input autoFocus maxLength={100} value={tag} onChange={event => setTag(event.target.value)} className="w-full border border-[var(--line)] bg-[var(--panel)]" /></label>
            <button className={MENU_BUTTON} disabled={!tag.trim() || renaming}>{t('confirmSlicerName')}</button>
          </form>}
          {view === 'removeTag' && organize?.tags.map(tag => <button className={MENU_BUTTON} key={tag} disabled={renaming} onClick={() => void run(() => organize.onRemoveTag(tag))}>{tag}</button>)}
          {view === 'collection' && organize?.collections.map(collection => <button className={MENU_BUTTON} key={collection.id} disabled={renaming} onClick={() => void run(() => organize.onCollection(collection.id))}>{collection.name}</button>)}
          {view === 'license' && organize && <LicenseEditor value={null} bulk onSave={async license => { await organize.onLicense(license); onClose(); }} />}
        </div>
      ) : (
        <>
          <div role="group" className="flex border-b border-[var(--line)]">{entries.filter(entry => entry.group === 'quick').map(renderAction)}</div>
          {entries.filter(entry => entry.group === 'files').map(renderAction)}
          {renderSubmenu('organize', t('modelMenuOrganize'))}
          {renderSubmenu('bundle', t('modelMenuBundle'))}
          <div className="border-t border-[var(--line)]" />
          {entries.filter(entry => entry.group === 'end').map(renderAction)}
        </>
      )}
      {renameError && view !== 'rename' && view !== 'confirmRemove' && <div role="alert" className="px-3 py-2">{renameError}</div>}
    </div>
    </ContextMenuFrame>
  );
}

interface CollectionMenuProps {
  x: number;
  y: number;
  onClose: () => void;
  collectionActions: { onRename: () => void; onDelete: () => void };
}

export function ContextMenu(props: Props | CollectionMenuProps) {
  const canEdit = useCanEditMetadata();
  const { lockProps } = useImportLock();
  const t = useT();
  const isWeb = import.meta.env.VITE_TARGET === 'web';
  if (isWeb && !('collectionActions' in props)) return <ModelContextMenu {...props} />;
  if (!canEdit) return null;
  if (!('collectionActions' in props)) return <ModelContextMenu {...props} />;
  return <ContextMenuFrame x={props.x} y={props.y} onClose={props.onClose}>
    <button role="menuitem" tabIndex={-1} {...lockProps} onClick={props.collectionActions.onRename}
      className="w-full text-left px-3 py-2 text-[length:var(--font-size-title)] text-[var(--ink)] cursor-pointer hover:bg-[var(--panel-2)] disabled:opacity-50">{t('renameLabel')}</button>
    <button role="menuitem" tabIndex={-1} {...lockProps} onClick={props.collectionActions.onDelete}
      className="w-full text-left px-3 py-2 text-[length:var(--font-size-title)] text-[var(--crit)] cursor-pointer hover:bg-[var(--panel-2)] disabled:opacity-50">{t('delete')}</button>
  </ContextMenuFrame>;
}

function isWebConfirmationOpen() {
  return import.meta.env.VITE_TARGET === 'web' && document.querySelector('[role="dialog"][aria-modal="true"]') !== null;
}

/** Keeps menus (and their expanded submenus) off the viewport edges. */
const MENU_MARGIN = 8;

export function ContextMenuFrame({ x, y, onClose, onEscape = onClose, children }: {
  x: number; y: number; onClose: () => void; onEscape?: () => void; children: ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ left: x, top: y });
  useLayoutEffect(() => {
    const menu = ref.current;
    if (!menu) return;
    const bounds = menu.getBoundingClientRect();
    setPosition({ left: Math.max(MENU_MARGIN, Math.min(x, window.innerWidth - MENU_MARGIN - bounds.width)),
      top: Math.max(MENU_MARGIN, Math.min(y, window.innerHeight - MENU_MARGIN - bounds.height)) });
  }, [x, y, children]);
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    ref.current?.querySelector<HTMLElement>('button:not(:disabled)')?.focus();
    return () => { if (previous?.isConnected) previous.focus(); };
  }, []);
  useEffect(() => {
    const pointer = (event: MouseEvent) => {
      // A web removal confirmation is portalled outside this menu.
      if (isWebConfirmationOpen()) return;
      if (ref.current && !ref.current.contains(event.target as Node)) onClose();
    };
    const key = (event: KeyboardEvent) => {
      if (isWebConfirmationOpen()) return;
      if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); onEscape(); }
    };
    document.addEventListener('mousedown', pointer);
    document.addEventListener('keydown', key);
    return () => { document.removeEventListener('mousedown', pointer); document.removeEventListener('keydown', key); };
  }, [onClose, onEscape]);
  return <div data-navigation-menu ref={ref}
    className="fixed z-50 min-w-[172px] max-w-[calc(100vw-16px)] max-h-[calc(100vh-16px)] rounded-[4px] border border-[var(--line-strong)] bg-[var(--panel)] shadow-lg overflow-auto"
    style={position}>{children}</div>;
}
