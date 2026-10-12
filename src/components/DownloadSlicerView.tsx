import { displayName } from '../lib/displayName';
import { useEffect, useId, useState } from 'react';
import { useRuntimeEnvironment } from '../hooks/useRuntimeEnvironment';
import { useT } from '../i18n/LanguageContext';
import { getCatalogWebLink, type CatalogWebLink } from '../lib/api/runtime';
import { ExternalLink } from './ExternalLink';

export function DownloadSlicerView() {
  const { container } = useRuntimeEnvironment();
  return container ? <DownloadSlicerContent /> : null;
}

function DownloadSlicerContent() {
  const t = useT();
  const requirementsId = useId();
  const [expanded, setExpanded] = useState(false);
  const [link, setLink] = useState<CatalogWebLink | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    let active = true;
    void getCatalogWebLink().then(result => {
      if (active) setLink(result);
    }).catch(() => { if (active) setFailed(true); });
    return () => { active = false; };
  }, []);

  return <main className="flex-1 min-h-0 overflow-y-auto p-6 text-[var(--ink)]">
    <div className="max-w-3xl space-y-6">
      <h1 className="text-xl font-semibold">{t('railDownloads')}</h1>
      <p>{t('downloadIntro')}</p>
      <div className="space-y-2 rounded-lg border border-[var(--line)] bg-[var(--panel)] p-4">
        {failed ? <p role="alert">{t('catalogWebFailed')}</p> : link ? <>
          {link.url ? <>
            <ExternalLink url={link.url} label={t('railDownloads')} />
            <p className="mt-2">{t('catalogWebPasteHint')}</p>
          </> : <>
            <p className="break-all" style={{ userSelect: 'text' }}>{displayName(link.path)}</p>
            <p>{t('catalogWebUnknown')}</p>
            <p>{t('catalogWebConfigure')}</p>
          </>}
        </> : <p role="status">{t('catalogWebLoading')}</p>}
      </div>
      <section className="space-y-2">
        <h2 className="text-lg font-semibold">{t('downloadSteps')}</h2>
        <ol className="list-decimal pl-6 space-y-2">
          <li>{t('downloadStepOpen')}</li>
          <li>{t('downloadStepFind')}</li>
          <li>{t('downloadStepAction')}</li>
        </ol>
      </section>
      <section className="space-y-2">
        <h2 className="text-lg font-semibold">
          <button type="button" aria-expanded={expanded} aria-controls={requirementsId}
            onClick={() => setExpanded(value => !value)}
            className="rounded border border-[var(--line-strong)] px-3 py-2 cursor-pointer hover:text-[var(--accent)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]">
            <span aria-hidden="true">{expanded ? '−' : '+'} </span>{t('downloadRequirements')}
          </button>
        </h2>
        <ul id={requirementsId} hidden={!expanded} className="list-disc pl-6 space-y-2">
          <li>{t('downloadRequirementInstalled')}</li>
          <li>{t('downloadRequirementReachable')}</li>
          <li>{t('downloadRequirementCertificate')}</li>
          <li>{t('downloadRequirementBambu')}</li>
          <li>{t('downloadRequirementPrusa')}</li>
        </ul>
      </section>
    </div>
  </main>;
}
