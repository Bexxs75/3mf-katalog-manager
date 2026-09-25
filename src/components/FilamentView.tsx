import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatWeightG } from '../i18n/format';
import type { FilamentSpool } from '../types';
import type { PrinterLinkState } from '../hooks/usePrinterLink';
import { filamentStockStatus } from '../lib/filamentStatus';
import { isValidColorHex } from '../lib/filamentColors';
import { FilamentDashboard } from './FilamentDashboard';
import { FilamentTable } from './FilamentTable';
import { FilamentSpoolForm } from './FilamentSpoolForm';
import { PrinterColumn } from './PrinterColumn';
import { PrinterJobsBanner } from './PrinterJobsBanner';
import { PrinterJobsDialog } from './PrinterJobsDialog';
import { PrinterManagePanel } from './PrinterManagePanel';
import { SpoolToast } from './SpoolToast';
import type { PrintersState } from '../hooks/usePrinters';
import { useSpoolDragAndDrop } from '../hooks/useSpoolDragAndDrop';
import * as printersApi from '../lib/api/printers';
import { isInStorage, spoolLabel } from '../lib/filamentSlots';

type LayoutMode = 'dashboard' | 'list';
type StatusFilter = 'low' | 'empty' | null;

interface Props {
  printerLink: PrinterLinkState;
  /**
   * Von App.tsx einmal angelegt und auch an Rail (Reiter "Drucker")
   * weitergereicht - eine gemeinsame Instanz, damit z.B. das Hinzufuegen
   * eines Druckers hier auch dort sofort sichtbar ist (nicht erst nach
   * einem Remount).
   */
  printers: PrintersState;
  /** Katalog neu laden (z.B. Druckstatus/Warteschlange) nach dem Bestaetigen von Drucken. */
  onCatalogChanged?: () => void;
}

