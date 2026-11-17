import { useState } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { currentVersion, releaseNotes } from '../lib/releaseNotes';
import { openReleaseUrl } from '../lib/api/update';
import { toAppError, type AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';
import { CatalogActionDialog, catalogActionButton } from './CatalogActionDialog';

export function WhatsNewDialog({ versions, onClose }: { versions: string[]; onClose: () => void }) {
  const t = useT();
  const { language } = useLanguage();
  const [error, setError] = useState<AppError | null>(null);
  return <CatalogActionDialog title={versions.length > 1 ? t('whatsNew') : t('whatsNewTitle').replace('{version}', currentVersion)} onClose={onClose} closeLabel={t('impClose')}
    footer={<div className="flex justify-end"><button data-initial-focus className={catalogActionButton} onClick={onClose}>{t('whatsNewClose')}</button></div>}>
    {versions.map(version => {
      const entry = releaseNotes[version];
      if (!entry) return null;
      const notes = entry.notes[language] ?? entry.notes.en;
      const url = `https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/v${version}`;
      return <section key={version} className="space-y-3">
        <h3 className="font-semibold">{version} · <time dateTime={entry.date}>{new Intl.DateTimeFormat(language, { dateStyle: 'long', timeZone: 'UTC' }).format(new Date(entry.date))}</time></h3>
        {(['new', 'improved', 'fixed'] as const).map(group => notes[group].length > 0 && <div key={group}>
          <h4 className="font-semibold">{t(({ new: 'whatsNewNew', improved: 'whatsNewImproved', fixed: 'whatsNewFixed' } as const)[group])}</h4>
          <ul className="list-disc pl-5 space-y-1">{notes[group].map((text, index) => <li key={index}>{text}</li>)}</ul>
        </div>)}
        <a className="text-[var(--accent)] underline" href={url} target="_blank" rel="noreferrer" onClick={event => {
          if (import.meta.env.VITE_TARGET === 'web') return;
          event.preventDefault();
          void openReleaseUrl(url).catch(e => setError(toAppError(e)));
        }}>{t('whatsNewChangelog')}</a>
      </section>;
    })}
    {error && <div role="alert"><ErrorText error={error} /></div>}
  </CatalogActionDialog>;
}
