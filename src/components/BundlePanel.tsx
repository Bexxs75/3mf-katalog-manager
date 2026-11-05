import { useEffect, useId, useMemo, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from 'react';
import { useCanEditMetadata } from '../lib/webSession';
import { displayName } from '../lib/displayName';
import { formatBytes, formatPrintDuration, formatVolumeCm3, formatWeightG } from '../i18n/format';
import { useFormatCount, useLanguage, useT } from '../i18n/LanguageContext';
import { useModelImages } from '../hooks/useModelImages';
import type { DisplayPreference } from '../hooks/useDisplayPreference';
import { resolveDisplayImage } from '../lib/resolveDisplayImage';
import { commonTags, favoriteFlips, memberTitle, notQueued, panelSummary, printStatusFlips, type PanelMember } from '../lib/bundlePanel';
import { tagLabel } from '../lib/autoTags';
import { modelFileType } from '../lib/stlExport';
import type { BundleCard } from '../lib/bundles';
import type { ModelFile, ModelImages } from '../types';
import { useBundleText } from './BundleCardParts';
import { Icon } from './Icon';
import { LicenseEditor } from './LicenseEditor';
import { TagInput } from './TagInput';

/** What the bundle views (panel, detail page) can ask the app to do with a whole bundle. */
export interface BundleViewActions {
  onEdit: (bundleId: string, returnTo: HTMLElement | null) => void;
  onDissolve: (bundleId: string) => void;
  /** Takes one file out of its bundle (the file stays in the catalog). */
  onRemoveMember: (fileId: string) => void;
  /** The bundle whose "Which variant?" question is open, set by the card menu as well as by the panel. */
  queueAskBundleId: string | null;
  setQueueAskBundleId: (bundleId: string | null) => void;
}

interface Props {
  card: BundleCard;
  /** Every living member, also those a filter hides. */
  members: PanelMember[];
  allTags: string[];
  displayPreference: DisplayPreference;
  actions: BundleViewActions;
  onOpenMember: (fileId: string) => void;
  onSetPrinted: (models: ModelFile[], printed: boolean) => void;
  onSetFavorite: (models: ModelFile[], favorite: boolean) => void;
  onAddTag: (models: ModelFile[], tag: string) => void;
  onSetLicense?: (ids: string[], license: string | null) => Promise<void>;
  onQueue: (models: ModelFile[]) => void;
}

const BUTTON = 'h-7 px-2.5 rounded-[5px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] text-small font-semibold cursor-pointer ' +
  'hover:border-[var(--accent)] hover:text-[var(--accent)] disabled:opacity-50 disabled:cursor-default disabled:hover:border-[var(--line-strong)] disabled:hover:text-[var(--ink-2)] ' +
  'focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-[var(--accent)]';
const PRIMARY = BUTTON.replace('bg-[var(--panel)] text-[var(--ink-2)]', 'bg-[var(--accent)] border-[var(--accent)] text-[var(--accent-ink)]')
  .replace('hover:border-[var(--accent)] hover:text-[var(--accent)]', 'hover:opacity-90');
const SECTION = 'border-t border-[var(--line)] px-4 pt-3.5 pb-3';
const HEADING = 'm-0 text-caption font-semibold uppercase tracking-[0.07em] text-[var(--ink-3)]';
const ROW = 'flex items-baseline gap-3 py-1.5 border-b border-[var(--line)] text-small';

function StatusPill({ printed }: { printed: boolean }) {
  const t = useT();
  return <span className={`flex-none inline-flex items-center h-[22px] px-2 rounded-full border text-caption font-semibold ${printed
    ? 'border-[var(--good)] bg-[var(--good-soft)] text-[var(--good)]'
    : 'border-[var(--line-strong)] bg-[var(--panel-2)] text-[var(--ink-2)]'}`}>
    {printed ? t('printStatusPrinted') : t('printStatusNotPrinted')}
  </span>;
}

function MemberThumb({ model, displayPreference, images }: { model: ModelFile; displayPreference: DisplayPreference; images: ModelImages | undefined }) {
  const source = resolveDisplayImage(model, displayPreference, images);
  return <span aria-hidden="true" className="relative flex-none w-10 h-10 rounded border border-[var(--line)] bg-[var(--plate)] overflow-hidden">
    {source && <img draggable={false} src={source} alt="" className="absolute inset-0 w-full h-full object-cover" />}
  </span>;
}

/** Panel of a bundle card: members, combined values and the actions that reach every member. */
export function BundlePanel({ card, members, allTags, displayPreference, actions, onOpenMember, onSetPrinted, onSetFavorite, onAddTag, onSetLicense, onQueue }: Props) {
  const t = useT();
  const formatCount = useFormatCount();
  const { language } = useLanguage();
  const canEdit = useCanEditMetadata();
  const text = useBundleText();
  const bundle = card.bundle;
  const models = useMemo(() => members.map(member => member.model), [members]);
  const summary = useMemo(() => panelSummary(bundle, models), [bundle, models]);
  const images = useModelImages(useMemo(() => members.map(member => member.model.id), [members]));
  const titleModel = card.title;
  const titleMember = members.find(member => member.model.id === titleModel.id);
  const count = models.length;
  const named = (key: 'bundleMarkAllPrinted' | 'bundleMarkAllNotPrinted' | 'bundleFavoriteAll' | 'bundleUnfavoriteAll' | 'bundleQueueAll') =>
    t(key).replace('{count}', String(count));

  const [status, setStatus] = useState('');
  const [tagOpen, setTagOpen] = useState(false);
  const [licenseOpen, setLicenseOpen] = useState(false);
  const [variantId, setVariantId] = useState('');
  const tagButton = useRef<HTMLButtonElement>(null);
  const licenseButton = useRef<HTMLButtonElement>(null);
  const queueButton = useRef<HTMLButtonElement>(null);
  const askRef = useRef<HTMLDivElement>(null);
  const askId = useId();
  const queueAsk = actions.queueAskBundleId === bundle.id;

  // Another bundle's panel starts clean: no leftover message, no open editor.
  useEffect(() => { setStatus(''); setTagOpen(false); setLicenseOpen(false); }, [bundle.id]);
  // The question starts on the first variant that is not queued yet.
  useEffect(() => {
    if (!queueAsk) return;
    const first = members.find(member => member.model.queuePosition === null) ?? members[0];
    setVariantId(first?.model.id ?? '');
  }, [queueAsk, members]);
  useEffect(() => {
    if (queueAsk) askRef.current?.querySelector<HTMLElement>('input, button')?.focus();
  }, [queueAsk]);

  const allPrinted = models.length > 0 && models.every(model => model.printStatus === 'printed');
  const allFavorite = models.length > 0 && models.every(model => model.favorite);
  const pendingQueue = notQueued(models);
  const closeAsk = () => { actions.setQueueAskBundleId(null); queueButton.current?.focus(); };
  const askKeys = (event: ReactKeyboardEvent) => {
    if (event.key !== 'Escape') return;
    event.preventDefault();
    closeAsk();
  };

  const togglePrinted = () => {
    const printed = !allPrinted;
    const flips = printStatusFlips(models, printed);
    if (flips.length > 0) onSetPrinted(flips, printed);
    setStatus(t(printed ? 'bundleAppliedPrinted' : 'bundleAppliedNotPrinted').replace('{count}', String(count)));
  };
  const toggleFavorite = () => {
    const favorite = !allFavorite;
    const flips = favoriteFlips(models, favorite);
    if (flips.length > 0) onSetFavorite(flips, favorite);
    setStatus(t(favorite ? 'bundleAppliedFavorite' : 'bundleAppliedUnfavorite').replace('{count}', String(count)));
  };
  const queueModels = (chosen: ModelFile[]) => {
    const fresh = notQueued(chosen);
    if (fresh.length > 0) onQueue(fresh);
    setStatus(fresh.length > 0 ? formatCount(t('bundleAppliedQueue'), fresh.length) : t('bundleAppliedQueueNone'));
    closeAsk();
  };

  const meta = (value: string, note?: string) => <span className="flex-1 min-w-0 text-right font-medium tabular-nums">{value}{note && <span className="text-[var(--ink-3)]"> {note}</span>}</span>;
  const row = (label: string, content: React.ReactNode) => <div className={ROW}><span className="flex-none min-w-[96px] text-[var(--ink-2)]">{label}</span>{content}</div>;
  const atLeast = (value: string, complete: boolean) => complete ? value : t('bundleAtLeast').replace('{value}', value);
  const perMember = <span className="flex-1 min-w-0 text-right text-[var(--ink-3)]">{t('bundlePerVariant')}</span>;

  return (
    <div data-bundle-panel className="flex flex-col min-w-0">
      <div className="px-4 pt-3.5">
        <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded border border-[var(--line-strong)] bg-[var(--panel)] text-small font-bold text-[var(--ink-2)]">
          <Icon name="layers" size={13} />{t('bundlePanelKind').replace('{kind}', t(bundle.kind === 'variants' ? 'bundleKindVariants' : 'bundleKindParts'))}
        </span>
      </div>
      <div className="relative aspect-[4/3] mt-3 bg-[var(--plate)] border-y border-[var(--line)] overflow-hidden">
        {(() => {
          const source = resolveDisplayImage(titleModel, displayPreference, images.get(titleModel.id));
          return source ? <img draggable={false} src={source} alt="" className="absolute inset-0 w-full h-full object-contain" />
            : <div className="absolute inset-0 grid place-items-center"><div className="w-[52px] h-[52px] border border-dashed border-[var(--line-strong)] rotate-45" /></div>;
        })()}
      </div>
      <div className="px-4 py-3.5">
        <h2 className="m-0 text-[length:var(--font-size-title)] font-bold break-words">{displayName(bundle.name)}</h2>
        <p className="mt-1 mb-0 text-small text-[var(--ink-3)]">
          {text.summary(card)} · {t('bundlePanelTitleModel').replace('{name}', displayName(titleMember ? memberTitle(titleMember) : titleModel.name))}
        </p>
        <div className="flex flex-wrap gap-1.5 mt-2.5">
          <button type="button" className={BUTTON} onClick={() => onOpenMember(titleModel.id)}>{t('bundleOpenTitle')}</button>
          {canEdit && <>
            <button type="button" className={BUTTON} onClick={event => actions.onEdit(bundle.id, event.currentTarget)}>{t('bundleEdit')}</button>
            <button type="button" className={BUTTON} onClick={() => actions.onDissolve(bundle.id)}>{t('bundleDissolve')}</button>
          </>}
        </div>
      </div>

      <section className={SECTION} aria-labelledby={`${askId}-members`}>
        <h3 id={`${askId}-members`} className={HEADING}>{t('bundleMembers')}</h3>
        <ul className="list-none m-0 mt-2 p-0 flex flex-col" aria-labelledby={`${askId}-members`}>
          {members.map(member => {
            const model = member.model;
            const details = [model.name, modelFileType(model).toUpperCase(), formatBytes(model.fileSizeBytes, language),
              ...(model.printFacts?.printSeconds !== undefined ? [formatPrintDuration(model.printFacts.printSeconds, language, t)] : [])].join(' · ');
            return (
              <li key={model.id}>
                <button type="button" data-open-member={model.id} onClick={() => onOpenMember(model.id)}
                  className="w-full flex items-center gap-2.5 py-1.5 px-1 text-left rounded border-0 bg-transparent cursor-pointer hover:bg-[var(--panel-2)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]">
                  <MemberThumb model={model} displayPreference={displayPreference} images={images.get(model.id)} />
                  <span className="flex-1 min-w-0">
                    <span className="block font-semibold text-small truncate">{displayName(memberTitle(member))}
                      {member.isTitle && <span className="ml-1.5 align-middle text-caption font-semibold px-1.5 rounded-full border border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]">{t('bundleDlgTitleBadge')}</span>}
                    </span>
                    <span className="block text-caption text-[var(--ink-3)] truncate" title={details}>{details}</span>
                  </span>
                  <StatusPill printed={model.printStatus === 'printed'} />
                </button>
              </li>
            );
          })}
        </ul>
      </section>

      <section className={SECTION} aria-labelledby={`${askId}-summary`}>
        <h3 id={`${askId}-summary`} className={HEADING}>{t('bundleSummary')}</h3>
        <div className="mt-1">
          {row(t('bundleSumSize'), meta(formatBytes(summary.sizeBytes, language), t('bundleSumTotal')))}
          {summary.summable ? <>
            {row(t('metaVolume'), meta(formatVolumeCm3(summary.volumeCm3, language)))}
            {row(t('printDataTime'), meta(summary.printSeconds === null ? t('noValue') : atLeast(formatPrintDuration(summary.printSeconds, language, t), summary.printSecondsComplete)))}
            {row(t('metaWeight'), meta(summary.weightG === null ? t('noValue') : atLeast(formatWeightG(summary.weightG, language), summary.weightComplete)))}
            {row(t('metaMaterial'), meta(summary.materials.length > 0 ? summary.materials.join(', ') : t('noValue')))}
          </> : <>
            {row(t('printDataTime'), perMember)}
            {row(t('metaWeight'), perMember)}
          </>}
          {row(t('printStatusFilter'), meta(t('bundlePrintedOf').replace('{printed}', String(summary.printedCount)).replace('{total}', String(summary.count))))}
          {row(t('licenseHeading'), <span className="flex-1 min-w-0 text-right font-medium">
            {summary.license.state === 'mixed' ? <span className="inline-flex items-center h-[22px] px-2 rounded-full border border-[var(--warn)] bg-[var(--warn-soft)] text-[var(--warn)] text-caption font-semibold">{t('bundleLicenseMixed')}</span>
              : summary.license.state === 'single' ? summary.license.value : <span className="text-[var(--ink-3)]">{t('licenseUnset')}</span>}
          </span>)}
        </div>
        <h4 className="mt-2.5 mb-1 text-caption font-semibold text-[var(--ink-2)]">{t('bundleSumTags')}</h4>
        {summary.tags.length > 0
          ? <ul className="list-none m-0 p-0 flex flex-wrap gap-1">{summary.tags.map(tag =>
            <li key={tag} className="text-caption font-medium px-1.5 py-px rounded-full bg-[var(--panel-2)] border border-[var(--line)] text-[var(--ink-2)]">#{tagLabel(tag, language)}</li>)}</ul>
          : <p className="m-0 text-small text-[var(--ink-3)]">{t('noValue')}</p>}
      </section>

      <section className={SECTION} aria-labelledby={`${askId}-apply`}>
        {canEdit ? <>
          <h3 id={`${askId}-apply`} className={HEADING}>{t('bundleApplyHeading').replace('{count}', String(count))}</h3>
          <div className="flex flex-wrap gap-1.5 mt-2.5">
            <button type="button" className={BUTTON} onClick={togglePrinted}>{named(allPrinted ? 'bundleMarkAllNotPrinted' : 'bundleMarkAllPrinted')}</button>
            <button type="button" className={BUTTON} onClick={toggleFavorite}>{named(allFavorite ? 'bundleUnfavoriteAll' : 'bundleFavoriteAll')}</button>
            <button ref={tagButton} type="button" className={BUTTON} aria-expanded={tagOpen} onClick={() => { setTagOpen(open => !open); setLicenseOpen(false); }}>{t('bundleApplyTag')}</button>
            {onSetLicense && <button ref={licenseButton} type="button" className={BUTTON} aria-expanded={licenseOpen} onClick={() => { setLicenseOpen(open => !open); setTagOpen(false); }}>{t('bundleApplyLicense')}</button>}
            <button ref={queueButton} type="button" className={BUTTON} aria-expanded={queueAsk}
              onClick={() => actions.setQueueAskBundleId(queueAsk ? null : bundle.id)}>{t('bundleQueueOpen')}</button>
          </div>
          {tagOpen && (
            <div className="mt-2.5 p-2.5 border border-[var(--line)] rounded-md bg-[var(--panel-2)]">
              <p className="m-0 mb-1.5 text-small font-semibold">{t('bundleApplyTagLabel').replace('{count}', String(count))}</p>
              <TagInput autoFocus allTags={allTags} tags={commonTags(models)} stripHash
                onEscape={() => { setTagOpen(false); tagButton.current?.focus(); }}
                onAddTag={tag => {
                  onAddTag(models.filter(model => !model.tags.includes(tag)), tag);
                  setStatus(t('bundleAppliedTag').replace('{tag}', tagLabel(tag, language)).replace('{count}', String(count)));
                  setTagOpen(false);
                  tagButton.current?.focus();
                }} />
            </div>
          )}
          {licenseOpen && onSetLicense && (
            <div className="mt-2.5 px-2.5 border border-[var(--line)] rounded-md bg-[var(--panel-2)]" onKeyDown={event => { if (event.key === 'Escape') { event.preventDefault(); setLicenseOpen(false); licenseButton.current?.focus(); } }}>
              <LicenseEditor value={null} bulk onSave={async value => {
                await onSetLicense(models.map(model => model.id), value);
                setStatus(t('bundleAppliedLicense').replace('{count}', String(count)));
                setLicenseOpen(false);
                licenseButton.current?.focus();
              }} />
            </div>
          )}
          {queueAsk && (
            <div ref={askRef} role="group" aria-labelledby={`${askId}-queue`} onKeyDown={askKeys}
              className="mt-2.5 p-2.5 border border-[var(--line)] rounded-md bg-[var(--panel-2)]">
              {bundle.kind === 'variants' ? <>
                <div id={`${askId}-queue`} className="font-semibold mb-1.5">{t('bundleQueueWhich')}</div>
                <div className="flex flex-col gap-1.5">
                  {members.map(member => (
                    <label key={member.model.id} className={`flex items-center gap-2.5 border rounded-md px-2.5 py-1.5 cursor-pointer bg-[var(--panel)] ${variantId === member.model.id ? 'border-[var(--accent)] bg-[var(--accent-soft)]' : 'border-[var(--line)]'}`}>
                      <input type="radio" name={`${askId}-variant`} value={member.model.id} checked={variantId === member.model.id}
                        className="accent-[var(--accent)]" onChange={() => setVariantId(member.model.id)} />
                      <span className="font-semibold text-small">{displayName(memberTitle(member))}
                        {member.model.queuePosition !== null && <span className="ml-1.5 font-normal text-[var(--ink-3)]">{t('bundleQueueIn')}</span>}
                      </span>
                    </label>
                  ))}
                </div>
                <div className="flex flex-wrap gap-1.5 mt-2.5">
                  <button type="button" className={PRIMARY} disabled={!variantId || members.find(member => member.model.id === variantId)?.model.queuePosition !== null}
                    onClick={() => queueModels(models.filter(model => model.id === variantId))}>{t('bundleQueueAddVariant')}</button>
                  <button type="button" className={BUTTON} onClick={closeAsk}>{t('cancel')}</button>
                </div>
              </> : <>
                <div id={`${askId}-queue`} className="mb-1.5">{t('bundleQueuePartsNote').replace('{count}', String(count))}</div>
                <div className="flex flex-wrap gap-1.5">
                  <button type="button" className={PRIMARY} disabled={pendingQueue.length === 0} onClick={() => queueModels(models)}>{named('bundleQueueAll')}</button>
                  <button type="button" className={BUTTON} onClick={closeAsk}>{t('cancel')}</button>
                </div>
              </>}
            </div>
          )}
        </> : (
          <p className="m-0 p-2.5 border border-[var(--line)] rounded-md bg-[var(--panel-2)] text-small text-[var(--ink-2)]">{t('bundleViewOnly')}</p>
        )}
        <p role="status" className="mt-1.5 mb-0 text-caption text-[var(--ink-2)] min-h-[1em]">{status}</p>
      </section>
    </div>
  );
}
