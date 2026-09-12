import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatWeightG } from '../i18n/format';
import type { FilamentSpool } from '../types';
import { filamentStockStatus } from '../lib/filamentStatus';
import { FilamentDashboard } from './FilamentDashboard';
import { FilamentTable } from './FilamentTable';
import { FilamentSpoolForm } from './FilamentSpoolForm';

type LayoutMode = 'dashboard' | 'list';
type StatusFilter = 'low' | 'empty' | null;

export function FilamentView() {
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
    () => [...new Set(spools.map((s) => s.location).filter((l): l is string => !!l))].sort(),
    [spools],
  );

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return spools.filter((s) => {
      if (statusFilter && filamentStockStatus(s) !== statusFilter) return false;
      if (!needle) return true;
      return [s.material, s.manufacturer, s.color, s.location].some((v) => v?.toLowerCase().includes(needle));
    });
  }, [spools, query, statusFilter]);

  const stats = useMemo(() => {
    const totalRemaining = spools.reduce((sum, s) => sum + s.remainingWeightG, 0);
    const locations = new Set(spools.map((s) => s.location).filter(Boolean)).size;
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

  const segBase = 'h-8 px-3.5 rounded-[6px] text-[12.5px] font-semibold cursor-pointer';
  const segActive = 'bg-[var(--panel)] text-[var(--ink)] shadow-[var(--shadow)]';
  const segInactive = 'text-[var(--ink-3)] hover:text-[var(--ink)]';

  return (
    <div className="flex-1 min-w-0 flex flex-col min-h-0 overflow-y-auto">
      <div className="flex-none px-4 py-3 border-b border-[var(--line)] text-[length:var(--font-size-body)] font-semibold">
        {t('filamentDialogTitle')}
      </div>

      <div className="flex-1 p-4 flex flex-col gap-3.5">
        {error && (
          <div className="text-[length:var(--font-size-title)] text-[var(--accent)] break-words">
            {t('filamentError')} {error}
          </div>
        )}

        <div className="grid grid-cols-2 sm:grid-cols-4 gap-2.5">
          <div className="rounded-lg border border-[var(--line)] bg-[var(--panel)] px-3.5 py-2.5">
            <div className="text-[10px] uppercase tracking-wider font-bold text-[var(--ink-3)] mb-1.5">{t('filamentStatTotal')}</div>
            <div className="font-mono-ui text-[19px] font-bold tabular-nums">{stats.total}</div>
          </div>
          <div className="rounded-lg border border-[var(--line)] bg-[var(--panel)] px-3.5 py-2.5">
            <div className="text-[10px] uppercase tracking-wider font-bold text-[var(--ink-3)] mb-1.5">{t('filamentStatRemaining')}</div>
            <div className="font-mono-ui text-[19px] font-bold tabular-nums">{formatWeightG(stats.totalRemaining, language)}</div>
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

        {layout === 'dashboard' ? (
          <FilamentDashboard
            spools={filtered}
            confirmDeleteId={confirmDeleteId}
            onEdit={openEditPanel}
            onRequestDelete={requestDelete}
            onCancelDelete={cancelDelete}
            onConfirmDelete={confirmDelete}
          />
        ) : (
          <FilamentTable
            spools={filtered}
            confirmDeleteId={confirmDeleteId}
            onEdit={openEditPanel}
            onRequestDelete={requestDelete}
            onCancelDelete={cancelDelete}
            onConfirmDelete={confirmDelete}
          />
        )}
      </div>

      <FilamentSpoolForm
        open={panelOpen}
        editing={editingSpool}
        knownLocations={knownLocations}
        onClose={() => setPanelOpen(false)}
        onSaved={refresh}
      />
    </div>
  );
}
