import { SettingRow } from './SettingsTools';
import { SETTING_DEFAULTS } from '../lib/settingsSession';
import { useEffect, useState } from 'react';
import { invoke } from '../lib/transport';
import { useT } from '../i18n/LanguageContext';
import { ErrorText } from '../diagnostics/ErrorText';
import { toAppError, type AppError } from '../lib/errors';
import { ToggleSwitch } from './ToggleSwitch';

export function ImportRenameSetting() {
  const t = useT();
  const [loaded, setLoaded] = useState(false);
  const [enabled, setEnabled] = useState(false);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState<AppError | null>(null);
  useEffect(() => {
    let active = true;
    invoke<boolean>('get_import_rename_enabled')
      .then(value => { if (active) { setEnabled(value === true); setLoaded(true); } })
      .catch(e => { if (active) setError(toAppError(e)); })
      .finally(() => { if (active) { setLoading(false); setBusy(false); } });
    return () => { active = false; };
  }, []);
  const change = async (value: boolean) => {
    setBusy(true);
    setError(null);
    try {
      await invoke('set_import_rename_enabled', { enabled: value });
      setEnabled(value); setLoaded(true);
    } catch (e) {
      setError(toAppError(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div>
      <SettingRow id="importRename" value={loaded ? enabled : undefined} label={t('importRenameTitle')} onReset={() => change(SETTING_DEFAULTS.importRename)} disabled={!loaded || loading || busy}>
        <div className="flex items-center justify-between gap-2 text-small">
          <span>{t('importRenameTitle')}</span>
          <ToggleSwitch checked={enabled} label={t('importRenameTitle')} disabled={loading} busy={busy} onChange={change} />
        </div>
        <p className="mt-1 text-small text-[var(--ink-2)]">{t('importRenameDescription')}</p>

      </SettingRow>
      {error && <p role="alert" className="text-small text-[var(--crit)]"><ErrorText error={error} /></p>}
    </div>
  );
}
