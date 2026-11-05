import { useEffect, useId, useMemo, useRef } from 'react';
import { useCanEditMetadata } from '../lib/webSession';
import { displayName } from '../lib/displayName';
import { formatPrintDuration, formatWeightG } from '../i18n/format';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { memberTitle, panelSummary, type PanelMember } from '../lib/bundlePanel';
import type { Bundle, ModelFile } from '../types';
import type { BundleViewActions } from './BundlePanel';
import { Icon } from './Icon';

/**
 * The detail page of another member replaces this one, so the page (and the button that was
 * pressed) is gone. The bar of the next page puts focus on its current entry instead.
 */
let focusPending = false;

const BUTTON = 'h-7 px-2.5 rounded-[5px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] text-small font-semibold cursor-pointer ' +
  'hover:border-[var(--accent)] hover:text-[var(--accent)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-[var(--accent)]';
const HEADING = 'm-0 text-caption font-semibold uppercase tracking-[0.07em] text-[var(--ink-3)]';

interface Props {
  bundle: Bundle;
  members: PanelMember[];
  currentId: string;
  onOpenMember: (fileId: string) => void;
}

function useOpen(onOpenMember: (fileId: string) => void) {
  return (fileId: string) => {
    focusPending = true;
    onOpenMember(fileId);
  };
}

/** Variants: a switch between the files; parts: the list of parts with the totals. */
export function BundleBar({ bundle, members, currentId, onOpenMember }: Props) {
  const t = useT();
  const { language } = useLanguage();
  const headingId = useId();
  const root = useRef<HTMLElement>(null);
  const open = useOpen(onOpenMember);
  const summary = useMemo(() => panelSummary(bundle, members.map(member => member.model)), [bundle, members]);

  useEffect(() => {
    if (!focusPending) return;
    focusPending = false;
    root.current?.querySelector<HTMLElement>('[data-bundle-current]')?.focus();
  }, []);

  if (bundle.kind === 'variants') {
    return (
      <section ref={root} aria-labelledby={headingId} className="rounded-[10px] border border-[var(--line)] bg-[var(--panel)] px-5 py-3.5">
        <h2 id={headingId} className={HEADING}>{t('bundleSwitchHeading')}</h2>
        <div role="group" aria-labelledby={headingId} className="inline-flex flex-wrap gap-0.5 mt-2 p-0.5 rounded-[3px] border border-[var(--line)] bg-[var(--panel-2)]">
          {members.map(member => {
            const current = member.model.id === currentId;
            return (
              <button key={member.model.id} type="button" aria-pressed={current} {...(current ? { 'data-bundle-current': '' } : {})}
                onClick={() => { if (!current) open(member.model.id); }}
                className={`h-7 px-3 rounded-[2px] border-0 text-small font-medium cursor-pointer focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)] ${current
                  ? 'bg-[var(--accent)] text-[var(--accent-ink)]' : 'bg-transparent text-[var(--ink-2)] hover:text-[var(--ink)]'}`}>
                {displayName(memberTitle(member))}
              </button>
            );
          })}
        </div>
        <p className="mt-2 mb-0 text-small text-[var(--ink-3)]">{t('bundleSwitchHint')}</p>
      </section>
    );
  }

  const cell = 'py-2 pr-3 border-t border-[var(--line)] tabular-nums align-top';
  const time = (model: ModelFile) => model.printFacts?.printSeconds !== undefined ? formatPrintDuration(model.printFacts.printSeconds, language, t) : t('noValue');
  const weight = (model: ModelFile) => model.estimatedWeightG === null ? t('noValue')
    : `${model.weightSource === 'slicer' ? '' : '≈ '}${formatWeightG(model.estimatedWeightG, language)}`;
  const atLeast = (value: string, complete: boolean) => complete ? value : t('bundleAtLeast').replace('{value}', value);
  return (
    <section ref={root} aria-labelledby={headingId} className="rounded-[10px] border border-[var(--line)] bg-[var(--panel)] px-5 py-3.5">
      <h2 id={headingId} className={HEADING}>{t('bundlePartsHeading')}</h2>
      <div className="overflow-x-auto mt-1.5">
        <table className="w-full border-collapse text-body">
          <caption className="sr-only">{t('bundlePartsCaption')}</caption>
          <thead>
            <tr className="text-left text-caption font-semibold uppercase tracking-[0.07em] text-[var(--ink-3)]">
              <th scope="col" className="py-2 pr-3">{t('bundlePartsPart')}</th>
              <th scope="col" className="py-2 pr-3">{t('metaMaterial')}</th>
              <th scope="col" className="py-2 pr-3">{t('printDataTime')}</th>
              <th scope="col" className="py-2 pr-3">{t('bundlePartsWeight')}</th>
              <th scope="col" className="py-2">{t('bundlePartsStatus')}</th>
            </tr>
          </thead>
          <tbody>
            {members.map((member, index) => {
              const current = member.model.id === currentId;
              return (
                <tr key={member.model.id} className={current ? 'bg-[var(--accent-soft)]' : undefined}>
                  <td className={cell}>
                    <button type="button" {...(current ? { 'aria-current': 'true' as const, 'data-bundle-current': '' } : {})}
                      onClick={() => { if (!current) open(member.model.id); }}
                      className="border-0 bg-transparent p-0 text-left text-[var(--accent)] font-semibold cursor-pointer hover:underline focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]">
                      {index + 1}. {displayName(memberTitle(member))}
                    </button>
                  </td>
                  <td className={cell}>{member.model.materials[0]?.name ?? t('noValue')}</td>
                  <td className={cell}>{time(member.model)}</td>
                  <td className={cell}>{weight(member.model)}</td>
                  <td className={`${cell} pr-0`}>{member.model.printStatus === 'printed' ? t('printStatusPrinted') : t('printStatusNotPrinted')}</td>
                </tr>
              );
            })}
          </tbody>
          <tfoot>
            <tr className="font-semibold">
              <th scope="row" className={`${cell} text-left`}>{t('bundlePartsTotal')}</th>
              <td className={cell}>{summary.materials.length > 0 ? summary.materials.join(', ') : t('noValue')}</td>
              <td className={cell}>{summary.printSeconds === null ? t('noValue') : atLeast(formatPrintDuration(summary.printSeconds, language, t), summary.printSecondsComplete)}</td>
              <td className={cell}>{summary.weightG === null ? t('noValue') : atLeast(formatWeightG(summary.weightG, language), summary.weightComplete)}</td>
              <td className={`${cell} pr-0`}>{t('bundlePrintedOf').replace('{printed}', String(summary.printedCount)).replace('{total}', String(summary.count))}</td>
            </tr>
          </tfoot>
        </table>
      </div>
    </section>
  );
}

