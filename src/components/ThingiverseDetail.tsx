import { useCallback, useEffect, useRef, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { toAppError, type AppError } from '../lib/errors';
import * as api from '../lib/api/thingiverse';
import type { ThingDetail, ThingSummary } from '../types';
import { ExternalLink } from './ExternalLink';
import { ThingiverseBanner } from './ThingiverseBanner';
import { ThingImage, thingButton } from './ThingiversePieces';

/** The backend cuts descriptions at this many characters. */
const DESCRIPTION_LIMIT = 4000;

/** One thing in detail: large image, author, license, plain-text description. There is no download here. */
export function ThingiverseDetail({ thingId, summary, onBack, onOpenSettings, copy, copyResult }: {
  thingId: number;
  summary: ThingSummary | undefined;
  onBack: () => void;
  onOpenSettings: () => void;
  copy: (url: string) => Promise<void>;
  copyResult: 'copied' | 'failed' | null;
}) {
  const t = useT();
  const [detail, setDetail] = useState<ThingDetail | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [image, setImage] = useState<string | null | undefined>(undefined);
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);

  const load = useCallback(() => {
    setError(null);
    api.thingiverseThing(thingId)
      .then(d => { if (mounted.current) setDetail(d); })
      .catch(e => { if (mounted.current) setError(toAppError(e)); });
  }, [thingId]);
  useEffect(() => { load(); }, [load]);

  const needImage = useCallback(() => {
    api.thingiverseImage(thingId, 'large')
      .then(d => { if (mounted.current) setImage(d); })
      .catch(() => { if (mounted.current) setImage(null); });
  }, [thingId]);

  const back = (
    <div className="mb-2.5">
      <button type="button" onClick={onBack} className="text-[var(--accent)] cursor-pointer hover:underline focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]">
        <span aria-hidden="true">← </span>{t('tvBackToList')}
      </button>
    </div>
  );
  if (error) return <>{back}<ThingiverseBanner error={error} onRetry={load} onSettings={onOpenSettings} onBack={onBack} /></>;
  const thing = detail ?? summary;
  if (!thing) return <>{back}<p role="status">{t('tvDetailLoading')}</p></>;
  const license = thing.licenseName;
  return (
    <>
      {back}
      <article className="grid gap-4 md:grid-cols-[minmax(240px,2fr)_3fr]" aria-label={thing.name}>
        <ThingImage thing={thing} image={image} onNeedImage={needImage} large />
        <div className="flex flex-col gap-3 min-w-0">
          <h3 className="text-title font-semibold break-words">{thing.name}</h3>
          <p>{thing.authorName ? t('tvAuthorBy').replace('{0}', thing.authorName) : t('tvAuthorUnknown')}</p>
          <div className="p-2 rounded border border-[var(--line-strong)] bg-[var(--panel-2)] text-small">
            <b>{license ? t('tvLicenseLine').replace('{0}', license) : t('tvLicenseMissingLine')}</b>
            <br />
            {license ? t('tvLicenseNotice') : t('tvLicenseMissingText')}
          </div>
          {detail ? (
            <>
              <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 text-small">
                {detail.createdAt && <><dt className="text-[var(--ink-3)]">{t('tvPosted')}</dt><dd>{detail.createdAt.slice(0, 10)}</dd></>}
                {detail.likeCount !== null && <><dt className="text-[var(--ink-3)]">{t('tvLikes')}</dt><dd className="tabular-nums">{detail.likeCount}</dd></>}
              </dl>
              {/* Plain text only: React escapes it and the line breaks are kept by CSS. */}
              {detail.description && <div className="whitespace-pre-wrap break-words text-small">{detail.description}</div>}
              {detail.description.length >= DESCRIPTION_LIMIT && <span className="text-caption text-[var(--ink-3)]">{t('tvDescriptionCut')}</span>}
              {detail.tags.length > 0 && (
                <>
                  <ul aria-label={t('tvTagsLabel')} className="flex flex-wrap gap-1.5">
                    {detail.tags.map(tag => <li key={tag} className="px-2 rounded-full border border-[var(--line-strong)] text-caption">{tag}</li>)}
                  </ul>
                  <span className="text-caption text-[var(--ink-3)]">{t('tvTagsNote')}</span>
                </>
              )}
            </>
          ) : (
            <p role="status" className="text-[var(--ink-2)]">{t('tvDetailLoading')}</p>
          )}
          <div className="flex flex-wrap gap-2">
            <ExternalLink url={thing.url} label={t('tvOpenLabel').replace('{0}', thing.name)} onOpen={() => void api.openThingiverseThing(thing.id).catch(e => console.error('Opening the Thingiverse page failed', e))}
              className="h-8 px-3 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-small font-semibold cursor-pointer inline-flex items-center">
              {t('tvOpen')}
            </ExternalLink>
            <button type="button" className={`${thingButton} !h-8 !px-3 !text-small`} onClick={() => void copy(thing.url)}>{t('tvCopyLink')}</button>
          </div>
          {copyResult && <p role="status" className="text-caption text-[var(--ink-2)]">{copyResult === 'copied' ? t('tvLinkCopied') : t('externalLinkCopyFailed')}</p>}
          <p className="p-2 rounded border border-[var(--line)] bg-[var(--panel-2)] text-small">{t('tvNoDownload')}</p>
        </div>
      </article>
    </>
  );
}
