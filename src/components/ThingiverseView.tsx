import { useCallback, useEffect, useRef, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { toAppError, type AppError } from '../lib/errors';
import * as api from '../lib/api/thingiverse';
import type { ThingSummary, ThingiverseSettings } from '../types';
import { ExternalLink } from './ExternalLink';
import { ThingiverseBanner } from './ThingiverseBanner';
import { ThingiverseDetail } from './ThingiverseDetail';
import { ThingImage, thingButton, useCopyLink } from './ThingiversePieces';

const primaryButton = 'h-8 px-3 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-small font-semibold cursor-pointer disabled:opacity-50 disabled:cursor-default';

function Card({ thing, image, onNeedImage, onDetails, onCopy }: {
  thing: ThingSummary;
  image: string | null | undefined;
  onNeedImage: (id: number) => void;
  onDetails: () => void;
  onCopy: () => void;
}) {
  const t = useT();
  return (
    <article aria-label={thing.name} className="flex flex-col gap-2 p-2 rounded border border-[var(--line)] bg-[var(--panel)]">
      <ThingImage thing={thing} image={image} onNeedImage={onNeedImage} />
      <div className="min-w-0">
        <div className="font-semibold text-small break-words">{thing.name}</div>
        <div className="text-caption text-[var(--ink-2)]">{thing.authorName ? t('tvAuthorBy').replace('{0}', thing.authorName) : t('tvAuthorUnknown')}</div>
        <span className="inline-block mt-1 px-1.5 rounded border border-[var(--line-strong)] text-caption text-[var(--ink-2)]" title={thing.licenseName ? t('tvLicenseTip') : t('tvLicenseSeeDetails')}>
          {thing.licenseName ?? t('tvLicenseSeeDetails')}
        </span>
      </div>
      <div className="flex flex-wrap gap-1.5 mt-auto">
        <button type="button" className={thingButton} onClick={onDetails} aria-label={`${t('tvDetails')}: ${thing.name}`}>{t('tvDetails')}</button>
        <ExternalLink url={thing.url} label={t('tvOpenLabel').replace('{0}', thing.name)} onOpen={() => void api.openThingiverseThing(thing.id).catch(e => console.error('Opening the Thingiverse page failed', e))} className={thingButton}>{t('tvOpen')}</ExternalLink>
        <button type="button" className={thingButton} onClick={onCopy} aria-label={t('tvCopyLinkLabel').replace('{0}', thing.name)}>{t('tvCopyLink')}</button>
      </div>
    </article>
  );
}

/**
 * The Thingiverse area: search, results, details. Everything shown lives in this component's
 * state and in the backend's memory; leaving the area drops it. Nothing is saved and nothing is downloaded.
 */
export function ThingiverseView({ onOpenSettings }: { onOpenSettings: () => void }) {
  const t = useT();
  const [settings, setSettings] = useState<ThingiverseSettings | null>(null);
  const [query, setQuery] = useState('');
  const [searched, setSearched] = useState('');
  const [items, setItems] = useState<ThingSummary[]>([]);
  const [page, setPage] = useState(0);
  const [hasMore, setHasMore] = useState(false);
  const [phase, setPhase] = useState<'idle' | 'loading' | 'done'>('idle');
  const [error, setError] = useState<AppError | null>(null);
  const [retryIn, setRetryIn] = useState(0);
  const [images, setImages] = useState<Record<number, string | null>>({});
  const [detailId, setDetailId] = useState<number | null>(null);
  const asked = useRef(new Set<number>());
  const mounted = useRef(true);
  const lastAction = useRef<(() => void) | null>(null);
  const { result: copyResult, copy } = useCopyLink();
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);

  const refresh = useCallback(() => api.getThingiverseSettings()
    .then(s => { if (mounted.current) setSettings(s); })
    .catch(e => console.error('Reading the Thingiverse setting failed', e)), []);
  useEffect(() => { void refresh(); }, [refresh]);

  // Rate limit: count down, then lift the banner.
  useEffect(() => {
    if (retryIn <= 0) return;
    const timer = setTimeout(() => setRetryIn(n => n - 1), 1000);
    return () => clearTimeout(timer);
  }, [retryIn]);
  useEffect(() => {
    if (retryIn === 0) setError(e => (e?.message === 'thingiverseRateLimited' ? null : e));
  }, [retryIn]);

  const fail = useCallback((e: unknown) => {
    const err = toAppError(e);
    setError(err);
    if (err.message === 'thingiverseRateLimited') setRetryIn(Math.max(1, Number(err.params?.[0]) || 60));
  }, []);

  const fetchPage = useCallback(async (q: string, pageNo: number, append: boolean) => {
    lastAction.current = () => void fetchPage(q, pageNo, append);
    setPhase('loading'); setError(null);
    try {
      const result = await api.thingiverseSearch(q, pageNo);
      if (!mounted.current) return;
      setItems(prev => (append ? [...prev, ...result.items] : result.items));
      setPage(result.page); setHasMore(result.hasMore); setSearched(q);
      if (!append) { setImages({}); asked.current.clear(); }
    } catch (e) {
      if (mounted.current) fail(e);
    } finally {
      if (mounted.current) { setPhase('done'); void refresh(); }
    }
  }, [fail, refresh]);

  const needImage = useCallback((id: number) => {
    if (asked.current.has(id)) return;
    asked.current.add(id);
    api.thingiverseImage(id, 'thumb')
      .then(data => { if (mounted.current) setImages(prev => ({ ...prev, [id]: data })); })
      // A missing preview is not worth a banner: the card shows its placeholder.
      .catch(() => { if (mounted.current) setImages(prev => ({ ...prev, [id]: null })); })
      .finally(() => { void refresh(); });
  }, [refresh]);

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!query.trim() || searchBlocked) return;
    void fetchPage(query.trim(), 1, false);
  };

  const dailyLimit = error?.message === 'thingiverseDailyLimit';
  const searchBlocked = phase === 'loading' || retryIn > 0 || dailyLimit;
  const shownError = error && error.message === 'thingiverseRateLimited' ? { ...error, params: [String(retryIn)] } : error;
  const detailThing = detailId === null ? undefined : items.find(i => i.id === detailId);

  return (
    <section aria-label={t('tvAreaTitle')} className="flex-1 min-h-0 flex flex-col bg-[var(--bg)]">
      <div className="px-4 py-2 border-b border-[var(--line)] border-l-4 border-l-[var(--accent)] bg-[var(--panel-2)]">
        <div className="flex items-center gap-2">
          <h2 className="text-title font-semibold">{t('tvAreaTitle')}</h2>
          <span className="px-1.5 rounded-full border border-[var(--line-strong)] text-caption text-[var(--ink-2)]">{t('tvExperimental')}</span>
        </div>
        <p className="text-small text-[var(--ink-2)]">{t('tvAreaSource')}</p>
      </div>

      <div className="flex-1 min-h-0 overflow-y-auto scrollbar-stable p-4">
        {settings && !settings.hasToken ? (
          <div className="max-w-[560px] mx-auto text-center space-y-3 py-8">
            <h3 className="text-title font-semibold">{t('tvNoKeyTitle')}</h3>
            <p className="text-[var(--ink-2)]">{t('tvNoKeyText')}</p>
            <div className="flex justify-center gap-2">
              <button type="button" className={primaryButton} onClick={onOpenSettings}>{t('tvOpenSettings')}</button>
              <button type="button" className={thingButton} onClick={onOpenSettings}>{t('tvHowTitle')}</button>
            </div>
          </div>
        ) : detailId !== null ? (
          <ThingiverseDetail
            thingId={detailId}
            summary={detailThing}
            onBack={() => setDetailId(null)}
            onOpenSettings={onOpenSettings}
            copy={copy}
            copyResult={copyResult}
          />
        ) : (
          <>
            <form role="search" onSubmit={submit} className="flex gap-2 mb-3">
              <input
                type="search"
                value={query}
                onChange={e => setQuery(e.target.value)}
                aria-label={t('tvSearchLabel')}
                placeholder={t('tvSearchPlaceholder')}
                maxLength={100}
                aria-invalid={error?.message === 'thingiverseQueryInvalid'}
                className="flex-1 h-8 px-2 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]"
              />
              <button type="submit" className={primaryButton} disabled={searchBlocked}>{t('tvSearchButton')}</button>
            </form>

            {shownError && <ThingiverseBanner error={shownError} onRetry={lastAction.current ?? undefined} onSettings={onOpenSettings} />}
            {copyResult && <p role="status" className="mb-2 text-caption text-[var(--ink-2)]">{copyResult === 'copied' ? t('tvLinkCopied') : t('externalLinkCopyFailed')}</p>}

            {phase === 'idle' && items.length === 0 && !error && (
              <div className="max-w-[560px] mx-auto text-center space-y-2 py-8">
                <h3 className="text-title font-semibold">{t('tvEmptyTitle')}</h3>
                <p className="text-[var(--ink-2)]">{t('tvEmptyText')}</p>
              </div>
            )}
            {phase === 'loading' && items.length === 0 && <div role="status" aria-busy="true" aria-label={t('tvSearching')} className="text-[var(--ink-2)]">{t('tvSearching')} …</div>}
            {phase === 'done' && items.length === 0 && !error && (
              <div role="status" className="max-w-[560px] mx-auto text-center space-y-2 py-8">
                <h3 className="text-title font-semibold">{t('tvNoResultsTitle')}</h3>
                <p className="text-[var(--ink-2)]">{t('tvNoResultsText').replace('{0}', searched)}</p>
              </div>
            )}
            {items.length > 0 && (
              <>
                <p role="status" className="mb-2 text-caption text-[var(--ink-2)]">{t('tvResultSummary').replace('{0}', String(items.length)).replace('{1}', searched)}</p>
                <div className="grid gap-3 grid-cols-[repeat(auto-fill,minmax(210px,1fr))]">
                  {items.map(thing => (
                    <Card key={thing.id} thing={thing} image={images[thing.id]} onNeedImage={needImage}
                      onDetails={() => setDetailId(thing.id)} onCopy={() => void copy(thing.url)} />
                  ))}
                </div>
                {hasMore && (
                  <div className="text-center mt-4">
                    <button type="button" className={thingButton} disabled={searchBlocked} onClick={() => void fetchPage(searched, page + 1, true)}>{t('tvLoadMore')}</button>
                  </div>
                )}
              </>
            )}
          </>
        )}
      </div>

      <div className="px-4 py-1.5 border-t border-[var(--line)] flex flex-wrap justify-between gap-2 text-caption text-[var(--ink-3)]">
        {settings && !settings.hasToken
          ? <span>{t('tvNoKeyFooter')}</span>
          : <>
              <span role="status" className="tabular-nums">
                {settings ? t('tvFootUsage').replace('{0}', String(settings.usageToday)).replace('{1}', String(settings.dailyLimit)) : ''}
              </span>
              <span>{t('tvFootPrivacy')}</span>
            </>}
      </div>
    </section>
  );
}
