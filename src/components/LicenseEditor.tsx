import { useEffect, useId, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { useImportLock } from '../hooks/ImportLockContext';
import { messageOf } from '../lib/errors';
import { licenseLabel, licensePresets } from '../lib/licenses';
import { LicenseBadge } from './LicenseBadge';

const controlClass = 'min-w-0 w-full rounded border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] px-2 py-1.5 text-small';

export function LicenseEditor({ value, onSave, onDirtyChange, bulk = false }: {
  value: string | null;
  onSave?: (value: string | null) => Promise<void>;
  bulk?: boolean;
  onDirtyChange?: (dirty: boolean) => void;
}) {
  const t = useT();
  const id = useId();
  const { lockProps } = useImportLock();
  const [savedValue, setSavedValue] = useState(value ?? '');
  const [draft, setDraft] = useState(value ?? '');
  const [other, setOther] = useState(!!value && !licensePresets.some(p => p.code === value));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    setSavedValue(value ?? '');
    setDraft(value ?? '');
    setOther(!!value && !licensePresets.some(p => p.code === value));
  }, [value]);
  const dirty = !!onSave && draft !== savedValue;
  useEffect(() => { onDirtyChange?.(dirty); }, [dirty, onDirtyChange]);
  return <section aria-label={t('licenseHeading')} className="py-3 space-y-2 min-w-0">
    <div className="flex flex-wrap items-center gap-2">
      <label htmlFor={id} className="text-small font-semibold text-[var(--ink-2)]">{t('licenseHeading')}</label>
      <LicenseBadge license={draft || null} showUnknown />
    </div>
    {onSave ? <>
      <select title={licenseLabel(draft, t)} id={id} className={controlClass} value={other ? '__other' : draft} disabled={busy} {...lockProps}
        onChange={event => {
          const next = event.target.value;
          setOther(next === '__other'); setDraft(next === '__other' ? '' : next); setError(null);
        }}>
        <option value="">{t('licenseUnset')}</option>
        {licensePresets.map(preset => <option key={preset.code} value={preset.code}>{t(preset.label)}</option>)}
        <option value="__other">{t('licenseOther')}</option>
      </select>
      {other && <input data-license-draft aria-label={t('licenseCustom')} className={controlClass} value={draft} maxLength={200}
        disabled={busy} {...lockProps} onChange={event => setDraft(event.target.value)} />}
      <button type="button" className="rounded border border-[var(--line-strong)] px-3 py-1.5 text-small text-[var(--ink)] hover:bg-[var(--panel-2)] disabled:opacity-50"
        disabled={busy || (!bulk && !dirty)} {...lockProps}
        onClick={async () => {
          setBusy(true); setError(null);
          try { await onSave(draft || null); setSavedValue(draft); }
          catch (error) { setError(messageOf(error)); }
          finally { setBusy(false); }
        }}>{t('licenseSave')}</button>
      {error && <div role="alert" className="text-small text-[var(--ink)]">{error}</div>}
    </> : <div className="text-small break-words">{licenseLabel(value, t)}</div>}
  </section>;
}
