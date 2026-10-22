import { canEditMetadata, useCanEditMetadata } from '../lib/webSession';
import { invalidateSpoolImage } from '../lib/spoolImages';
import type { PrinterNavigation } from '../types';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '../lib/transport';
import { useLanguage, useT, useFormatCount } from '../i18n/LanguageContext';
import { formatVolumeMl, formatWeightG } from '../i18n/format';

import type { FilamentSpool, SpoolKind } from '../types';
import type { PrinterLinkState } from '../hooks/usePrinterLink';
import { filamentStockStatus } from '../lib/filamentStatus';
import { groupFilaments, singleSpoolGroups, type SpoolGroup } from '../lib/filamentGroups';
import { restockSpoolLabel, roundTenth } from '../lib/filamentRestock';
import { loadLayout, saveLayout, type LayoutMode } from '../lib/layoutPreference';
import { loadSpoolKind, saveSpoolKind } from '../lib/spoolKindPreference';
import { loadGroupMode, saveGroupMode, type GroupMode } from '../lib/groupModePreference';
import { FilamentDashboard } from './FilamentDashboard';
import { FilamentTable } from './FilamentTable';
import { FilamentSpoolForm } from './FilamentSpoolForm';
import { PrinterColumn } from './PrinterColumn';
import { PrinterJobsBanner } from './PrinterJobsBanner';
import { PrinterJobsDialog } from './PrinterJobsDialog';
import { SpoolToast } from './SpoolToast';
import { RestockPopover } from './RestockPopover';
import { ConsumeResinPopover } from './ConsumeResinPopover';
import { SegmentedControl } from './SegmentedControl';
import { SpoolColorDot } from './SpoolColorDot';
import type { PrintersState } from '../hooks/usePrinters';
import { useSpoolDragAndDrop } from '../hooks/useSpoolDragAndDrop';
import * as printersApi from '../lib/api/printers';
import { isInStorage, spoolFitsUnit, spoolLabel } from '../lib/filamentSlots';
import { toAppError, type AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';

type StatusFilter = 'low' | 'empty' | null;

/** How long newly created entries keep their green outline. */
const HIGHLIGHT_MS = 2500;

type PopoverType = 'restock' | 'consume';
type PopoverState = { type: PopoverType; spool: FilamentSpool; anchor: HTMLElement; choices?: FilamentSpool[] };

type ToastState =
  | { type: 'unload'; spoolId: string; label: string; location: string | null }
  | { type: 'message'; label: string };

interface Props {
  printerContext?: PrinterNavigation;
  onPrinterManager?: (printerId?: string) => void;
  printerLink: PrinterLinkState;
  /** Shared instance with Printer Manager so changes show up everywhere immediately. */
  printers: PrintersState;
  /** Reload the catalog (e.g. print status/queue) after prints are confirmed. */
  onCatalogChanged?: () => void;
}

export function FilamentView({ printerLink, printers, onCatalogChanged, printerContext, onPrinterManager }: Props) {
  const t = useT();
  const canEdit = useCanEditMetadata();
  const web = import.meta.env.VITE_TARGET === 'web';
  const formatCount = useFormatCount();
  const { language } = useLanguage();
  const [spools, setSpools] = useState<FilamentSpool[]>([]);
  const [error, setError] = useState<AppError | null>(null);
  const [layout, setLayout] = useState<LayoutMode>(loadLayout);
  const [groupMode, setGroupModeState] = useState<GroupMode>(loadGroupMode);
  const [query, setQuery] = useState('');
  const [statusFilter, setStatusFilter] = useState<StatusFilter>(null);
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);
  const [panelOpen, setPanelOpen] = useState(false);
  const [editingSpool, setEditingSpool] = useState<FilamentSpool | null>(null);
  const [toast, setToast] = useState<ToastState | null>(null);
  const [popover, setPopover] = useState<PopoverState | null>(null);
  const [highlightIds, setHighlightIds] = useState<ReadonlySet<string>>(() => new Set());
  const [kind, setKindState] = useState<SpoolKind>(() => printers.printers.find(p => p.id === printerContext?.printerId)?.kind ?? loadSpoolKind());
  // Hides the hint banner as soon as the confirmation dialog is open.
  const [jobsOpen, setJobsOpen] = useState(!web && (printerContext?.reviewJobs ?? false));
  useEffect(() => {
    const target = printers.printers.find(p => p.id === printerContext?.printerId);
    if (target) setKindState(target.kind);
  }, [printerContext?.printerId, printers.printers]);
  const [models, setModels] = useState<{ id: string; name: string }[]>([]);

  useEffect(() => {
    if (!jobsOpen) return;
    invoke<{ id: string; name: string }[]>('list_file_summaries')
      .then((list) => setModels(list.map((m) => ({ id: m.id, name: m.name }))))
      .catch(() => setModels([]));
  }, [jobsOpen]);

  // If the list empties while the dialog is open, reset jobsOpen: otherwise
  // the banner would stay hidden and the dialog would pop up on the next sync.
  useEffect(() => {
    if (jobsOpen && printerLink.jobs.length === 0) setJobsOpen(false);
  }, [jobsOpen, printerLink.jobs.length]);

  const changeKind = (next: SpoolKind) => {
    setKindState(next);
    saveSpoolKind(next);
    setPopover(null);
    setConfirmDeleteId(null);
  };

  const changeGroupMode = (next: GroupMode) => {
    setGroupModeState(next);
    saveGroupMode(next);
    setPopover(null);
    setConfirmDeleteId(null);
  };

  useEffect(() => {
    if (highlightIds.size === 0) return;
    const timer = setTimeout(() => setHighlightIds(new Set()), HIGHLIGHT_MS);
    return () => clearTimeout(timer);
  }, [highlightIds]);

  const refresh = () => {
    invoke<FilamentSpool[]>('list_filament_spools')
      .then((result) => {
        setSpools(result);
        setError(null);
      })
      .catch((e) => setError(toAppError(e)));
  };

  useEffect(refresh, []);

  const knownLocations = useMemo(
    () =>
      [...new Set(spools.flatMap((s) => [s.location, s.homeLocation]).filter((l): l is string => !!l))].sort(),
    [spools],
  );

  const formKind = editingSpool?.kind ?? kind;
  const knownManufacturers = useMemo(
    () => spools.filter(s => s.kind === formKind).map(s => s.manufacturer).filter((name): name is string => !!name),
    [spools, formKind],
  );

  // Single view: spools in a printer appear only in the right column, not in storage.
  const storageSpools = useMemo(() => spools.filter((s) => isInStorage(s) && s.kind === kind), [spools, kind]);

  // The printer connection only knows filament (resin is never a booking target).
  const filamentSpools = useMemo(() => spools.filter((s) => s.kind === 'filament'), [spools]);

  /** Does the entry fit the unit? (Resin only into a resin vat, filament never.) */
  const fitsUnit = useCallback(
    (spoolId: string, unitId: string) => {
      const spool = spools.find((s) => s.id === spoolId);
      const unit = printers.printers.flatMap((p) => p.units).find((u) => u.id === unitId);
      return !!spool && !!unit && spoolFitsUnit(spool, unit);
    },
    [spools, printers.printers],
  );

  /** "X1C · AMS 1": printer and unit name of a loaded group. */
  const unitLabel = useCallback(
    (unitId: string) => {
      const printer = printers.printers.find((p) => p.units.some((u) => u.id === unitId));
      const unit = printer?.units.find((u) => u.id === unitId);
      if (!printer || !unit) return '';
      return `${printer.name} · ${unit.kind === 'resin_vat' ? t('printersKindResinVat') : unit.name}`;
    },
    [printers.printers, t],
  );

  const grouped = groupMode === 'grouped';
  const ofKind = useMemo(() => spools.filter((s) => s.kind === kind), [spools, kind]);
  // Grouped view also shows loaded spools, one group per printer unit; the single view
  // keeps listing only what is on the shelf.
  const allGroups = useMemo(() => groupFilaments(ofKind, unitLabel), [ofKind, unitLabel]);

  const filteredGroups = useMemo<SpoolGroup[]>(() => {
    const needle = query.trim().toLowerCase();
    if (grouped) {
      return allGroups.filter((g) => {
        if (statusFilter && g.status !== statusFilter) return false;
        if (!needle) return true;
        const place = g.place.kind === 'unit' ? g.place.label : g.place.location;
        return [g.first.material, g.first.manufacturer, g.first.color, place].some((v) => v?.toLowerCase().includes(needle));
      });
    }
    const singles = storageSpools.filter((s) => {
      if (statusFilter && filamentStockStatus(s) !== statusFilter) return false;
      if (!needle) return true;
      return [s.material, s.manufacturer, s.color, s.location].some((v) => v?.toLowerCase().includes(needle));
    });
    return singleSpoolGroups(singles);
  }, [grouped, allGroups, storageSpools, query, statusFilter]);
  const filtered = useMemo(() => filteredGroups.flatMap((g) => g.spools), [filteredGroups]);

  const stats = useMemo(() => {
    const totalRemaining = roundTenth(ofKind.reduce((sum, s) => sum + s.remainingWeightG, 0));
    // Loaded spools keep their storage location in `homeLocation`.
    const locations = new Set(ofKind.map((s) => s.location ?? s.homeLocation).filter(Boolean)).size;
    // The cards warn per group, so the grouped view counts groups; the single view counts spools.
    const attention = grouped
      ? allGroups.filter((g) => g.status !== 'ok').length
      : ofKind.filter((s) => filamentStockStatus(s) !== 'ok').length;
    return { total: ofKind.length, sorts: allGroups.length, totalRemaining, locations, attention };
  }, [ofKind, allGroups, grouped]);

  const openAddPanel = () => {
    if (!canEditMetadata()) return;
    setEditingSpool(null);
    setPanelOpen(true);
  };
  const openEditPanel = (spool: FilamentSpool) => {
    if (!canEditMetadata()) return;
    setEditingSpool(spool);
    setPanelOpen(true);
  };

  const requestDelete = (id: string) => { if (canEditMetadata()) setConfirmDeleteId(id); };
  const cancelDelete = () => setConfirmDeleteId(null);
  const confirmDelete = (id: string) => {
    if (!canEditMetadata()) return;
    invoke('delete_filament_spool', { spoolId: id })
      .then(() => {
        if (web) invalidateSpoolImage(id);
        setConfirmDeleteId(null);
        refresh();
      })
      .catch((e) => setError(toAppError(e)));
  };

  const toggleStatusFilter = (val: StatusFilter) => setStatusFilter((prev) => (prev === val ? null : val));

  const loadSpool = useCallback(
    (spoolId: string, unitId: string, slotIndex: number) => {
      // The backend rejects it anyway; an unsuitable target doesn't even
      // send a request.
      if (!canEditMetadata() || !fitsUnit(spoolId, unitId)) return;
      printersApi
        .loadSpool(spoolId, unitId, slotIndex)
        .then(result => {
          if (web && result.displacedSpoolId) {
            const displaced = spools.find(s => s.id === result.displacedSpoolId);
            setToast({ type: 'message', label: t('webManagerDisplaced').replace('{spool}', displaced ? spoolLabel(displaced) : result.displacedSpoolId) });
          }
          refresh();
        })
        .catch((e) => setError(toAppError(e)));
    },
    [fitsUnit, spools, web, t],
  );

  const unloadSpool = useCallback(
    (spoolId: string) => {
      if (!canEditMetadata()) return;
      const spool = spools.find((s) => s.id === spoolId);
      printersApi
        .unloadSpool(spoolId, null)
        .then((location) => {
          setToast({ type: 'unload', spoolId, label: spool ? spoolLabel(spool) : '', location });
          refresh();
        })
        .catch((e) => setError(toAppError(e)));
    },
    [spools],
  );

  const changeUnloadedLocation = (location: string) => {
    const spool = toast?.type === 'unload' ? spools.find((s) => s.id === toast.spoolId) : undefined;
    if (!canEditMetadata() || !spool) return Promise.resolve();
    const { hasImage: _hasImage, imagePng: _imagePng, ...fields } = spool;
    return invoke('update_filament_spool', { spool: web ? { ...fields, location } : { ...spool, location } })
      .then(() => refresh())
      .catch((e) => setError(toAppError(e)));
  };

  const dismissToast = useCallback(() => setToast(null), []);

  const togglePopover = (type: PopoverType, spool: FilamentSpool, anchor: HTMLElement, choices?: FilamentSpool[]) =>
    !web && canEditMetadata() && setPopover((prev) => (prev?.type === type && prev.spool.id === spool.id ? null : { type, spool, anchor, choices }));
  const closePopover = useCallback(() => setPopover(null), []);
  const handleRestocked = (created: FilamentSpool[]) => {
    const template = popover?.spool;
    setPopover(null);
    setHighlightIds(new Set(created.map((s) => s.id)));
    if (template) {
      const forms = template.kind === 'resin' ? t('resinRestockDone') : t('filamentRestockDone');
      setToast({ type: 'message', label: formatCount(forms, created.length).replace('{spool}', restockSpoolLabel(template)) });
    }
    refresh();
  };
  const handleConsumed = (updated: FilamentSpool) => {
    // With several bottles to choose from, the one used may differ from the one opened.
    const before = spools.find((s) => s.id === updated.id) ?? popover?.spool;
    setPopover(null);
    if (before) {
      const deducted = roundTenth(before.remainingWeightG - updated.remainingWeightG);
      setToast({
        type: 'message',
        label: t('resinConsumeDone')
          .replace('{amount}', formatVolumeMl(deducted, language))
          .replace('{spool}', restockSpoolLabel(before)),
      });
    }
    refresh();
  };

  const drag = useSpoolDragAndDrop({ onLoad: loadSpool, onUnload: unloadSpool, canDropOnSlot: fitsUnit });
  const draggedSpool = drag.draggingSpoolId ? spools.find((s) => s.id === drag.draggingSpoolId) : undefined;
  const storageIsTarget = drag.draggingFromSlot !== null && drag.target?.kind === 'storage';

  const segBase = 'h-8 px-3.5 rounded-[6px] text-small font-semibold cursor-pointer';
  const segActive = 'bg-[var(--panel)] text-[var(--ink)] shadow-[var(--shadow)] hover:bg-[var(--panel-2)]';
  const segInactive = 'text-[var(--ink-3)] hover:text-[var(--ink)]';

  return (
    <div className={`flex-1 min-w-0 flex flex-col min-h-0 scrollbar-stable overflow-y-auto ${drag.draggingSpoolId ? 'select-none' : ''}`}>
      <div className="flex-none px-4 py-3 border-b border-[var(--line)] text-[length:var(--font-size-body)] font-semibold">
        {t('filamentDialogTitle')}
      </div>

      <div className="flex-1 p-4 flex flex-col lg:flex-row lg:items-start gap-4">
      <div className="flex-1 min-w-0 w-full flex flex-col gap-3.5">
        <SegmentedControl
          label={t('spoolKindLabel')}
          options={[
            { value: 'filament', label: t('spoolKindFilament') },
            { value: 'resin', label: t('spoolKindResin') },
          ]}
          value={kind}
          onChange={changeKind}
        />
        {error && (
          <div className="text-[length:var(--font-size-title)] text-[var(--accent)] break-words">
            {t('filamentError')} <ErrorText error={error} />
          </div>
        )}

        {!web && printerLink.enabled && !jobsOpen && (
          <PrinterJobsBanner jobs={printerLink.jobs} onReview={() => setJobsOpen(true)} />
        )}

        <div className="grid grid-cols-2 sm:grid-cols-3 xl:grid-cols-5 gap-2.5">
          <div className="rounded-lg border border-[var(--line)] bg-[var(--panel)] px-3.5 py-2.5">
            <div className="text-compact-meta ui-label text-[var(--ink-3)] mb-1.5">{kind === 'resin' ? t('resinStatTotal') : t('filamentStatTotal')}</div>
            <div className="tabular-nums text-heading font-bold">{stats.total}</div>
          </div>
          <div className="rounded-lg border border-[var(--line)] bg-[var(--panel)] px-3.5 py-2.5">
            <div className="text-compact-meta ui-label text-[var(--ink-3)] mb-1.5">{t('filamentStatSorts')}</div>
            <div className="tabular-nums text-heading font-bold">{stats.sorts}</div>
          </div>
          <div className="rounded-lg border border-[var(--line)] bg-[var(--panel)] px-3.5 py-2.5">
            <div className="text-compact-meta ui-label text-[var(--ink-3)] mb-1.5">{t('filamentStatRemaining')}</div>
            {/* Filament: sum over remaining weights precise to 0.1 g, rounded to
                whole grams (printer connection spec). Resin: milliliters, precise to 0.1 ml. */}
            <div className="tabular-nums text-heading font-bold">
              {kind === 'resin'
                ? formatVolumeMl(stats.totalRemaining, language)
                : formatWeightG(Math.round(stats.totalRemaining), language)}
            </div>
          </div>
          <div className="rounded-lg border border-[var(--line)] bg-[var(--panel)] px-3.5 py-2.5">
            <div className="text-compact-meta ui-label text-[var(--ink-3)] mb-1.5">{t('filamentStatLocations')}</div>
            <div className="tabular-nums text-heading font-bold">{stats.locations}</div>
          </div>
          <div className="rounded-lg border border-[var(--line)] bg-[var(--panel)] px-3.5 py-2.5">
            <div className="text-compact-meta ui-label text-[var(--ink-3)] mb-1.5">{t('filamentStatAttention')}</div>
            <div className={`tabular-nums text-heading font-bold ${stats.attention > 0 ? 'text-[var(--warn)]' : ''}`}>
              {stats.attention}
            </div>
          </div>
        </div>

        <div className="flex items-center gap-2 flex-wrap">
          <div className="relative flex-1 min-w-[180px] max-w-[320px]">
            <svg
              width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2"
              className="absolute left-2.5 top-1/2 -translate-y-1/2 opacity-50 pointer-events-none"
            >
              <circle cx="11" cy="11" r="7" /><path d="m21 21-4.3-4.3" />
            </svg>
            <input
              type="search"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t('filamentSearchPlaceholder')}
              className="w-full h-8 pl-8 pr-2.5 rounded-md border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-small outline-0 focus:border-[var(--accent)]"
            />
          </div>

          <button
            onClick={() => toggleStatusFilter('low')}
            className={`h-8 px-3 rounded-md border text-small font-semibold cursor-pointer ${
              statusFilter === 'low'
                ? 'bg-[var(--warn-soft)] border-[var(--warn)] text-[var(--warn)]'
                : 'border-[var(--line-strong)] text-[var(--ink-2)] hover:border-[var(--warn)] hover:text-[var(--warn)]'
            }`}
          >
            {t('filamentFilterLow')}
          </button>
          <button
            onClick={() => toggleStatusFilter('empty')}
            className={`h-8 px-3 rounded-md border text-small font-semibold cursor-pointer ${
              statusFilter === 'empty'
                ? 'bg-[var(--crit-soft)] border-[var(--crit)] text-[var(--crit)]'
                : 'border-[var(--line-strong)] text-[var(--ink-2)] hover:border-[var(--crit)] hover:text-[var(--crit)]'
            }`}
          >
            {t('filamentFilterEmpty')}
          </button>

          <div className="flex p-0.5 gap-0.5 border border-[var(--line)] rounded-[8px] bg-[var(--panel-2)]">
            <button onClick={() => { setLayout('dashboard'); saveLayout('dashboard'); }} className={`${segBase} ${layout === 'dashboard' ? segActive : segInactive}`}>
              {t('filamentViewDashboard')}
            </button>
            <button onClick={() => { setLayout('list'); saveLayout('list'); }} className={`${segBase} ${layout === 'list' ? segActive : segInactive}`}>
              {t('filamentViewList')}
            </button>
          </div>

          <SegmentedControl
            label={t('filamentGroupModeLabel')}
            options={[
              { value: 'grouped', label: t('filamentGroupModeGrouped') },
              { value: 'single', label: t('filamentGroupModeSingle') },
            ]}
            value={groupMode}
            onChange={changeGroupMode}
          />

          <button
            disabled={!canEdit}
            onClick={openAddPanel}
            className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] ml-auto h-8 px-3.5 rounded-md border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-small font-bold cursor-pointer flex items-center gap-1.5"
          >
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.6"><path d="M12 5v14M5 12h14" /></svg>
            {kind === 'resin' ? t('resinOpenAddPanelButton') : t('filamentOpenAddPanelButton')}
          </button>
        </div>

        <div
          data-testid="filament-storage"
          onMouseEnter={() => drag.enterTarget({ kind: 'storage' })}
          onMouseLeave={() => drag.leaveTarget({ kind: 'storage' })}
          className={`rounded-lg min-h-[240px] ${storageIsTarget ? 'outline-2 outline-dashed outline-[var(--accent)] outline-offset-4' : ''}`}
        >
          {layout === 'dashboard' ? (
            <FilamentDashboard
              spools={filtered}
              groups={filteredGroups}
              confirmDeleteId={confirmDeleteId}
              onEdit={openEditPanel}
              onRequestDelete={requestDelete}
              onCancelDelete={cancelDelete}
              onConfirmDelete={confirmDelete}
              onSpoolMouseDown={(spoolId, e) => drag.startDrag(spoolId, null, e)}
              onRestock={web ? undefined : (spool, anchor) => togglePopover('restock', spool, anchor)}
              restockOpenId={!web && popover?.type === 'restock' ? popover.spool.id : null}
              onConsume={web ? undefined : (spool, anchor, choices) => togglePopover('consume', spool, anchor, choices)}
              consumeOpenId={!web && popover?.type === 'consume' ? popover.spool.id : null}
              highlightIds={highlightIds}
            />
          ) : (
            <FilamentTable
              spools={filtered}
              groups={filteredGroups}
              showCount={grouped}
              confirmDeleteId={confirmDeleteId}
              onEdit={openEditPanel}
              onRequestDelete={requestDelete}
              onCancelDelete={cancelDelete}
              onConfirmDelete={confirmDelete}
              onSpoolMouseDown={(spoolId, e) => drag.startDrag(spoolId, null, e)}
              onRestock={web ? undefined : (spool, anchor) => togglePopover('restock', spool, anchor)}
              restockOpenId={!web && popover?.type === 'restock' ? popover.spool.id : null}
              onConsume={web ? undefined : (spool, anchor, choices) => togglePopover('consume', spool, anchor, choices)}
              consumeOpenId={!web && popover?.type === 'consume' ? popover.spool.id : null}
              highlightIds={highlightIds}
              kind={kind}
            />
          )}
        </div>
      </div>

      <PrinterColumn
        printers={printers.printers}
        spools={spools}
        kind={kind}
        draggingSpoolId={drag.draggingSpoolId}
        dropTarget={drag.target}
        onSlotMouseDown={(spoolId, unitId, slotIndex, e) => drag.startDrag(spoolId, { unitId, slotIndex }, e)}
        onEnterSlot={(unitId, slotIndex) => drag.enterTarget({ kind: 'slot', unitId, slotIndex })}
        onLeaveSlot={(unitId, slotIndex) => drag.leaveTarget({ kind: 'slot', unitId, slotIndex })}
        onLoad={loadSpool}
        onUnload={unloadSpool}
        onEditSpool={openEditPanel}
        onConsume={web ? undefined : (spool, anchor) => togglePopover('consume', spool, anchor)}
        onManage={id => onPrinterManager?.(id)}
        focusPrinterId={printerContext?.printerId}
        printerLink={printerLink}
      />
      </div>

      {draggedSpool && drag.pointer && (
        <div
          aria-hidden
          className="fixed z-50 pointer-events-none px-2.5 py-1.5 rounded-md border border-[var(--accent)] bg-[var(--panel)] shadow-[var(--shadow)] text-small font-semibold flex items-center gap-1.5 -rotate-2"
          style={{ left: drag.pointer.x + 12, top: drag.pointer.y + 12 }}
        >
          <SpoolColorDot spool={draggedSpool} />
          {spoolLabel(draggedSpool)}
        </div>
      )}

      {toast?.type === 'unload' && (
        <SpoolToast
          label={toast.label}
          location={toast.location}
          knownLocations={knownLocations}
          onChangeLocation={canEdit ? changeUnloadedLocation : undefined}
          onDone={dismissToast}
        />
      )}
      {toast?.type === 'message' && (
        <SpoolToast label={toast.label} location={null} knownLocations={[]} onDone={dismissToast} />
      )}

      {!web && popover?.type === 'restock' && (
        <RestockPopover
          key={popover.spool.id}
          spool={popover.spool}
          anchor={popover.anchor}
          knownLocations={knownLocations}
          onClose={closePopover}
          onCreated={handleRestocked}
        />
      )}

      {!web && popover?.type === 'consume' && (
        <ConsumeResinPopover
          key={popover.spool.id}
          spool={popover.spool}
          choices={popover.choices}
          anchor={popover.anchor}
          onClose={closePopover}
          onConsumed={handleConsumed}
        />
      )}

      <FilamentSpoolForm
        open={panelOpen && canEdit}
        editing={editingSpool}
        knownManufacturers={knownManufacturers}
        knownSpools={spools}
        knownLocations={knownLocations}
        onClose={() => setPanelOpen(false)}
        onSaved={refresh}
        defaultKind={kind}
      />

      {!web && <PrinterJobsDialog
        open={jobsOpen && printerLink.jobs.length > 0}
        jobs={printerLink.jobs}
        spools={filamentSpools}
        models={models}
        link={printerLink}
        onClose={() => setJobsOpen(false)}
        onBooked={() => {
          refresh();
          onCatalogChanged?.();
        }}
      />}
    </div>
  );
}
