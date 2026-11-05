import { useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { usePrintTechnology } from '../hooks/PrintTechnologyContext';
import { toAppError, type AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';
import type { PrintTechnology } from '../lib/printTechnology';
import { CatalogActionDialog, catalogActionButton } from './CatalogActionDialog';
import { PrintTechnologyChoice, usePrintTechnologyEffect } from './PrintTechnologyChoice';

/**
 * The question on the first start. Closing without applying keeps the current
 * choice ("both") and counts as answered, so it is not asked again.
 */
export function PrintTechnologyDialog({ onDone }: { onDone: () => void }) {
  const t = useT();
  const { technology, save } = usePrintTechnology();
  const [value, setValue] = useState<PrintTechnology>(technology);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const effect = usePrintTechnologyEffect(value);
  const apply = async (choice: PrintTechnology) => {
    setBusy(true);
    setError(null);
    try {
      await save(choice);
      onDone();
    } catch (e) {
      setError(toAppError(e));
      setBusy(false);
    }
  };
  return (
    <CatalogActionDialog title={t('ptTitle')} busy={busy} wide onClose={() => void apply(technology)}>
      <p className="text-[var(--ink-2)]">{t('ptIntro')}</p>
      <PrintTechnologyChoice value={value} onChange={setValue} variant="tiles" disabled={busy} describedBy="pt-dialog-effect" />
      <p id="pt-dialog-effect" className="text-small text-[var(--ink-2)]">{effect} {t('ptNothingDeleted')}</p>
      {error && <div role="alert" className="p-3 border border-[var(--crit)] rounded text-body">{t('ptSaveFailed')} <ErrorText error={error} /></div>}
      <div className="flex justify-end">
        <button type="button" className={catalogActionButton} disabled={busy} onClick={() => void apply(value)}>{t('ptApply')}</button>
      </div>
    </CatalogActionDialog>
  );
}
