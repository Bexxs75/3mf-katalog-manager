import { useLanguage, useT } from '../i18n/LanguageContext';
import { filamentUsageHelpUrl } from '../lib/filamentUsageHelp';
import { ExternalLink } from './ExternalLink';

// Explains why a 3MF shows no filament usage, so an empty spot is not
// mistaken for a bug of the app.
export function FilamentUsageMissing() {
  const t = useT();
  const { language } = useLanguage();
  return (
    <div className="rounded-[10px] border border-[var(--line)] bg-[var(--panel)] px-5 py-4">
      <p className="ui-label text-[var(--ink-3)] mb-3">{t('filamentUsageMissingHeading')}</p>
      <div role="status" className="rounded-md border border-[var(--line)] bg-[var(--panel-2)] px-3 py-2.5 text-body text-[var(--ink-2)]">
        <b className="font-semibold text-[var(--ink)]">{t('filamentUsageMissingTitle')}</b>{' '}
        {t('filamentUsageMissingBody')}{' '}
        <ExternalLink url={filamentUsageHelpUrl(language)} label={t('filamentUsageMissingLink')}
          className="underline decoration-[var(--line-strong)] underline-offset-2 text-[var(--accent)]">
          {t('filamentUsageMissingLink')}
        </ExternalLink>
      </div>
    </div>
  );
}
