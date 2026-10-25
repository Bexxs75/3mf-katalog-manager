import { WebFileActions } from './WebFileActions';
import { canEditMetadata, useCanEditMetadata } from '../lib/webSession';
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
import { Icon } from './Icon';
import { DateRenameDialog, type DateRenameOutcome } from './DateRenameDialog';

interface BulkActionToolbarProps {
  selectedCount: number;
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
  /**
   * Narrow window: only the most used actions stay in the bar and the rest
   * moves into the "More" menu, so the bar keeps one row and its height.
   */
  compact?: boolean;
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
  /** Entries that open a popover or dialog leave focus handling to it. */
  opensOwnUi?: boolean;
  separatorBefore?: boolean;
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
  compact = false,
}: BulkActionToolbarProps) {
  const canEdit = useCanEditMetadata();
  const { lockProps } = useImportLock();
  const t = useT();
  const formatCount = useFormatCount();
  const [licenseOpen, setLicenseOpen] = useState(false);
  const [moreOpen, setMoreOpen] = useState(false);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  // Snapshot at opening: the preview must not reload when the selection prop is rebuilt.
  const [dateRenameIds, setDateRenameIds] = useState<string[] | null>(null);
  const hintId = useId();
  const noSelection = selectedCount === 0;

  const cancelRef = useRef<HTMLButtonElement>(null);
  // The "More" button is the anchor for everything that lives in its menu, so
  // focus always has a place to return to, even after the entry itself is gone.
  const moreRef = useRef<HTMLButtonElement>(null);
  const moreWrapRef = useRef<HTMLDivElement>(null);
  const moreMenuRef = useRef<HTMLDivElement>(null);
  const deleteRef = useRef<HTMLButtonElement>(null);
  const collectionRef = useRef<HTMLButtonElement>(null);
  const addTagRef = useRef<HTMLButtonElement>(null);
  const removeTagRef = useRef<HTMLButtonElement>(null);
  const removeTagFocusRef = compact ? moreRef : removeTagRef;
  const previousConfirmation = useRef<'remove' | 'delete' | null>(null);
  useEffect(() => {
    const current = confirmRemove ? 'remove' : confirmBulkDelete ? 'delete' : null;
    if (current) cancelRef.current?.focus();
    else if (previousConfirmation.current) {
      (previousConfirmation.current === 'remove' ? moreRef : deleteRef).current?.focus();
    }
    previousConfirmation.current = current;
  }, [confirmRemove, confirmBulkDelete]);

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
  }, [moreOpen]);

  // The entries that open these surfaces disappear with the menu, taking focus with them,
  // so the surface has to take it over. Without that, arrow keys would act on the page.
  // The tag list is only reachable through the menu in the compact bar.
  useEffect(() => {
    if (!licenseOpen) return;
    moreWrapRef.current?.querySelector<HTMLElement>('[data-license-popover] select')?.focus({ preventScroll: true });
  }, [licenseOpen]);
  useEffect(() => {
    if (!removeTagMenuOpen || !compact) return;
    const popover = moreWrapRef.current?.querySelector<HTMLElement>('[data-remove-tag-popover]');
    (popover?.querySelector<HTMLElement>('button') ?? popover)?.focus({ preventScroll: true });
  }, [removeTagMenuOpen, compact]);
  // Picking a tag closes the list and removes the focused entry; focus goes back to the trigger
  // instead of falling to the page. Escape already moves focus itself.
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

  const closeMoreThen = (item: MoreItem) => () => {
    setMoreOpen(false);
    if (!item.opensOwnUi) moreRef.current?.focus();
    item.onSelect();
  };

  const moreItems: MoreItem[] = [
    ...(compact ? [
      { key: 'removeTag', label: t('bulkRemoveTagLabel'), opensOwnUi: true, onSelect: () => onRemoveTagMenuOpenChange(true) },
      { key: 'printed', label: t('printedBadge'), onSelect: () => onBulkSetPrintStatus('printed') },
    ] : []),
    ...(onBulkSetLicense && !compact ? [{ key: 'license', label: t('licenseSet'), locked: true, opensOwnUi: true, onSelect: () => setLicenseOpen(true) }] : []),
    { key: 'notPrinted', label: t('notPrintedLabel'), onSelect: () => onBulkSetPrintStatus('not_printed') },
    ...(compact ? [
      ...(onBulkMove ? [{ key: 'move', label: t('moveBulkLabel'), locked: true, onSelect: onBulkMove }] : []),
      { key: 'queue', label: t('addToQueue'), onSelect: onBulkAddToQueue },
      ...(onBulkSetLicense ? [{ key: 'license', label: t('licenseSet'), locked: true, opensOwnUi: true, onSelect: () => setLicenseOpen(true) }] : []),
    ] : []),
    ...(selectedIds && onDateRenamed ? [{ key: 'dateRename', label: t('dateRenameMenu'), locked: true, opensOwnUi: true, onSelect: () => setDateRenameIds([...selectedIds]) }] : []),
    ...(activeCollection ? [{ key: 'removeFromCollection', label: t('removeFromCollectionLabel'), onSelect: onBulkRemoveFromCollection }] : []),
    ...(onBulkRemove ? [{ key: 'removeCatalog', label: t('removeCatalogShort'), locked: true, opensOwnUi: true, separatorBefore: true, onSelect: () => { setError(null); setConfirmRemove(true); } }] : []),
  ];

  const removeTagPopover = removeTagMenuOpen && (
    <ViewportPopover fitHeight data-navigation-menu data-remove-tag-popover tabIndex={-1} onKeyDown={navigateListMenu} className={`absolute top-9 ${compact ? 'right-0' : 'left-0'} min-w-[160px] max-w-[300px] max-h-[max(11rem,60vh)] overflow-y-auto overscroll-contain py-1.5 bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)] z-40`}>
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
            {t('delete')}
          </button>
        </>
      ) : (
        <>
          {noSelection && <span id={hintId} className="sr-only">{t('bulkNeedsSelectionHint')}</span>}
          <div role="group" aria-label={t('bulkGroupSelection')} className={GROUP}>
            {!compact && <span aria-hidden="true" className="ui-label text-[var(--ink-3)]">{t('bulkGroupSelection')}</span>}
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
          <div role="group" aria-label={t('bulkGroupEdit')} className={GROUP}>
            {!compact && <span aria-hidden="true" className="ui-label text-[var(--ink-3)]">{t('bulkGroupEdit')}</span>}
            <div className="relative">
              <button
                type="button" ref={addTagRef} onClick={act(() => onAddTagMenuOpenChange(!addTagMenuOpen))} {...idle}
                aria-haspopup="true" aria-expanded={addTagMenuOpen}
                className={BUTTON}
              >
                {t('bulkAddTagLabel')}
              </button>
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
            </div>
            {!compact && <div className="relative">
              <button
                type="button" ref={removeTagRef} onClick={act(() => onRemoveTagMenuOpenChange(!removeTagMenuOpen))} {...idle}
                aria-haspopup="true" aria-expanded={removeTagMenuOpen}
                className={BUTTON}
              >
                {t('bulkRemoveTagLabel')}
              </button>
              {removeTagPopover}
            </div>}
            <div className="relative">
              <button
                type="button" ref={collectionRef} onClick={act(() => onAddToCollectionMenuOpenChange(!addToCollectionMenuOpen))} {...idle}
                aria-haspopup="true" aria-expanded={addToCollectionMenuOpen}
                className={BUTTON}
              >
                {t('addToCollectionLabel')}
              </button>
              {addToCollectionMenuOpen && (
                <ViewportPopover fitHeight data-navigation-menu onKeyDown={navigateListMenu} className="absolute top-9 left-0 min-w-[220px] max-w-[360px] max-h-[max(11rem,60vh)] overflow-y-auto overscroll-contain py-1.5 bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)] z-40">
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
            </div>
            {!compact && <button type="button" onClick={act(() => onBulkSetPrintStatus('printed'))} {...idle} className={BUTTON}>
              {t('printedBadge')}
            </button>}
          </div>
          {!compact && <div role="group" aria-label={t('bulkGroupFile')} className={GROUP}>
            <span aria-hidden="true" className="ui-label text-[var(--ink-3)]">{t('bulkGroupFile')}</span>
            {onBulkMove && <button type="button" {...idle} {...lockProps} onClick={act(onBulkMove)} className={BUTTON}>{t('moveBulkLabel')}</button>}
            <button type="button" onClick={act(onBulkAddToQueue)} {...idle} className={BUTTON}>
              {t('addToQueue')}
            </button>
          </div>}
          <span className="flex-1" />
          <div ref={moreWrapRef} className="relative">
            <button
              type="button" ref={moreRef} {...idle}
              aria-haspopup="menu" aria-expanded={moreOpen}
              onClick={act(() => setMoreOpen(open => !open))}
              onKeyDown={event => {
                if (event.key === 'ArrowDown' && !noSelection) { event.preventDefault(); setMoreOpen(true); }
              }}
              className={`${BUTTON} inline-flex items-center gap-1.5`}
            >
              {t('bulkMoreLabel')} <Icon name="chevron" size={14} />
            </button>
            {moreOpen && (
              <ViewportPopover
                data-navigation-menu role="menu" aria-label={t('bulkMoreMenuLabel')}
                // The menu is a single tab stop like a native one: Tab leaves it and closes it.
                onKeyDown={event => { navigateActionMenu(event); if (event.key === 'Tab') setMoreOpen(false); }}
                className="absolute top-9 right-0 min-w-[230px] py-1.5 bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)] z-40"
              >
                <div ref={moreMenuRef}>
                  {moreItems.map(item => (
                    <div key={item.key}>
                      {item.separatorBefore && <div role="separator" className="my-1 border-t border-[var(--line)]" />}
                      <button type="button" role="menuitem" tabIndex={-1} {...(item.locked ? lockProps : {})} onClick={closeMoreThen(item)} className={MENU_ITEM}>
                        {item.label}
                      </button>
                    </div>
                  ))}
                </div>
              </ViewportPopover>
            )}
            {licenseOpen && onBulkSetLicense && <ViewportPopover data-navigation-menu data-license-popover className="absolute top-9 right-0 w-80 max-w-[85vw] px-3 bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)] z-40">
              <LicenseEditor value={null} bulk onSave={async value => { await onBulkSetLicense(value); setLicenseOpen(false); moreRef.current?.focus(); }} />
            </ViewportPopover>}
            {compact && removeTagPopover}
          </div>
          <span aria-hidden="true" className="w-px h-6 mx-1 bg-[var(--line-strong)]" />
          <WebFileActions fileIds={selectedIds ?? []} names={selectedNames} hiddenCount={hiddenSelectedCount} onDone={onClearSelection} requestTrashConfirm={confirmBulkDelete} onConfirmationClosed={() => onConfirmBulkDeleteChange(false)} />
          {!isReadOnly() && <button type="button" {...idle} {...lockProps} ref={deleteRef} onClick={act(() => onConfirmBulkDeleteChange(true))} className={`${BUTTON} !border-[var(--line-strong)] !text-red-400 hover:!border-red-400`}>
            {t('delete')}
          </button>}
        </>
      )}
      {dateRenameIds && onDateRenamed && (
        <DateRenameDialog fileIds={dateRenameIds} returnFocus={moreRef}
          onClose={() => setDateRenameIds(null)}
          onDone={outcome => { setDateRenameIds(null); onDateRenamed(outcome); }} />
      )}
    </div>
  );
}
