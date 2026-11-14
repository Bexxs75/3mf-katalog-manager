import { useId } from 'react';
import { useT } from '../i18n/LanguageContext';
import type { Translations } from '../i18n/types';
import { MAX_FAILURE_NOTE_CHARS, PRINT_FAILURE_REASONS, type PrintFailureReason } from '../lib/printFailureReasons';

/** The wording of each reason. A new code in the list does not compile until it has one. */
export const FAILURE_REASON_KEYS: Record<PrintFailureReason, keyof Translations> = {
  adhesion: 'pfReasonAdhesion',
  clog: 'pfReasonClog',
  filament: 'pfReasonFilament',
  power_network: 'pfReasonPowerNetwork',
  slicer: 'pfReasonSlicer',
  mechanical: 'pfReasonMechanical',
  spaghetti: 'pfReasonSpaghetti',
  intentional: 'pfReasonIntentional',
  other: 'pfReasonOther',
};

/** Text of a reason code; a code this version does not know (newer data) is shown as it is. */
export function useFailureReasonText() {
  const t = useT();
  return (code: string): string => {
    const key = (FAILURE_REASON_KEYS as Record<string, keyof Translations | undefined>)[code];
    return key ? String(t(key)) : code;
  };
}

interface Props {
  reason: string | null;
  note: string;
  onChange: (reason: string | null, note: string) => void;
  /** Shown under the list: why nothing is preselected. */
  hint?: string;
  disabled?: boolean;
  className?: string;
}

/**
 * Optional reason why a print ended early: never required, never preselected. The short
 * note appears only for "Other".
 */
export function PrintFailureReasonField({ reason, note, onChange, hint, disabled, className = '' }: Props) {
  const t = useT();
  const reasonText = useFailureReasonText();
  const id = useId();
  return <div className={`flex flex-col gap-1.5 ${className}`}>
    <label htmlFor={`${id}-reason`} className="text-small text-[var(--ink-2)]">{t('pfFieldLabel')}</label>
    <select id={`${id}-reason`} disabled={disabled} value={reason ?? ''} aria-describedby={hint ? `${id}-hint` : undefined}
      onChange={e => onChange(e.target.value || null, e.target.value === 'other' ? note : '')}
      className="w-full min-w-0 px-2 py-1.5 rounded border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-small">
      <option value="">{t('pfNone')}</option>
      {PRINT_FAILURE_REASONS.map(code => <option key={code} value={code}>{reasonText(code)}</option>)}
    </select>
    {hint && <span id={`${id}-hint`} className="text-caption text-[var(--ink-3)]">{hint}</span>}
    {reason === 'other' && <>
      <label htmlFor={`${id}-note`} className="text-small text-[var(--ink-2)]">{t('pfNoteLabel').replace('{max}', String(MAX_FAILURE_NOTE_CHARS))}</label>
      <input id={`${id}-note`} disabled={disabled} maxLength={MAX_FAILURE_NOTE_CHARS} value={note} onChange={e => onChange(reason, e.target.value)}
        className="w-full min-w-0 px-2 py-1.5 rounded border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-small" />
      <span aria-hidden className="text-caption text-[var(--ink-3)] tabular-nums self-end">{t('pfNoteCount').replace('{count}', String(Array.from(note).length)).replace('{max}', String(MAX_FAILURE_NOTE_CHARS))}</span>
    </>}
  </div>;
}
