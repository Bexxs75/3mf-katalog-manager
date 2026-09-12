import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatWeightG, formatDiameterMm, formatPrice } from '../i18n/format';
import type { FilamentSpool } from '../types';
import { FILAMENT_MATERIALS, FILAMENT_MANUFACTURERS } from '../lib/filamentCatalog';
import { AutocompleteInput } from './AutocompleteInput';

interface FormState {
  material: string;
  manufacturer: string;
  color: string;
  diameterMm: string;
  originalWeightG: string;
  remainingWeightG: string;
  price: string;
  imagePng: string | null;
}

const EMPTY_FORM: FormState = {
  material: '',
  manufacturer: '',
  color: '',
  diameterMm: '1.75',
  originalWeightG: '1000',
  remainingWeightG: '1000',
  price: '',
  imagePng: null,
};

const fieldClass =
  'h-8 px-2 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0 text-[length:var(--font-size-title)]';

export function FilamentView() {
  const t = useT();
  const { language } = useLanguage();
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
      imagePng: spool.imagePng,
    });
  };

  const cancelForm = () => {
    setEditingId(null);
    setForm(EMPTY_FORM);
  };

  const handlePickImage = () => {
    invoke<string | null>('pick_and_read_image')
      .then((base64) => {
        if (base64 === null) return;
        setForm((prev) => ({ ...prev, imagePng: base64 }));
      })
      .catch((e) => setError(String(e)));
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
      imagePng: form.imagePng,
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
        if (editingId === id) cancelForm();
        refresh();
      })
      .catch((e) => setError(String(e)));
  };

  return (
    <div className="flex-1 min-w-0 flex flex-col min-h-0">
      <div className="flex-none px-4 py-3 border-b border-[var(--line)] text-[length:var(--font-size-body)] font-semibold">
        {t('filamentDialogTitle')}
      </div>

      <div className="flex-1 overflow-y-auto p-4">
        {error && (
          <div className="pb-2 text-[length:var(--font-size-title)] text-[var(--accent)] break-words">
            {t('filamentError')} {error}
          </div>
        )}
        {spools.length === 0 ? (
          <div className="text-[length:var(--font-size-title)] text-[var(--ink-3)]">{t('filamentEmptyState')}</div>
        ) : (
          <div className="grid gap-3.5" style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(178px, 1fr))' }}>
            {spools.map((spool) => (
              <div key={spool.id} className="rounded-[4px] overflow-hidden border border-[var(--line)]">
                <div className="relative aspect-square bg-[var(--plate)] border-b border-[var(--line)] overflow-hidden">
                  {spool.imagePng ? (
                    <img
                      src={`data:image/png;base64,${spool.imagePng}`}
                      className="absolute inset-0 w-full h-full object-cover"
                    />
                  ) : (
                    <>
                      <div
                        className="absolute inset-0 opacity-90"
                        style={{
                          backgroundImage:
                            'repeating-linear-gradient(135deg, var(--hatch) 0 1px, transparent 1px 9px)',
                        }}
                      />
                      <div className="absolute inset-0 grid place-items-center px-2">
                        <span className="text-[length:var(--font-size-body)] font-semibold text-center truncate">{spool.material}</span>
                      </div>
                    </>
                  )}
                </div>
                <div className="flex flex-col gap-1 px-2.5 py-2 bg-[var(--panel)]">
                  <div className="text-[11.5px] text-[var(--ink-2)] truncate">
                    {[spool.manufacturer, spool.color].filter(Boolean).join(' · ') || t('noValue')}
                  </div>
                  <div className="font-mono-ui text-[length:var(--font-size-meta)] text-[var(--ink-3)]">
                    {formatWeightG(spool.remainingWeightG, language)} / {formatWeightG(spool.originalWeightG, language)}
                  </div>
                  <div className="font-mono-ui text-[length:var(--font-size-meta)] text-[var(--ink-3)]">
                    {formatDiameterMm(spool.diameterMm, language)}
                    {spool.price !== null ? ` · ${formatPrice(spool.price, language)}` : ''}
                  </div>

                  {confirmDeleteId === spool.id ? (
                    <div className="flex items-center gap-1.5 pt-1">
                      <span className="flex-1 text-[10.5px] text-[var(--ink)]">{t('deleteConfirmQuestion')}</span>
                      <button
                        onClick={() => setConfirmDeleteId(null)}
                        className="h-6 px-1.5 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[length:var(--font-size-meta)] cursor-pointer"
                      >
                        {t('cancel')}
                      </button>
                      <button
                        onClick={() => deleteSpool(spool.id)}
                        className="h-6 px-1.5 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[length:var(--font-size-meta)] cursor-pointer"
                      >
                        {t('delete')}
                      </button>
                    </div>
                  ) : (
                    <div className="flex items-center justify-end gap-1.5 pt-1">
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
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="flex-none px-4 py-3 border-t border-[var(--line)] bg-[var(--panel-2)]">
        <div className="flex items-center gap-2 mb-2">
          <button
            type="button"
            onClick={handlePickImage}
            className="h-8 px-3 rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-[12px] cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            {t('filamentUploadImageLabel')}
          </button>
          {form.imagePng && (
            <img
              src={`data:image/png;base64,${form.imagePng}`}
              className="w-8 h-8 rounded-[3px] object-cover border border-[var(--line)]"
            />
          )}
        </div>
        <div className="grid grid-cols-2 gap-2 mb-2">
          <AutocompleteInput
            value={form.material}
            onChange={(v) => setForm({ ...form, material: v })}
            options={FILAMENT_MATERIALS}
            placeholder={t('filamentMaterialLabel')}
            className={fieldClass}
          />
          <AutocompleteInput
            value={form.manufacturer}
            onChange={(v) => setForm({ ...form, manufacturer: v })}
            options={FILAMENT_MANUFACTURERS}
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
              className="flex-1 h-8 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[length:var(--font-size-title)] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
            >
              {t('cancel')}
            </button>
          )}
          <button
            onClick={submitForm}
            disabled={!form.material.trim()}
            className="flex-1 h-8 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[length:var(--font-size-title)] font-semibold cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
          >
            {editingId ? t('filamentSaveButton') : t('filamentAddButton')}
          </button>
        </div>
      </div>
    </div>
  );
}
