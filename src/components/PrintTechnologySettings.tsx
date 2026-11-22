import { SettingRow } from './SettingsTools';
import { SETTING_DEFAULTS } from '../lib/settingsSession';
import { useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { usePrintTechnology } from '../hooks/PrintTechnologyContext';
import { toAppError, type AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';
import type { PrintTechnology } from '../lib/printTechnology';
import { PrintTechnologyChoice, usePrintTechnologyEffect } from './PrintTechnologyChoice';

/** Settings group "What do you print with?". The browser version only shows the choice. */
export function PrintTechnologySettings() {
  const t = useT();
  const { technology, save, loading } = usePrintTechnology();
  const web = import.meta.env.VITE_TARGET === 'web';
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState<PrintTechnology | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const effect = usePrintTechnologyEffect(technology);
  const change = async (value: PrintTechnology) => {
    if (busy || loading) return;
    setBusy(true);
    setError(null);
    setSaved(null);
    try {
      await save(value);
      setSaved(value);
    } catch (e) {
      setError(toAppError(e));
    } finally { setBusy(false); }
  };
  const label = t(technology === 'filament' ? 'ptFilament' : technology === 'resin' ? 'ptResin' : 'ptBoth');
  return (
    <SettingRow id="printMethod" value={loading ? undefined : technology} label={t('ptTitle')} onReset={web ? undefined : () => change(SETTING_DEFAULTS.printMethod)} disabled={web || loading || busy}>
      <fieldset disabled={loading || busy} className="mb-4" aria-describedby="pt-settings-help">
        <legend className="text-[length:var(--font-size-body)] font-semibold mb-2">{t('ptTitle')}</legend>
        {web
          ? <p className="text-small">{t('ptCurrent').replace('{choice}', label)} {t('ptWebReadOnly')}</p>
          : <PrintTechnologyChoice value={technology} onChange={(v) => void change(v)} variant="list" />}
        <p id="pt-settings-help" className="mt-2 text-caption leading-relaxed text-[var(--ink-3)]">
          {effect} {t('ptNothingDeleted')}
        </p>
        <p role="status" className="text-caption text-[var(--ink-2)]">
          {saved && t('ptSaved').replace('{choice}', t(saved === 'filament' ? 'ptFilament' : saved === 'resin' ? 'ptResin' : 'ptBoth'))}
        </p>
        {error && <p role="alert" className="text-caption text-[var(--crit)]">{t('ptSaveFailed')} <ErrorText error={error} /></p>}

      </fieldset>
    </SettingRow>
  );
}
