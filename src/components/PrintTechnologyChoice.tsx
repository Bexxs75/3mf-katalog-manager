import { useId } from 'react';
import { useT } from '../i18n/LanguageContext';
import { PRINT_TECHNOLOGIES, type PrintTechnology } from '../lib/printTechnology';
import type { Translations } from '../i18n/types';
import { Icon, type IconName } from './Icon';

type TextKey = { [K in keyof Translations]: Translations[K] extends string ? K : never }[keyof Translations];

const LABEL: Record<PrintTechnology, TextKey> = { filament: 'ptFilament', resin: 'ptResin', both: 'ptBoth' };
const DESCRIPTION: Record<PrintTechnology, TextKey> = { filament: 'ptFilamentDesc', resin: 'ptResinDesc', both: 'ptBothDesc' };
const ICON: Record<PrintTechnology, IconName> = { filament: 'spool', resin: 'resin', both: 'catalog' };
const EFFECT: Record<PrintTechnology, TextKey> = { filament: 'ptEffectFilament', resin: 'ptEffectResin', both: 'ptEffectBoth' };

/** One sentence on what the current choice shows and hides. */
export function usePrintTechnologyEffect(value: PrintTechnology) {
  return useT()(EFFECT[value]);
}

/**
 * The three-way choice as native radio buttons: arrow keys, focus and the group
 * name come from the browser. `tiles` is the large form for the question
 * dialog, `list` the compact form for the settings.
 */
export function PrintTechnologyChoice({ value, onChange, variant, disabled = false, describedBy }: {
  value: PrintTechnology;
  onChange: (value: PrintTechnology) => void;
  variant: 'tiles' | 'list';
  disabled?: boolean;
  describedBy?: string;
}) {
  const t = useT();
  const name = useId();
  return (
    <div role="radiogroup" aria-label={t('ptTitle')} aria-describedby={describedBy}
      className={variant === 'tiles' ? 'grid gap-2.5 sm:grid-cols-3' : 'flex flex-col gap-1.5'}>
      {PRINT_TECHNOLOGIES.map((option) => {
        const checked = value === option;
        return (
          <label key={option}
            className={`relative flex gap-2 rounded-lg border p-3 ${disabled ? 'cursor-not-allowed opacity-60' : 'cursor-pointer hover:bg-[var(--panel-2)]'} ${checked ? 'border-[var(--accent)] bg-[var(--sel)]' : 'border-[var(--line)]'} ${variant === 'tiles' ? 'flex-col' : 'items-start'} has-[:focus-visible]:outline has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-[var(--accent)]`}>
            <input type="radio" name={name} value={option} checked={checked} disabled={disabled}
              onChange={() => onChange(option)} className={variant === 'tiles' ? 'sr-only' : 'print-technology-radio mt-1'} data-initial-focus={checked ? true : undefined} />
            {variant === 'tiles' && (
              <span aria-hidden="true" className="flex items-center gap-1.5 text-[var(--ink-2)]">
                <Icon name={ICON[option]} size={20} />
                {checked && <Icon name="check" size={16} />}
              </span>
            )}
            <span className="flex flex-col gap-1 min-w-0">
              <span className="text-body font-semibold">
                {t(LABEL[option])}
                {option === 'both' && <span className="ml-1.5 text-caption font-normal text-[var(--ink-3)]">({t('ptDefault')})</span>}
              </span>
              <span className="text-small text-[var(--ink-2)]">{t(DESCRIPTION[option])}</span>
            </span>
          </label>
        );
      })}
    </div>
  );
}
