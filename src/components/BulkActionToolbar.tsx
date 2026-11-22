import { WebFileActions } from './WebFileActions';
import { canEditMetadata, useCanEditMetadata, useCanManageFiles } from '../lib/webSession';
import { isReadOnly } from '../lib/readOnly';
import { LicenseEditor } from './LicenseEditor';
import { shouldIgnoreCatalogShortcut } from '../lib/keyboardGuard';
import { useImportLock } from '../hooks/ImportLockContext';
import { useEffect, useId, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from 'react';
import { toAppError, type AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';
import type { Collection } from '../types';
import { useLanguage, useT, useFormatCount } from '../i18n/LanguageContext';
import { TagInput } from './TagInput';
import { ViewportPopover } from './ViewportPopover';
import { tagLabel } from '../lib/autoTags';
import { modelMenuActions, type ModelMenuActionId } from '../lib/modelMenuActions';
import { usePrintDataReread } from '../hooks/usePrintDataReread';
import { PrintDataRereadJob } from './PrintDataRereadJob';
import { DateRenameDialog, type DateRenameOutcome } from './DateRenameDialog';

/** Bundle group entries; each handler gets the element focus should return to. */
export interface BulkBundleActions {
  /** Two or more models selected. */
  canCreate: boolean;
  canAddTo: boolean;
  canRemove: boolean;
  /** The selection holds whole bundle cards. */
  canDissolve: boolean;
  /** The selection is exactly one bundle. */
  canEdit: boolean;
  onCreate: (returnTo: HTMLElement | null) => void;
  onAddTo: (returnTo: HTMLElement | null) => void;
  onRemove: () => void;
  onDissolve: () => void;
  onEdit: (returnTo: HTMLElement | null) => void;
}

interface BulkActionToolbarProps {
  selectedCount: number;
  favorite?: boolean | 'mixed';
  queue?: boolean | 'mixed';
  printed?: boolean | 'mixed';
  onBulkToggleFavorite?: () => void;
  onBulkRemoveFromQueue?: () => void;
  selectedNames?: string[];
  hiddenSelectedCount?: number;
  confirmBulkDelete: boolean;
  onConfirmBulkDeleteChange: (value: boolean) => void;
  onSelectAllVisible: () => void;
  onClearSelection: () => void;
  onBulkAddToQueue: () => void;
  addToCollectionMenuOpen: boolean;
  onAddToCollectionMenuOpenChange: (value: boolean) => void;
  collections: Collection[];
  onBulkAddToCollection: (collectionId: string) => void;
  activeCollection: string | null;
  onBulkRemoveFromCollection: () => void;
  onBulkSetLicense?: (license: string | null) => Promise<void>;
  onBulkSetPrintStatus: (status: 'printed' | 'not_printed') => void;
  onBulkDelete: () => void;
  onBulkMove?: () => void;
  onBulkRemove?: () => Promise<void>;
  addTagMenuOpen: boolean;
  onAddTagMenuOpenChange: (value: boolean) => void;
  allTags: string[];
  tagsInAllSelection: string[];
  tagDraft: string;
  onTagDraftChange: (value: string) => void;
  onSubmitBulkAddTag: (tag?: string) => void;
  removeTagMenuOpen: boolean;
  onRemoveTagMenuOpenChange: (value: boolean) => void;
  tagsInSelection: string[];
  onBulkRemoveTag: (tag: string) => void;
  /** Ids of the selected models; together with `onDateRenamed` this enables the date rename entry. */
  selectedIds?: string[];
  onDateRenamed?: (outcome: DateRenameOutcome) => void;
  bundleActions?: BulkBundleActions;
}

const BUTTON = 'h-8 px-3 rounded-[3px] border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] text-small font-semibold cursor-pointer hover:text-[var(--ink)] ' +
  'focus-visible:outline focus-visible:outline-[3px] focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)] ' +
  'aria-disabled:opacity-45 aria-disabled:cursor-default aria-disabled:hover:text-[var(--ink-2)] disabled:opacity-50 disabled:cursor-default';
const MENU_ITEM = 'w-full text-left px-3 py-1.5 text-body text-[var(--ink)] hover:bg-[var(--panel-2)] cursor-pointer whitespace-nowrap ' +
  'focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-[var(--accent)] disabled:opacity-50 disabled:cursor-default';
const GROUP = 'flex items-center gap-2 pr-3 mr-1 border-r border-[var(--line-strong)]';

interface MoreItem {
  key: string;
  label: string;
  onSelect: () => void;
  /** Blocked while an import runs, like the former buttons. */
  locked?: boolean;
  /** Reason the entry cannot be used right now; it is disabled and names the reason as its tooltip. */
  disabledReason?: string;
  /** Entries that open a popover or dialog leave focus handling to it. */
  opensOwnUi?: boolean;
}

function navigateListMenu(event: ReactKeyboardEvent<HTMLDivElement>) {
  if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
  const entries = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('button:not(:disabled)'));
  const current = entries.indexOf(document.activeElement as HTMLButtonElement);
  if (current < 0) return;
  event.preventDefault();
  // Stop at the edges so repeated key presses do not jump across a long list.
  const next = entries[Math.max(0, Math.min(entries.length - 1, current + (event.key === 'ArrowDown' ? 1 : -1)))];
  next.focus({ preventScroll: true });
  next.scrollIntoView?.({ block: 'nearest' });
}

function navigateActionMenu(event: ReactKeyboardEvent<HTMLDivElement>) {
  const keys = ['ArrowDown', 'ArrowUp', 'Home', 'End'];
  if (!keys.includes(event.key)) return;
  const entries = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)'));
  if (entries.length === 0) return;
  event.preventDefault();
  const current = entries.indexOf(document.activeElement as HTMLButtonElement);
  const index = event.key === 'Home' ? 0 : event.key === 'End' ? entries.length - 1
    : Math.max(0, Math.min(entries.length - 1, current + (event.key === 'ArrowDown' ? 1 : -1)));
  entries[index].focus({ preventScroll: true });
}

