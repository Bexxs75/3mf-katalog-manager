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
  const { technology, save } = usePrintTechnology();
  const web = import.meta.env.VITE_TARGET === 'web';
  const [saved, setSaved] = useState<PrintTechnology | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const effect = usePrintTechnologyEffect(technology);
  const change = async (value: PrintTechnology) => {
    setError(null);
    setSaved(null);
    try {
      await save(value);
      setSaved(value);
    } catch (e) {
      setError(toAppError(e));
    }
  };
  const label = t(technology === 'filament' ? 'ptFilament' : technology === 'resin' ? 'ptResin' : 'ptBoth');
  return (
    <fieldset className="mb-4" aria-describedby="pt-settings-help">
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
  );
}