export function FilamentView({ printerLink, printers, onCatalogChanged }: Props) {
  const t = useT();
  const { language } = useLanguage();
  const [spools, setSpools] = useState<FilamentSpool[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [layout, setLayout] = useState<LayoutMode>('dashboard');
  const [query, setQuery] = useState('');
  const [statusFilter, setStatusFilter] = useState<StatusFilter>(null);
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);
  const [panelOpen, setPanelOpen] = useState(false);
  const [editingSpool, setEditingSpool] = useState<FilamentSpool | null>(null);
  const [manageOpen, setManageOpen] = useState(false);
  const [toast, setToast] = useState<{ spoolId: string; label: string; location: string | null } | null>(null);
  // Blendet den Hinweisbanner aus, sobald der Bestaetigungs-Dialog offen ist.
  const [jobsOpen, setJobsOpen] = useState(false);
  const [models, setModels] = useState<{ id: string; name: string }[]>([]);

  useEffect(() => {
    if (!jobsOpen) return;
    invoke<{ id: string; name: string }[]>('list_file_summaries')
      .then((list) => setModels(list.map((m) => ({ id: m.id, name: m.name }))))
      .catch(() => setModels([]));
  }, [jobsOpen]);

  // Wird die Liste waehrend der Dialog offen ist leer (z.B. nach dem letzten
  // Bestaetigen/Ignorieren), muss `jobsOpen` mit zurueckgesetzt werden -
  // sonst bleibt der Banner dauerhaft ausgeblendet (er zeigt sich nur, wenn
  // `!jobsOpen`) UND der Dialog wuerde beim naechsten Abgleich mit neuen
  // Drucken ungefragt wieder aufspringen (`open={jobsOpen && jobs.length > 0}`).
  useEffect(() => {
    if (jobsOpen && printerLink.jobs.length === 0) setJobsOpen(false);
  }, [jobsOpen, printerLink.jobs.length]);

  const refresh = () => {
    invoke<FilamentSpool[]>('list_filament_spools')
      .then((result) => {
        setSpools(result);
        setError(null);
      })
      .catch((e) => setError(String(e)));
  };

  useEffect(refresh, []);

  const knownLocations = useMemo(
    () =>
      [...new Set(spools.flatMap((s) => [s.location, s.homeLocation]).filter((l): l is string => !!l))].sort(),
    [spools],
  );

  // Spulen im Drucker stehen nur in der rechten Spalte, nicht im Lager
  // (Spec: "Spulen im Drucker werden getrennt vom Lager angezeigt").
  const storageSpools = useMemo(() => spools.filter(isInStorage), [spools]);

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return storageSpools.filter((s) => {
      if (statusFilter && filamentStockStatus(s) !== statusFilter) return false;
      if (!needle) return true;
      return [s.material, s.manufacturer, s.color, s.location].some((v) => v?.toLowerCase().includes(needle));
    });
  }, [storageSpools, query, statusFilter]);

  const stats = useMemo(() => {
    const totalRemaining = spools.reduce((sum, s) => sum + s.remainingWeightG, 0);
    // `s.location ?? s.homeLocation`: eine geladene Spule hat `location` auf
    // NULL stehen (ihr Lagerort liegt als Stammplatz in `homeLocation`, siehe
    // db/printers.rs) - ohne den Fallback wuerde ihr Lagerort beim Laden aus
    // dieser Statistik verschwinden, obwohl er weiterhin existiert.
    const locations = new Set(spools.map((s) => s.location ?? s.homeLocation).filter(Boolean)).size;
    const attention = spools.filter((s) => filamentStockStatus(s) !== 'ok').length;
    return { total: spools.length, totalRemaining, locations, attention };
  }, [spools]);

  const openAddPanel = () => {
    setEditingSpool(null);
    setPanelOpen(true);
  };
  const openEditPanel = (spool: FilamentSpool) => {
    setEditingSpool(spool);
    setPanelOpen(true);
  };

  const requestDelete = (id: string) => setConfirmDeleteId(id);
  const cancelDelete = () => setConfirmDeleteId(null);
  const confirmDelete = (id: string) => {
    invoke('delete_filament_spool', { spoolId: id })
      .then(() => {
        setConfirmDeleteId(null);
        refresh();
      })
      .catch((e) => setError(String(e)));
  };

  const toggleStatusFilter = (val: StatusFilter) => setStatusFilter((prev) => (prev === val ? null : val));

  const loadSpool = useCallback((spoolId: string, unitId: string, slotIndex: number) => {
    printersApi
      .loadSpool(spoolId, unitId, slotIndex)
      .then(() => refresh())
      .catch((e) => setError(String(e)));
  }, []);

  const unloadSpool = useCallback(
    (spoolId: string) => {
      const spool = spools.find((s) => s.id === spoolId);
      printersApi
        .unloadSpool(spoolId, null)
        .then((location) => {
          setToast({ spoolId, label: spool ? spoolLabel(spool) : '', location });
          refresh();
        })
        .catch((e) => setError(String(e)));
    },
    [spools],
  );

  const changeUnloadedLocation = (location: string) => {
    const spool = spools.find((s) => s.id === toast?.spoolId);
    if (!spool) return Promise.resolve();
    return invoke('update_filament_spool', { spool: { ...spool, location } })
      .then(() => refresh())
      .catch((e) => setError(String(e)));
  };

  const dismissToast = useCallback(() => setToast(null), []);

  const drag = useSpoolDragAndDrop({ onLoad: loadSpool, onUnload: unloadSpool });
  const draggedSpool = drag.draggingSpoolId ? spools.find((s) => s.id === drag.draggingSpoolId) : undefined;
  const storageIsTarget = drag.draggingFromSlot !== null && drag.target?.kind === 'storage';

  const segBase = 'h-8 px-3.5 rounded-[6px] text-[12.5px] font-semibold cursor-pointer';
  const segActive = 'bg-[var(--panel)] text-[var(--ink)] shadow-[var(--shadow)]';
  const segInactive = 'text-[var(--ink-3)] hover:text-[var(--ink)]';

  return (
    <div className={`flex-1 min-w-0 flex flex-col min-h-0 overflow-y-auto ${drag.draggingSpoolId ? 'select-none' : ''}`}>
      <div className="flex-none px-4 py-3 border-b border-[var(--line)] text-[length:var(--font-size-body)] font-semibold">
        {t('filamentDialogTitle')}
      </div>

      <div className="flex-1 p-4 flex flex-col lg:flex-row lg:items-start gap-4">
      <div className="flex-1 min-w-0 w-full flex flex-col gap-3.5">
        {error && (
          <div className="text-[length:var(--font-size-title)] text-[var(--accent)] break-words">
            {t('filamentError')} {error}
          </div>
        )}

        {printerLink.enabled && !jobsOpen && (
          <PrinterJobsBanner jobs={printerLink.jobs} onReview={() => setJobsOpen(true)} />
        )}

        <div className="grid grid-cols-2 sm:grid-cols-4 gap-2.5">
          <div className="rounded-lg border border-[var(--line)] bg-[var(--panel)] px-3.5 py-2.5">
            <div className="text-[10px] uppercase tracking-wider font-bold text-[var(--ink-3)] mb-1.5">{t('filamentStatTotal')}</div>
            <div className="font-mono-ui text-[19px] font-bold tabular-nums">{stats.total}</div>
          </div>
          <div className="rounded-lg border border-[var(--line)] bg-[var(--panel)] px-3.5 py-2.5">
            <div className="text-[10px] uppercase tracking-wider font-bold text-[var(--ink-3)] mb-1.5">{t('filamentStatRemaining')}</div>
            {/* Summe ueber Zehntelgramm-genaue Restgewichte - ohne Runden zeigt
                formatWeightG hier gelegentlich Nachkommastellen (Spec: ganze Gramm). */}
            <div className="font-mono-ui text-[19px] font-bold tabular-nums">{formatWeightG(Math.round(stats.totalRemaining), language)}</div>
          </div>
          <div className="rounded-lg border border-[var(--line)] bg-[var(--panel)] px-3.5 py-2.5">
            <div className="text-[10px] uppercase tracking-wider font-bold text-[var(--ink-3)] mb-1.5">{t('filamentStatLocations')}</div>
            <div className="font-mono-ui text-[19px] font-bold tabular-nums">{stats.locations}</div>
          </div>
          <div className="rounded-lg border border-[var(--line)] bg-[var(--panel)] px-3.5 py-2.5">
            <div className="text-[10px] uppercase tracking-wider font-bold text-[var(--ink-3)] mb-1.5">{t('filamentStatAttention')}</div>
            <div className={`font-mono-ui text-[19px] font-bold tabular-nums ${stats.attention > 0 ? 'text-[var(--warn)]' : ''}`}>
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
              className="w-full h-8 pl-8 pr-2.5 rounded-md border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[12.5px] outline-0 focus:border-[var(--accent)]"
            />
          </div>

          <button
            onClick={() => toggleStatusFilter('low')}
            className={`h-8 px-3 rounded-md border text-[12px] font-semibold cursor-pointer ${
              statusFilter === 'low'
                ? 'bg-[var(--warn-soft)] border-[var(--warn)] text-[var(--warn)]'
                : 'border-[var(--line-strong)] text-[var(--ink-2)] hover:border-[var(--warn)] hover:text-[var(--warn)]'
            }`}
          >
            {t('filamentFilterLow')}
          </button>
          <button
            onClick={() => toggleStatusFilter('empty')}
            className={`h-8 px-3 rounded-md border text-[12px] font-semibold cursor-pointer ${
              statusFilter === 'empty'
                ? 'bg-[var(--crit-soft)] border-[var(--crit)] text-[var(--crit)]'
                : 'border-[var(--line-strong)] text-[var(--ink-2)] hover:border-[var(--crit)] hover:text-[var(--crit)]'
            }`}
          >
            {t('filamentFilterEmpty')}
          </button>

          <div className="flex p-0.5 gap-0.5 border border-[var(--line)] rounded-[8px] bg-[var(--panel-2)]">
            <button onClick={() => setLayout('dashboard')} className={`${segBase} ${layout === 'dashboard' ? segActive : segInactive}`}>
              {t('filamentViewDashboard')}
            </button>
            <button onClick={() => setLayout('list')} className={`${segBase} ${layout === 'list' ? segActive : segInactive}`}>
              {t('filamentViewList')}
            </button>
          </div>

          <button
            onClick={openAddPanel}
            className="ml-auto h-8 px-3.5 rounded-md border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12.5px] font-bold cursor-pointer flex items-center gap-1.5"
          >
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.6"><path d="M12 5v14M5 12h14" /></svg>
            {t('filamentOpenAddPanelButton')}
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
              confirmDeleteId={confirmDeleteId}
              onEdit={openEditPanel}
              onRequestDelete={requestDelete}
              onCancelDelete={cancelDelete}
              onConfirmDelete={confirmDelete}
              onSpoolMouseDown={(spoolId, e) => drag.startDrag(spoolId, null, e)}
            />
          ) : (
            <FilamentTable
              spools={filtered}
              confirmDeleteId={confirmDeleteId}
              onEdit={openEditPanel}
              onRequestDelete={requestDelete}
              onCancelDelete={cancelDelete}
              onConfirmDelete={confirmDelete}
              onSpoolMouseDown={(spoolId, e) => drag.startDrag(spoolId, null, e)}
            />
          )}
        </div>
      </div>

      <PrinterColumn
        printers={printers.printers}
        spools={spools}
        draggingSpoolId={drag.draggingSpoolId}
        dropTarget={drag.target}
        onSlotMouseDown={(spoolId, unitId, slotIndex, e) => drag.startDrag(spoolId, { unitId, slotIndex }, e)}
        onEnterSlot={(unitId, slotIndex) => drag.enterTarget({ kind: 'slot', unitId, slotIndex })}
        onLeaveSlot={(unitId, slotIndex) => drag.leaveTarget({ kind: 'slot', unitId, slotIndex })}
        onLoad={loadSpool}
        onUnload={unloadSpool}
        onEditSpool={openEditPanel}
        onManage={() => setManageOpen(true)}
        printerLink={printerLink}
      />
      </div>

      {draggedSpool && drag.pointer && (
        <div
          aria-hidden
          className="fixed z-50 pointer-events-none px-2.5 py-1.5 rounded-md border border-[var(--accent)] bg-[var(--panel)] shadow-[var(--shadow)] text-[12px] font-semibold flex items-center gap-1.5 -rotate-2"
          style={{ left: drag.pointer.x + 12, top: drag.pointer.y + 12 }}
        >
          {draggedSpool.colorHex && isValidColorHex(draggedSpool.colorHex) && (
            <span className="w-3 h-3 rounded-full border border-[var(--line-strong)]" style={{ backgroundColor: draggedSpool.colorHex }} />
          )}
          {spoolLabel(draggedSpool)}
        </div>
      )}

      {toast && (
        <SpoolToast
          label={toast.label}
          location={toast.location}
          knownLocations={knownLocations}
          onChangeLocation={changeUnloadedLocation}
          onDone={dismissToast}
        />
      )}

      <PrinterManagePanel
        open={manageOpen}
        printers={printers.printers}
        spools={spools}
        error={printers.error}
        actions={printers}
        onClose={() => setManageOpen(false)}
        onSpoolsChanged={refresh}
        printerLink={printerLink}
      />

      <FilamentSpoolForm
        open={panelOpen}
        editing={editingSpool}
        knownLocations={knownLocations}
        onClose={() => setPanelOpen(false)}
        onSaved={refresh}
      />

      <PrinterJobsDialog
        open={jobsOpen && printerLink.jobs.length > 0}
        jobs={printerLink.jobs}
        spools={spools}
        models={models}
        link={printerLink}
        onClose={() => setJobsOpen(false)}
        onBooked={() => {
          refresh();
          onCatalogChanged?.();
        }}
      />
    </div>
  );
}
