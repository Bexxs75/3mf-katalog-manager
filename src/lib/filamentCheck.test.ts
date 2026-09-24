import { describe, expect, it } from 'vitest';
import { de } from '../i18n/de';
import { en } from '../i18n/en';
import { STATUS_SYMBOL, queueTooltip, roundG, slotText, spoolFillRatio, statusLabel } from './filamentCheck';
import type { FilamentCheck, FilamentNeedCheck, FilamentSpoolUse } from '../types';

const tDe = <K extends keyof typeof de>(k: K) => de[k];
const tEn = <K extends keyof typeof en>(k: K) => en[k];

const need = (over: Partial<FilamentNeedCheck>): FilamentNeedCheck => ({
  filamentType: 'PLA', color: '#C0392B', neededG: 10, status: 'ok', missingG: 0, spools: [], possible: [], ...over,
});

describe('filamentCheck helpers', () => {
  it('maps every status to its symbol', () => {
    expect(STATUS_SYMBOL).toEqual({ ok: '✓', swap: '⇄', short: '✗', unknown: '?', no_data: '–' });
  });

  it('rounds grams to one decimal', () => {
    expect(roundG(208.456)).toBe(208.5);
    expect(roundG(13.44)).toBe(13.4);
  });

  it('translates status labels', () => {
    expect(statusLabel('short', tDe)).toBe('reicht nicht');
    expect(statusLabel('swap', tEn)).toBe('enough with spool change');
  });

  it('formats a slot with a 1-based number', () => {
    expect(slotText({ printer: 'X1C', unit: 'AMS 1', slotNumber: 2 }, tDe)).toBe('X1C · AMS 1 · Fach 2');
    expect(slotText({ printer: 'X1C', unit: 'AMS 1', slotNumber: 2 }, tEn)).toBe('X1C · AMS 1 · Slot 2');
  });

  it('builds the queue tooltip from the non-ok needs', () => {
    const check: FilamentCheck = {
      fileId: '1',
      status: 'short',
      needs: [
        need({ status: 'ok' }),
        need({ filamentType: 'PLA Silk', status: 'short', missingG: 13.46 }),
        need({ filamentType: 'PETG', status: 'swap' }),
        need({ filamentType: 'TPU', status: 'unknown' }),
      ],
    };
    expect(queueTooltip(check, tDe, 'de')).toBe(
      'PLA Silk: es fehlen 13,5 g\nPETG: nur mit Spulenwechsel\nTPU: keine passende Spule',
    );
  });

  it('uses fixed texts for ok and no_data', () => {
    expect(queueTooltip({ fileId: '1', status: 'ok', needs: [need({})] }, tDe, 'de')).toBe('Filament reicht');
    expect(queueTooltip({ fileId: '1', status: 'no_data', needs: [] }, tDe, 'de')).toBe('Keine Verbrauchsdaten');
  });

  it('handles special characters in user-supplied names (slotText)', () => {
    expect(slotText({ printer: 'Drucker $&', unit: 'AMS $$', slotNumber: 1 }, tDe)).toBe('Drucker $& · AMS $$ · Fach 1');
  });

  it('handles special characters in user-supplied filament types (queueTooltip)', () => {
    const check: FilamentCheck = {
      fileId: '1',
      status: 'short',
      needs: [
        need({ filamentType: 'PLA $&', status: 'short', missingG: 13.46 }),
      ],
    };
    expect(queueTooltip(check, tDe, 'de')).toBe('PLA $&: es fehlen 13,5 g');
  });

  const spool = (over: Partial<FilamentSpoolUse> = {}): FilamentSpoolUse => ({
    spoolId: 's1', label: 'Bambu PLA · Rot', colorName: 'Rot', remainingG: 640, originalG: 1000, slot: null, location: null, ...over,
  });

  it('computes the fill ratio of a spool', () => {
    expect(spoolFillRatio(spool({ remainingG: 640, originalG: 1000 }))).toBe(0.64);
  });

  it('clamps the fill ratio to 0..1', () => {
    expect(spoolFillRatio(spool({ remainingG: -5, originalG: 1000 }))).toBe(0);
    expect(spoolFillRatio(spool({ remainingG: 1200, originalG: 1000 }))).toBe(1);
  });

  it('has no fill ratio without a known original weight', () => {
    expect(spoolFillRatio(spool({ originalG: 0 }))).toBeNull();
    expect(spoolFillRatio(spool({ originalG: -1 }))).toBeNull();
  });
});