export function BulkActionToolbar({
  selectedCount,
  favorite = false, queue = false, printed = false, onBulkToggleFavorite, onBulkRemoveFromQueue,
  selectedNames,
  hiddenSelectedCount = 0,
  confirmBulkDelete,
  onConfirmBulkDeleteChange,
  onSelectAllVisible,
  onClearSelection,
  onBulkAddToQueue,
  addToCollectionMenuOpen,
  onAddToCollectionMenuOpenChange,
  collections,
  onBulkAddToCollection,
  activeCollection,
  onBulkRemoveFromCollection,
  onBulkSetPrintStatus,
  onBulkSetLicense,
  onBulkDelete,
  onBulkMove,
  onBulkRemove,
  addTagMenuOpen,
  onAddTagMenuOpenChange,
  allTags,
  tagsInAllSelection,
  tagDraft,
  onTagDraftChange,
  onSubmitBulkAddTag,
  removeTagMenuOpen,
  onRemoveTagMenuOpenChange,
  tagsInSelection,
  onBulkRemoveTag,
  selectedIds,
  onDateRenamed,
  bundleActions,
}: BulkActionToolbarProps) {
  const canEdit = useCanEditMetadata();
  const canManage = useCanManageFiles();
  const { lockProps } = useImportLock();
  const { state: rereadState, start: startReread, dismiss: dismissReread } = usePrintDataReread();
  const t = useT();
  const formatCount = useFormatCount();
  const [webMoveOpen, setWebMoveOpen] = useState(false);
  const [licenseOpen, setLicenseOpen] = useState(false);
  const [moreOpen, setMoreOpen] = useState(false);
  const [group, setGroup] = useState<'quick' | 'organize' | 'bundle'>('organize');

  const [confirmRemove, setConfirmRemove] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  // Snapshot at opening: the preview must not reload when the selection prop is rebuilt.
  const [dateRenameIds, setDateRenameIds] = useState<string[] | null>(null);
  const hintId = useId();
  const noSelection = selectedCount === 0;

  const cancelRef = useRef<HTMLButtonElement>(null);
  // Dialogs return focus to the group trigger after their menu entry disappears.
  const moreRef = useRef<HTMLButtonElement>(null);
  const moreWrapRef = useRef<HTMLDivElement>(null);
  const moreMenuRef = useRef<HTMLDivElement>(null);
  const deleteRef = useRef<HTMLButtonElement>(null);
  const collectionRef = moreRef;
  const addTagRef = moreRef;
  const removeTagFocusRef = moreRef;
  const previousConfirmation = useRef<'remove' | 'delete' | null>(null);
  useEffect(() => {
    const current = confirmRemove ? 'remove' : confirmBulkDelete ? 'delete' : null;
    if (current) cancelRef.current?.focus();
    else if (previousConfirmation.current) {
      (previousConfirmation.current === 'remove' ? moreRef : deleteRef).current?.focus();
    }
    previousConfirmation.current = current;
  }, [confirmRemove, confirmBulkDelete]);

  // The result of a read of the chosen files belongs to that selection.
  useEffect(() => {
    if (selectedCount === 0) dismissReread('selection');
  }, [selectedCount, dismissReread]);
  useEffect(() => () => dismissReread('selection'), [dismissReread]);

  // The bar stays mounted without a selection, so whatever was open for the
  // old selection has to close when it is gone.
  useEffect(() => {
    if (selectedCount > 0) return;
    setMoreOpen(false);
    setLicenseOpen(false);
    setConfirmRemove(false);
    if (confirmBulkDelete) onConfirmBulkDeleteChange(false);
    if (addToCollectionMenuOpen) onAddToCollectionMenuOpenChange(false);
    if (addTagMenuOpen) onAddTagMenuOpenChange(false);
    if (removeTagMenuOpen) onRemoveTagMenuOpenChange(false);
  }, [selectedCount, confirmBulkDelete, addToCollectionMenuOpen, addTagMenuOpen, removeTagMenuOpen, onConfirmBulkDeleteChange, onAddToCollectionMenuOpenChange, onAddTagMenuOpenChange, onRemoveTagMenuOpenChange]);

  // The menus stay open until they are closed explicitly; without Escape a
  // stray click after trying to dismiss one lands on a menu entry, e.g. removes a tag.
  const anyMenuOpen = moreOpen || licenseOpen || addToCollectionMenuOpen || addTagMenuOpen || removeTagMenuOpen;
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (shouldIgnoreCatalogShortcut(e, { allowInteractive: anyMenuOpen || confirmRemove || confirmBulkDelete, allowMenus: anyMenuOpen, preserveEscapeSelection: !anyMenuOpen && !confirmRemove && !confirmBulkDelete })) return;
      if (e.key !== 'Escape') return;
      // The dialog handles its own Escape; it must not also clear the selection.
      if (dateRenameIds) return;
      if (anyMenuOpen) {
        (moreOpen || licenseOpen ? moreRef : addToCollectionMenuOpen ? collectionRef : addTagMenuOpen ? addTagRef : removeTagFocusRef).current?.focus();
        setMoreOpen(false);
        setLicenseOpen(false);
        onAddToCollectionMenuOpenChange(false);
        onAddTagMenuOpenChange(false);
        onRemoveTagMenuOpenChange(false);
        return;
      }
      if (confirmRemove) {
        if (!busy) setConfirmRemove(false);
        return;
      }
      if (confirmBulkDelete) {
        onConfirmBulkDeleteChange(false);
        return;
      }
      // Without a selection Escape has nothing to clear and belongs to other panels.
      if (selectedCount > 0) onClearSelection();
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [anyMenuOpen, moreOpen, licenseOpen, addToCollectionMenuOpen, addTagMenuOpen, removeTagFocusRef, selectedCount, confirmRemove, confirmBulkDelete, busy, dateRenameIds, onConfirmBulkDeleteChange, onClearSelection, onAddToCollectionMenuOpenChange, onAddTagMenuOpenChange, onRemoveTagMenuOpenChange]);
  // A confirmation replaces the bar's buttons, so a menu left open behind it
  // would be invisible: Escape would seem to do nothing, and the menu would
  // reappear after cancelling. Opening a confirmation therefore closes the menus.
  const confirmationOpen = confirmRemove || confirmBulkDelete;
  useEffect(() => {
    if (!confirmationOpen) return;
    setMoreOpen(false);
    setLicenseOpen(false);
    onAddToCollectionMenuOpenChange(false);
    onAddTagMenuOpenChange(false);
    onRemoveTagMenuOpenChange(false);
  }, [confirmationOpen, onAddToCollectionMenuOpenChange, onAddTagMenuOpenChange, onRemoveTagMenuOpenChange]);

  // Move focus into the action menu when it opens; a click outside closes it
  // like in a native menu (the other popovers keep their explicit-close rule).
  useEffect(() => {
    if (!moreOpen) return;
    moreMenuRef.current?.querySelector<HTMLButtonElement>('[role="menuitem"]:not(:disabled)')?.focus({ preventScroll: true });
    const onPointerDown = (e: MouseEvent) => {
      if (e.target instanceof Node && !moreWrapRef.current?.contains(e.target)) setMoreOpen(false);
    };
    document.addEventListener('mousedown', onPointerDown);
    return () => document.removeEventListener('mousedown', onPointerDown);
  }, [moreOpen, group]);

  // The entries that open these surfaces disappear with the menu, taking focus with them,
  // so the surface has to take it over. Without that, arrow keys would act on the page.
  useEffect(() => {
    if (!licenseOpen) return;
    moreWrapRef.current?.querySelector<HTMLElement>('[data-license-popover] [role="combobox"]')?.focus({ preventScroll: true });
  }, [licenseOpen]);
  useEffect(() => {
    if (!removeTagMenuOpen) return;
    const popover = moreWrapRef.current?.querySelector<HTMLElement>('[data-remove-tag-popover]');
    (popover?.querySelector<HTMLElement>('button') ?? popover)?.focus({ preventScroll: true });
  }, [removeTagMenuOpen]);
  useEffect(() => {
    if (addToCollectionMenuOpen) moreWrapRef.current?.querySelector<HTMLElement>('[data-collection-popover] button')?.focus();
  }, [addToCollectionMenuOpen]);
  // Picking a tag closes the list and removes the focused entry; focus goes back to the trigger
  // instead of falling to the page. Escape already moves focus itself.
  const wasCollectionOpen = useRef(false);
  useEffect(() => {
    if (wasCollectionOpen.current && !addToCollectionMenuOpen && document.activeElement === document.body) moreRef.current?.focus();
    wasCollectionOpen.current = addToCollectionMenuOpen;
  }, [addToCollectionMenuOpen]);
  const wasRemoveTagOpen = useRef(false);
  useEffect(() => {
    const closed = wasRemoveTagOpen.current && !removeTagMenuOpen;
    wasRemoveTagOpen.current = removeTagMenuOpen;
    const active = document.activeElement;
    if (closed && (!active || active === document.body)) removeTagFocusRef.current?.focus();
  }, [removeTagMenuOpen, removeTagFocusRef]);

  const { language } = useLanguage();

  // Disabled actions stay focusable (aria-disabled) so that a screen reader can
  // find them and read why they are unavailable, and so that focus is not lost
  // when the last selected model is deselected from the bar itself.
  const idle = noSelection ? { 'aria-disabled': true as const, 'aria-describedby': hintId } : {};
  const act = (fn: () => void) => () => { if (!noSelection) fn(); };

  const openGroup = (id: typeof group, toggle = false) => {
    setLicenseOpen(false);
    if (addToCollectionMenuOpen) onAddToCollectionMenuOpenChange(false);
    if (addTagMenuOpen) onAddTagMenuOpenChange(false);
    if (removeTagMenuOpen) onRemoveTagMenuOpenChange(false);
    setGroup(id);
    setMoreOpen(!toggle || group !== id || !moreOpen);
  };

  const closeMoreThen = (item: MoreItem) => () => {
    setMoreOpen(false);
    if (!item.opensOwnUi) moreRef.current?.focus();
    item.onSelect();
  };

  const handlers: Partial<Record<ModelMenuActionId, Omit<MoreItem, 'key' | 'label'>>> = {
    favorite: onBulkToggleFavorite ? { onSelect: onBulkToggleFavorite } : undefined,
    queue: { onSelect: queue === true && onBulkRemoveFromQueue ? onBulkRemoveFromQueue : onBulkAddToQueue },
    printed: { onSelect: () => onBulkSetPrintStatus(printed === true ? 'not_printed' : 'printed') },
    addTag: { opensOwnUi: true, onSelect: () => onAddTagMenuOpenChange(true) },
    removeTag: { opensOwnUi: true, onSelect: () => onRemoveTagMenuOpenChange(true) },
    collection: { opensOwnUi: true, onSelect: () => onAddToCollectionMenuOpenChange(true) },
    uncollect: activeCollection ? { onSelect: onBulkRemoveFromCollection } : undefined,
    license: onBulkSetLicense ? { locked: true, opensOwnUi: true, onSelect: () => setLicenseOpen(true) } : undefined,
    move: isReadOnly() && canManage ? { locked: true, opensOwnUi: true, onSelect: () => setWebMoveOpen(true) } : onBulkMove ? { locked: true, onSelect: onBulkMove } : undefined,
    remove: onBulkRemove ? { locked: true, opensOwnUi: true, onSelect: () => { setError(null); setConfirmRemove(true); } } : undefined,
    dateRename: selectedIds && onDateRenamed ? { locked: true, opensOwnUi: true, onSelect: () => setDateRenameIds([...selectedIds]) } : undefined,
    rereadPrintData: { locked: true, disabledReason: rereadState.status === 'running' || rereadState.status === 'cancelling' ? t('printDataRereadBusy') : undefined, onSelect: () => { startReread('selection', selectedIds ?? []); } },
    bundle: bundleActions ? { opensOwnUi: true, disabledReason: bundleActions.canCreate ? undefined : t('bundleDlgNeedTwo'), onSelect: () => bundleActions.onCreate(moreRef.current) } : undefined,
    addBundle: bundleActions?.canAddTo ? { opensOwnUi: true, onSelect: () => bundleActions.onAddTo(moreRef.current) } : undefined,
    unbundle: bundleActions?.canRemove ? { onSelect: bundleActions.onRemove } : undefined,
    editBundle: bundleActions?.canEdit ? { opensOwnUi: true, onSelect: () => bundleActions.onEdit(moreRef.current) } : undefined,
    dissolveBundle: bundleActions?.canDissolve ? { onSelect: bundleActions.onDissolve } : undefined,
  };
  const definitions = modelMenuActions({ toolbar: true, web: isReadOnly(), count: selectedCount, favorite, queue, printed });
  const trashLabel = t(definitions.find(action => action.id === 'trash')!.label);
  const itemsOf = (id: typeof group) => definitions.filter(action =>
    action.group === id && handlers[action.id]
  ).map(action => ({ ...handlers[action.id]!, key: action.id, label: t(action.label) }));
  const moreItems = itemsOf(group);
  const groups = [ ['quick', 'bulkGroupMark'], ['organize', 'bulkGroupFile'], ['bundle', 'modelMenuBundle'] ] as const;

  const removeTagPopover = removeTagMenuOpen && (
    <ViewportPopover fitHeight data-navigation-menu data-remove-tag-popover tabIndex={-1} onKeyDown={navigateListMenu} className={`absolute top-9 left-0 min-w-[160px] max-w-[300px] max-h-[max(11rem,60vh)] overflow-y-auto overscroll-contain py-1.5 bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)] z-40`}>
      {tagsInSelection.map((tag) => (
        <button
          key={tag}
          onClick={() => onBulkRemoveTag(tag)}
          className="w-full text-left px-3 py-1.5 text-body text-[var(--ink)] hover:bg-[var(--panel-2)] cursor-pointer whitespace-nowrap overflow-hidden text-ellipsis"
        >
          #{tagLabel(tag, language)}
        </button>
      ))}
      {tagsInSelection.length === 0 && (
        <div className="px-3 py-1.5 font-medium tabular-nums text-caption text-[var(--ink-3)]">
          {t('noTagsInSelectionEmptyState')}
        </div>
      )}
    </ViewportPopover>
  );

  if (!canEdit) return null;
  return (
    <div role="toolbar" aria-label={t('bulkToolbarLabel')} className="flex-none flex flex-wrap items-center gap-x-2 gap-y-1.5 px-4 py-2 min-h-12 border-b border-[var(--line)] bg-[var(--panel-2)] [&_button]:whitespace-nowrap">
      {canEdit && confirmRemove ? (
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-small font-medium text-[var(--ink)]">{formatCount(t('removeModelsQuestion'), selectedCount)}</span>
          <span className="text-small text-[var(--ink-2)]">{t(isReadOnly() ? 'webRemoveModelHint' : 'removeModelHint')}</span>
          {error && <span role="alert" className="font-medium tabular-nums text-[length:var(--font-size-meta)] text-[var(--accent)]"><ErrorText error={error} /></span>}
          <button ref={cancelRef} className="h-8 px-3 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-small font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)] disabled:opacity-50" disabled={busy} onClick={() => setConfirmRemove(false)}>{t('cancel')}</button>
          <button className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] h-8 px-3 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-small font-semibold cursor-pointer disabled:opacity-50" disabled={busy} {...lockProps} onClick={async () => {
            if (!onBulkRemove || !canEditMetadata() || busy) return;
            setBusy(true); setError(null);
            try { await onBulkRemove(); setConfirmRemove(false); }
            catch (e) { setError(toAppError(e)); }
            finally { setBusy(false); }
          }}>{t('removeEntry')}</button>
        </div>
      ) : !isReadOnly() && confirmBulkDelete ? (
        <>
          <span className="text-small font-medium text-[var(--ink)]">
            {formatCount(t('bulkDeleteConfirmQuestion'), selectedCount)}
          </span>
          <button
            ref={cancelRef}
            onClick={() => onConfirmBulkDeleteChange(false)}
            className="h-8 px-3 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-small font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            {t('cancel')}
          </button>
          <button {...lockProps}
            onClick={onBulkDelete}
            className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] h-8 px-3 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-small font-semibold cursor-pointer"
          >
            {t('modelMenuTrash')}
          </button>
        </>
      ) : (
        <>
          {noSelection && <span id={hintId} className="sr-only">{t('bulkNeedsSelectionHint')}</span>}
          <div role="group" aria-label={t('bulkGroupSelection')} className={GROUP}>
            {/* Announced politely so the new count is read after each (de)selection. */}
            <span aria-live="polite" className="text-small font-medium text-[var(--ink)] tabular-nums">
              {t('bulkSelectedCount').replace('{count}', String(selectedCount))}
            </span>
            <button type="button" onClick={onSelectAllVisible} className={BUTTON}>
              {t('selectAllLabel')}
            </button>
            <button type="button" onClick={act(onClearSelection)} {...idle} className={BUTTON}>
              {t('clearSelectionLabel')}
            </button>
          </div>
          <div ref={moreWrapRef} className="relative flex items-center gap-2">
            {groups.map(([id, label]) => {
              const empty = itemsOf(id).length === 0;
              return (
                <div key={id} className="relative">
                  <button type="button" ref={element => {
                    if (id === group) moreRef.current = element;
                  }} {...idle} {...(empty ? { disabled: true, title: t('bulkGroupEmpty') } : {})} aria-haspopup="menu" aria-expanded={moreOpen && group === id}
                    onClick={act(() => { openGroup(id, true); })}
                    onKeyDown={event => { if (['ArrowDown', 'ArrowUp'].includes(event.key) && !noSelection) { event.preventDefault(); openGroup(id); } }} className={BUTTON}>{t(label)}</button>
                  {moreOpen && group === id && moreItems.length > 0 && (
                    <ViewportPopover
                      data-navigation-menu role="menu" aria-label={t(label)}
                      // The menu is a single tab stop like a native one: Tab leaves it and closes it.
                      onKeyDown={event => { navigateActionMenu(event); if (event.key === 'Tab') setMoreOpen(false); }}
                      className="absolute top-9 left-0 min-w-[230px] py-1.5 bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)] z-40"
                    >
                      <div ref={moreMenuRef}>
                        {moreItems.map(item => (
                          <div key={item.key}>
                            <button type="button" role="menuitem" tabIndex={-1} {...(item.locked ? lockProps : {})} {...(item.disabledReason ? { disabled: true, title: item.disabledReason } : {})} onClick={closeMoreThen(item)} className={MENU_ITEM}>
                              {item.label}
                            </button>
                          </div>
                        ))}
                      </div>
                    </ViewportPopover>
                  )}
                  {id === 'organize' && <>
              {addTagMenuOpen && (
                <ViewportPopover data-navigation-menu className="absolute top-9 left-0 flex items-center gap-1.5 p-1.5 bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)] z-40">
                  <TagInput
                    allTags={allTags}
                    tags={tagsInAllSelection}
                    value={tagDraft}
                    onChange={onTagDraftChange}
                    onAddTag={onSubmitBulkAddTag}
                    stripHash
                    autoFocus
                    inputClassName="h-7 w-[140px] px-2 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-none text-small"
                  />
                  <button
                    onClick={() => onSubmitBulkAddTag()}
                    className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] h-7 px-2.5 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-caption font-semibold cursor-pointer whitespace-nowrap"
                  >
                    {t('confirmSlicerName')}
                  </button>
                </ViewportPopover>
              )}
              {addToCollectionMenuOpen && (
                <ViewportPopover fitHeight data-navigation-menu data-collection-popover onKeyDown={navigateListMenu} className="absolute top-9 left-0 min-w-[220px] max-w-[360px] max-h-[max(11rem,60vh)] overflow-y-auto overscroll-contain py-1.5 bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)] z-40">
                  {collections.map((c) => (
                    <button
                      key={c.id}
                      title={c.name}
                      onClick={() => {
                        onBulkAddToCollection(c.id);
                        onAddToCollectionMenuOpenChange(false);
                      }}
                      className="w-full text-left px-3 py-1.5 text-body text-[var(--ink)] hover:bg-[var(--panel-2)] cursor-pointer whitespace-nowrap overflow-hidden text-ellipsis"
                    >
                      {c.name}
                    </button>
                  ))}
                  {collections.length === 0 && (
                    <div className="px-3 py-1.5 font-medium tabular-nums text-caption text-[var(--ink-3)]">
                      {t('noCollectionsEmptyState')}
                    </div>
                  )}
                </ViewportPopover>
              )}
                    {licenseOpen && onBulkSetLicense && <ViewportPopover data-navigation-menu data-license-popover className="absolute top-9 right-0 w-80 max-w-[85vw] px-3 bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)] z-40">
              <LicenseEditor value={null} bulk onSave={async value => { await onBulkSetLicense(value); setLicenseOpen(false); moreRef.current?.focus(); }} />
            </ViewportPopover>}
            {removeTagPopover}
                  </>}
                </div>
              );
            })}
          </div>
          <span className="flex-1" />
          <WebFileActions menuAction="move" hideTrigger requestMoveConfirm={webMoveOpen} returnFocus={moreRef} fileIds={selectedIds ?? []} names={selectedNames} hiddenCount={hiddenSelectedCount} onDone={onClearSelection} onConfirmationClosed={() => setWebMoveOpen(false)} />
          <WebFileActions menuAction="trash" menuLabel={trashLabel} fileIds={selectedIds ?? []} names={selectedNames} hiddenCount={hiddenSelectedCount} onDone={onClearSelection} requestTrashConfirm={confirmBulkDelete} onConfirmationClosed={() => onConfirmBulkDeleteChange(false)} onTrashRequest={() => onConfirmBulkDeleteChange(true)} />
          {!isReadOnly() && <button type="button" {...idle} {...lockProps} ref={deleteRef} onClick={act(() => onConfirmBulkDeleteChange(true))} className={`${BUTTON} !border-[var(--line-strong)] !text-red-400 hover:!border-red-400`}>
            {trashLabel}
          </button>}
        </>
      )}
      <div className="basis-full empty:hidden"><PrintDataRereadJob origin="selection" returnFocus={moreRef} /></div>
      {dateRenameIds && onDateRenamed && (
        <DateRenameDialog fileIds={dateRenameIds} returnFocus={moreRef}
          onClose={() => setDateRenameIds(null)}
          onDone={outcome => { setDateRenameIds(null); onDateRenamed(outcome); }} />
      )}
    </div>
  );
}