interface SectionProps extends Props {
  actions: BundleViewActions;
}

/** "Part of ..." on the detail page of a member, with a row for each member to jump to. */
export function BundleDetailSection({ bundle, members, currentId, onOpenMember, actions }: SectionProps) {
  const t = useT();
  const canEdit = useCanEditMetadata();
  const headingId = useId();
  const open = useOpen(onOpenMember);
  return (
    <section aria-labelledby={headingId} className="rounded-[10px] border border-[var(--line)] bg-[var(--panel)] px-5 py-4">
      <h2 id={headingId} className={HEADING}>{t('bundleSectionHeading')}</h2>
      <p className="mt-1.5 mb-1.5 text-body">
        {t('bundleSectionPartOf').replace('{name}', displayName(bundle.name)).replace('{kind}', t(bundle.kind === 'variants' ? 'bundleKindVariants' : 'bundleKindParts'))}
      </p>
      <ul className="list-none m-0 p-0">
        {members.map((member, index) => {
          const current = member.model.id === currentId;
          return (
            <li key={member.model.id}>
              <button type="button" aria-current={current ? 'true' : undefined} disabled={current}
                aria-label={current ? undefined : t('bundleSectionOpen').replace('{name}', displayName(memberTitle(member)))}
                onClick={() => open(member.model.id)}
                className={`w-full flex items-center gap-2 py-1.5 border-0 border-b border-[var(--line)] bg-transparent text-left text-body ${current ? 'font-bold cursor-default' : 'cursor-pointer hover:bg-[var(--panel-2)]'} focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]`}>
                <span className="w-[22px] flex-none text-[var(--ink-3)]">{index + 1}.</span>
                <span className="flex-1 min-w-0 truncate">{displayName(memberTitle(member))}{current && <span className="font-normal text-[var(--ink-3)]"> {t('bundleSectionThisFile')}</span>}</span>
                {!current && <Icon name="next" size={14} />}
              </button>
            </li>
          );
        })}
      </ul>
      {canEdit && (
        <div className="flex flex-wrap gap-1.5 mt-2.5">
          <button type="button" className={BUTTON} onClick={event => actions.onEdit(bundle.id, event.currentTarget)}>{t('bundleEdit')}</button>
          <button type="button" className={BUTTON} onClick={() => actions.onRemoveMember(currentId)}>{t('bundleRemoveFrom')}</button>
        </div>
      )}
      <p className="mt-2.5 mb-0 text-small text-[var(--ink-3)]">{t('bundleSectionNotes')}</p>
    </section>
  );
}
