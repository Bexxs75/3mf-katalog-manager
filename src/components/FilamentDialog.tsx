import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useT } from '../i18n/LanguageContext';
import type { FilamentSpool } from '../types';

interface Props {
  onClose: () => void;
}

interface FormState {
  material: string;
  manufacturer: string;
  color: string;
  diameterMm: string;
  originalWeightG: string;
  remainingWeightG: string;
  price: string;
}

const EMPTY_FORM: FormState = {
  material: '',
  manufacturer: '',
  color: '',
  diameterMm: '1.75',
  originalWeightG: '1000',
  remainingWeightG: '1000',
  price: '',
};

const fieldClass =
  'h-8 px-2 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0 text-[12.5px]';

export function FilamentDialog({ onClose }: Props) {
  const t = useT();
  const [spools, setSpools] = useState<FilamentSpool[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [form, setForm] = useState<FormState>(EMPTY_FORM);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);

  const refresh = () => {
    invoke<FilamentSpool[]>('list_filament_spools')
      .then((result) => {
        setSpools(result);
        setError(null);
      })
      .catch((e) => setError(String(e)));
  };

  useEffect(refresh, []);

  const startEdit = (spool: FilamentSpool) => {
    setEditingId(spool.id);
    setConfirmDeleteId(null);
    setForm({
      material: spool.material,
      manufacturer: spool.manufacturer ?? '',
      color: spool.color ?? '',
      diameterMm: String(spool.diameterMm),
      originalWeightG: String(spool.originalWeightG),
      remainingWeightG: String(spool.remainingWeightG),
      price: spool.price === null ? '' : String(spool.price),
    });
  };

  const cancelForm = () => {
    setEditingId(null);
    setForm(EMPTY_FORM);
  };

  const submitForm = () => {
    if (!form.material.trim()) return;
    const payload: FilamentSpool = {
      id: editingId ?? '',
      material: form.material.trim(),
      manufacturer: form.manufacturer.trim() || null,
      color: form.color.trim() || null,
      diameterMm: parseFloat(form.diameterMm) || 0,
      originalWeightG: parseInt(form.originalWeightG, 10) || 0,
      remainingWeightG: parseInt(form.remainingWeightG, 10) || 0,
      price: form.price.trim() === '' ? null : parseFloat(form.price),
    };
    const command = editingId ? 'update_filament_spool' : 'add_filament_spool';
    invoke(command, { spool: payload })
      .then(() => {
        cancelForm();
        refresh();
      })
      .catch((e) => setError(String(e)));
  };

  const deleteSpool = (id: string) => {
    invoke('delete_filament_spool', { spoolId: id })
      .then(() => {
        setConfirmDeleteId(null);
        refresh();
      })
      .catch((e) => setError(String(e)));
  };

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/50">
      <div className="w-[480px] max-h-[640px] flex flex-col bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)]">
        <div className="flex-none px-4 py-3 border-b border-[var(--line)] flex items-center justify-between">
          <span className="text-[13px] font-semibold">{t('filamentDialogTitle')}</span>
          <span
            onClick={onClose}
            aria-label={t('cancel')}
            className="w-6 h-6 grid place-items-center rounded-full cursor-pointer text-[12px] text-[var(--ink-3)] hover:bg-[var(--panel-2)]"
          >
            ✕
          </span>
        </div>

        <div className="flex-1 overflow-y-auto px-4 py-3">
          {error && (
            <div className="pb-2 text-[12.5px] text-[var(--accent)] break-words">
              {t('filamentError')} {error}
            </div>
          )}
          {spools.length === 0 ? (
            <div className="text-[12.5px] text-[var(--ink-3)]">{t('filamentEmptyState')}</div>
          ) : (
            <div className="flex flex-col gap-2">
              {spools.map((spool) => (
                <div
                  key={spool.id}
                  className="flex items-center gap-2 px-2.5 py-2 rounded-[3px] border border-[var(--line)]"
                >
                  <div className="flex-1 min-w-0">
                    <div className="text-[13px] text-[var(--ink)] truncate">
                      {spool.material}
                      {spool.color ? ` · ${spool.color}` : ''}
                      {spool.manufacturer ? ` · ${spool.manufacturer}` : ''}
                    </div>
                    <div className="font-mono-ui text-[10.5px] text-[var(--ink-3)]">
                      {spool.remainingWeightG} g / {spool.originalWeightG} g · {spool.diameterMm} mm
                      {spool.price !== null ? ` · ${spool.price}` : ''}
                    </div>
                  </div>
                  {confirmDeleteId === spool.id ? (
                    <div className="flex items-center gap-1.5 flex-none">
                      <span className="text-[11.5px] text-[var(--ink)]">{t('deleteConfirmQuestion')}</span>
                      <button
                        onClick={() => setConfirmDeleteId(null)}
                        className="h-6 px-2 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[11px] cursor-pointer"
                      >
                        {t('cancel')}
                      </button>
                      <button
                        onClick={() => deleteSpool(spool.id)}
                        className="h-6 px-2 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[11px] cursor-pointer"
                      >
                        {t('delete')}
                      </button>
                    </div>
                  ) : (
                    <div className="flex items-center gap-1.5 flex-none">
                      <span
                        onClick={() => startEdit(spool)}
                        aria-label={t('filamentEditAria')}
                        className="w-6 h-6 grid place-items-center rounded-full cursor-pointer text-[11px] text-[var(--ink-3)] hover:bg-[var(--panel-2)]"
                      >
                        ✎
                      </span>
                      <span
                        onClick={() => setConfirmDeleteId(spool.id)}
                        aria-label={t('deleteAriaLabel')}
                        className="w-6 h-6 grid place-items-center rounded-full cursor-pointer text-[11px] text-[var(--ink-3)] hover:bg-[var(--accent)] hover:text-[var(--accent-ink)]"
                      >
                        ✕
                      </span>
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>

        <div className="flex-none px-4 py-3 border-t border-[var(--line)] bg-[var(--panel-2)]">
          <div className="grid grid-cols-2 gap-2 mb-2">
            <input
              value={form.material}
              onChange={(e) => setForm({ ...form, material: e.target.value })}
              placeholder={t('filamentMaterialLabel')}
              className={fieldClass}
            />
            <input
              value={form.manufacturer}
              onChange={(e) => setForm({ ...form, manufacturer: e.target.value })}
              placeholder={t('filamentManufacturerLabel')}
              className={fieldClass}
            />
            <input
              value={form.color}
              onChange={(e) => setForm({ ...form, color: e.target.value })}
              placeholder={t('filamentColorLabel')}
              className={fieldClass}
            />
            <input
              type="number"
              step="0.01"
              value={form.diameterMm}
              onChange={(e) => setForm({ ...form, diameterMm: e.target.value })}
              placeholder={t('filamentDiameterLabel')}
              className={fieldClass}
            />
            <input
              type="number"
              value={form.originalWeightG}
              onChange={(e) => setForm({ ...form, originalWeightG: e.target.value })}
              placeholder={t('filamentOriginalWeightLabel')}
              className={fieldClass}
            />
            <input
              type="number"
              value={form.remainingWeightG}
              onChange={(e) => setForm({ ...form, remainingWeightG: e.target.value })}
              placeholder={t('filamentRemainingWeightLabel')}
              className={fieldClass}
            />
            <input
              type="number"
              step="0.01"
              value={form.price}
              onChange={(e) => setForm({ ...form, price: e.target.value })}
              placeholder={t('filamentPriceLabel')}
              className={fieldClass}
            />
          </div>
          <div className="flex gap-2">
            {editingId && (
              <button
                onClick={cancelForm}
                className="flex-1 h-8 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[12.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
              >
                {t('cancel')}
              </button>
            )}
            <button
              onClick={submitForm}
              disabled={!form.material.trim()}
              className="flex-1 h-8 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12.5px] font-semibold cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
            >
              {editingId ? t('filamentSaveButton') : t('filamentAddButton')}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
