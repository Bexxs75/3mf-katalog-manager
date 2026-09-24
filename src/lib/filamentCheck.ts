import type { Language, Translations } from '../i18n/types';
import { formatWeightG } from '../i18n/format';
import type { FilamentCheck, FilamentCheckStatus, FilamentSlotRef } from '../types';

type T = <K extends keyof Translations>(key: K) => Translations[K];

export const STATUS_SYMBOL: Record<FilamentCheckStatus, string> = {
  ok: '✓',
  swap: '⇄',
  short: '✗',
  unknown: '?',
  no_data: '–',
};

export function roundG(grams: number): number {
  return Math.round(grams * 10) / 10;
}

export function statusLabel(status: Exclude<FilamentCheckStatus, 'no_data'>, t: T): string {
  switch (status) {
    case 'ok':
      return t('filamentCheckStatusOk');
    case 'swap':
      return t('filamentCheckStatusSwap');
    case 'short':
      return t('filamentCheckStatusShort');
    case 'unknown':
      return t('filamentCheckStatusUnknown');
  }
}

export function slotText(slot: FilamentSlotRef, t: T): string {
  return t('filamentCheckSlot')
    .replace('{printer}', slot.printer)
    .replace('{unit}', slot.unit)
    .replace('{slot}', String(slot.slotNumber));
}

export function queueTooltip(check: FilamentCheck, t: T, language: Language): string {
  if (check.status === 'no_data') return t('queueFilamentNoData');
  const lines = check.needs
    .filter((n) => n.status !== 'ok')
    .map((n) => {
      if (n.status === 'short') {
        return t('queueFilamentShortLine')
          .replace('{type}', n.filamentType)
          .replace('{g}', formatWeightG(roundG(n.missingG), language));
      }
      if (n.status === 'swap') return t('queueFilamentSwapLine').replace('{type}', n.filamentType);
      return t('queueFilamentUnknownLine').replace('{type}', n.filamentType);
    });
  return lines.length === 0 ? t('queueFilamentOk') : lines.join('\n');
}
