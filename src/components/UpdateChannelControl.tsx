import { useEffect, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import * as api from '../lib/api/updater';
import { CatalogActionDialog, catalogActionButton } from './CatalogActionDialog';
import { ErrorText } from '../diagnostics/ErrorText';
import { toAppError, type AppError } from '../lib/errors';

export function UpdateChannelControl({ disabled, onExport, onChanged, exportError }: {
  disabled: boolean;
  exportError?: AppError | null;
  onExport: () => Promise<boolean>;
  onChanged: () => void;
}) {
  const t = useT();
  const [channel, setChannel] = useState<api.UpdateChannel | null>(null);
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);
  const [exported, setExported] = useState(false);
  const [exportIncomplete, setExportIncomplete] = useState(false);
  const [disabledNotice, setDisabledNotice] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  useEffect(() => {
    api.getUpdateChannel().then(setChannel).catch(e => setError(toAppError(e)));
  }, []);
  async function change(next: api.UpdateChannel) {
    setBusy(true);
    setError(null);
    try {
      // A failed install can leave a pending package from the previous channel.
      await api.discardAppUpdate();
      await api.setUpdateChannel(next);
      setChannel(next);
      setConfirm(false);
      setDisabledNotice(next === 'stable');
      onChanged();
    } catch (e) { setError(toAppError(e)); }
    finally { setBusy(false); }
  }
  async function exportCatalog() {
    setBusy(true);
    setError(null);
    setExported(false);
    setExportIncomplete(false);
    try {
      const saved = await onExport();
      setExported(saved);
      setExportIncomplete(!saved);
    } catch (e) {
      setError(toAppError(e));
    } finally { setBusy(false); }
  }
  return <div className="my-3 text-small space-y-2">
    <label className="flex items-center gap-2">
      <input type="checkbox" checked={channel === 'rc'} disabled={disabled || busy || channel === null}
        onChange={() => {
          if (channel === 'rc') void change('stable');
          else { setError(null); setExported(false); setExportIncomplete(false); setConfirm(true); }
        }} />
      {t('updateChannelLabel')}
    </label>
    <p className="text-[var(--ink-3)]">{t('updateChannelHelp')}</p>
    {disabledNotice && <p role="status">{t('updateChannelDisabled')}</p>}
    {error && !confirm && <ErrorText error={error} />}
    {confirm && <CatalogActionDialog title={t('updateChannelLabel')} onClose={() => { if (!busy) setConfirm(false); }}>
      <p>{t('updateChannelWarning')}</p>
      {exported && <p role="status">{t('updateChannelExported')}</p>}
      {(error || exportIncomplete) && <div role="alert" className="p-3 border border-[var(--crit)] rounded text-body">
        {error ? <ErrorText error={error} /> : exportError ? <ErrorText error={exportError} /> : t('updateChannelExportIncomplete')}
      </div>}
      <div className="flex flex-wrap gap-2">
        <button className={catalogActionButton} disabled={busy} onClick={() => void exportCatalog()}>{t('updateChannelExport')}</button>
        <button className={catalogActionButton} disabled={busy || disabled} onClick={() => void change('rc')}>{t(exported ? 'updateChannelActivateNow' : 'updateChannelActivate')}</button>
        <button className={catalogActionButton} data-initial-focus disabled={busy} onClick={() => setConfirm(false)}>{t('cancel')}</button>
      </div>
    </CatalogActionDialog>}
  </div>;
}
