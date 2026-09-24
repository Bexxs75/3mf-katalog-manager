import { Fragment } from 'react';
import type { FilamentCheck, FilamentCheckStatus, FilamentNeedCheck, FilamentSpoolUse } from '../types';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatWeightG } from '../i18n/format';
import { STATUS_SYMBOL, roundG, slotText, spoolFillRatio, statusLabel } from '../lib/filamentCheck';

type NeedStatus = Exclude<FilamentCheckStatus, 'no_data'>;

// Farbtoken je Status: ok/swap/short nutzen die vorhandenen Statusfarben,
// "unklar" die eigenen --unk-Tokens.
const TONE: Record<NeedStatus, string> = {
  ok: 'text-[var(--good)] bg-[var(--good-soft)] border-[color-mix(in_oklch,var(--good)_35%,transparent)]',
  swap: 'text-[var(--warn)] bg-[var(--warn-soft)] border-[color-mix(in_oklch,var(--warn)_35%,transparent)]',
  short: 'text-[var(--crit)] bg-[var(--crit-soft)] border-[color-mix(in_oklch,var(--crit)_35%,transparent)]',
  unknown: 'text-[var(--unk)] bg-[var(--unk-soft)] border-[color-mix(in_oklch,var(--unk)_35%,transparent)]',
};

function StatusChip({ status }: { status: NeedStatus }) {
  const t = useT();
  return (
    <span
      className={`inline-flex items-center gap-1.5 h-6 px-2.5 rounded-full border font-mono-ui text-[11.5px] whitespace-nowrap ${TONE[status]}`}
    >
      <span aria-hidden="true">{STATUS_SYMBOL[status]}</span> {statusLabel(status, t)}
    </span>
  );
}

function PlaceChip({ spool }: { spool: FilamentSpoolUse }) {
  const t = useT();
  if (spool.slot) {
    return (
      <span className="inline-flex items-center gap-1 font-mono-ui text-[11px] px-1.5 rounded-[6px] border text-[var(--good)] border-[color-mix(in_oklch,var(--good)_35%,transparent)] bg-[var(--panel-2)]">
        <span aria-hidden="true">●</span>
        <span>{slotText(spool.slot, t)}</span>
      </span>
    );
  }
  if (!spool.location) return null;
  return (
    <span className="inline-flex items-center font-mono-ui text-[11px] px-1.5 rounded-[6px] border border-[var(--line)] text-[var(--ink-2)] bg-[var(--panel-2)]">
      {spool.location}
    </span>
  );
}

// Duenner Fuellbalken unter der Spulen-Zeile fuer "reicht" (gruen) und
// "reicht nicht" (rot); kein Balken ohne bekanntes Originalgewicht.
function FillBar({ spool, tone }: { spool: FilamentSpoolUse; tone: 'ok' | 'short' }) {
  const t = useT();
  const ratio = spoolFillRatio(spool);
  if (ratio === null) return null;
  const percent = Math.round(ratio * 100);
  return (
    <div
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={percent}
      aria-label={t('filamentCheckSpoolFill').replace('{percent}', () => String(percent))}
      className="h-[5px] w-full max-w-[260px] rounded-full bg-[var(--panel-2)] overflow-hidden"
    >
      <div
        className="h-full rounded-full"
        style={{ width: `${percent}%`, backgroundColor: tone === 'ok' ? 'var(--good)' : 'var(--crit)' }}
      />
    </div>
  );
}

function SpoolLine({ spool, showRemaining }: { spool: FilamentSpoolUse; showRemaining: boolean }) {
  const t = useT();
  const { language } = useLanguage();
  return (
    <span className="inline-flex flex-wrap items-center gap-1.5">
      <b className="font-semibold text-[var(--ink)]">{spool.label}</b>
      {showRemaining && (
        <span className="font-mono-ui tabular-nums">
          {t('filamentCheckRemaining').replace('{g}', () => formatWeightG(roundG(spool.remainingG), language))}
        </span>
      )}
      <PlaceChip spool={spool} />
    </span>
  );
}

function NeedRow({ need }: { need: FilamentNeedCheck }) {
  const t = useT();
  const { language } = useLanguage();
  const colorName = need.spools[0]?.colorName;
  const title = colorName ? `${need.filamentType} ${colorName}` : need.filamentType;
  const total = need.spools.reduce((sum, s) => sum + s.remainingG, 0);
  return (
    <div className="grid grid-cols-[auto_minmax(0,1fr)_auto] gap-x-3 gap-y-1 items-center py-2.5 border-t border-[var(--line)] first:border-t-0">
      <span
        className="w-3 h-3 rounded-full border border-[var(--line)] flex-none"
        style={need.color ? { backgroundColor: need.color } : undefined}
      />
      <span className="flex flex-wrap items-baseline gap-2 text-[13.5px]">
        <span>{title}</span>
        <span className="font-mono-ui tabular-nums text-[var(--ink-3)]">{formatWeightG(roundG(need.neededG), language)}</span>
      </span>
      <StatusChip status={need.status} />
      <div className="col-start-2 col-span-2 text-[12.5px] text-[var(--ink-2)] flex flex-col gap-1">
        {need.status === 'ok' && need.spools[0] && (
          <>
            <SpoolLine spool={need.spools[0]} showRemaining />
            <FillBar spool={need.spools[0]} tone="ok" />
          </>
        )}
        {need.status === 'swap' && (
          <>
            <span>
              {t('filamentCheckSwapSpools')
                .replace('{count}', () => String(need.spools.length))
                .replace('{g}', () => formatWeightG(roundG(total), language))}
            </span>
            {need.spools.map((s) => <SpoolLine key={s.spoolId} spool={s} showRemaining />)}
          </>
        )}
        {need.status === 'short' && (
          <>
            {need.spools.map((s) => (
              <Fragment key={s.spoolId}>
                <SpoolLine spool={s} showRemaining />
                <FillBar spool={s} tone="short" />
              </Fragment>
            ))}
            <span className="text-[var(--crit)]">
              {t('filamentCheckMissing').replace('{g}', () => formatWeightG(roundG(need.missingG), language))}
            </span>
          </>
        )}
        {need.status === 'unknown' &&
          (need.possible.length === 0 ? (
            <span>{t('filamentCheckNoMatch')}</span>
          ) : (
            <>
              <span>{t('filamentCheckPossible')}</span>
              {need.possible.map((s) => <SpoolLine key={s.spoolId} spool={s} showRemaining />)}
            </>
          ))}
      </div>
    </div>
  );
}

export function FilamentCheckSection({ check, error }: { check: FilamentCheck | null; error: boolean }) {
  const t = useT();
  if (error) {
    return (
      <div className="mt-3 pt-3 border-t border-[var(--line)] text-[12.5px] text-[var(--ink-3)]">
        {t('filamentCheckError')}
      </div>
    );
  }
  if (!check || check.status === 'no_data') return null;
  return (
    <div className="mt-3 pt-3 border-t border-[var(--line)]">
      <div className="flex items-center justify-between gap-3 flex-wrap mb-1">
        <h3 className="m-0 text-[14px] font-semibold">{t('filamentCheckHeading')}</h3>
        <StatusChip status={check.status} />
      </div>
      <p className="m-0 mb-1 text-[12px] text-[var(--ink-3)]">{t('filamentCheckIntro')}</p>
      {check.needs.map((need, i) => (
        <NeedRow key={`${need.filamentType}-${need.color ?? ''}-${i}`} need={need} />
      ))}
    </div>
  );
}
