import { useCallback, useEffect, useRef, useState } from 'react';
import type { ModelFile } from '../types';
import { isReadOnly } from '../lib/readOnly';
import { useRuntimeEnvironment } from './useRuntimeEnvironment';
import { beginDragOut, loadNativeDragEnabled, MAX_NATIVE_DRAG_FILES, nativeDragIdsFor, NativeDragState, useNativeDragEnabled } from '../lib/nativeDrag';
import { info } from '../lib/logger';
import { listen } from '../lib/events';
import { reportMetadataError } from '../lib/metadataErrors';

export type DragOutHint = 'dgOutsideOffHint' | 'dgLimit' | 'dgStepHint';
let outsideOffHintShown = false;

export function useDragOutTransition({ active, ids, models, onHandOver }: {
  active: boolean; ids: readonly string[]; models: readonly ModelFile[]; onHandOver: () => void;
}) {
  const environment = useRuntimeEnvironment();
  const supported = !isReadOnly() && !environment.web && !environment.container;
  const enabled = useNativeDragEnabled();
  const [hint, setHint] = useState<DragOutHint | null>(null);
  const dismissHint = useCallback(() => setHint(null), []);
  const latest = useRef({ active, ids, models, onHandOver, supported, enabled });
  latest.current = { active, ids, models, onHandOver, supported, enabled };
  const probe = useRef<{ start: number; attempted: boolean; outside: number; left: boolean; mouseup: boolean } | null>(null);
  useEffect(() => { if (supported) void loadNativeDragEnabled(); }, [supported]);
  useEffect(() => {
    if (active) probe.current = { start: performance.now(), attempted: false, outside: 0, left: false, mouseup: false };
  }, [active]);
  useEffect(() => {
    const log = (message: string) => { void info(message).catch(() => {}); };
    const summary = (reason: string) => {
      const p = probe.current;
      if (p?.left) log(`dragout-probe end=${reason} mousemove_outside=${p.outside} mouseup=${p.mouseup}`);
    };
    const trigger = (event: MouseEvent, source: 'mousemove' | 'mouseleave') => {
      const current = latest.current;
      const p = probe.current;
      if (!current.supported || !p) return;
      const outside = event.clientX < 0 || event.clientY < 0 || event.clientX >= window.innerWidth || event.clientY >= window.innerHeight;
      if (source === 'mousemove' && !outside) return;
      if (source === 'mousemove') p.outside++;
      if (p.left) summary('observation');
      if (!current.active || p.attempted || !(event.buttons & 1)) return;
      p.left = true;
      log(`dragout-probe trigger=${source} x=${event.clientX} y=${event.clientY} innerW=${window.innerWidth} innerH=${window.innerHeight} buttons=${event.buttons} t_ms_since_start=${Math.round(performance.now() - p.start)} mousemove_outside=${p.outside} mouseup=${p.mouseup}`);
      p.attempted = true;
      if (!current.enabled) {
        if (!outsideOffHintShown) { outsideOffHintShown = true; setHint('dgOutsideOffHint'); }
        return;
      }
      if (NativeDragState.isActive()) return;
      const selection = current.models.filter(model => current.ids.includes(model.id));
      const model = selection[0];
      if (!model) return;
      const filtered = nativeDragIdsFor(model, selection);
      if (filtered.length > MAX_NATIVE_DRAG_FILES) { setHint('dgLimit'); return; }
      if (!filtered.length) {
        if (selection.some(file => !file.deletedAt && /\.(step|stp)$/i.test(file.path))) setHint('dgStepHint');
        return;
      }
      current.onHandOver();
      void beginDragOut(filtered).catch(reportMetadataError);
    };
    const move = (e: MouseEvent) => trigger(e, 'mousemove');
    const leave = (e: MouseEvent) => trigger(e, 'mouseleave');
    const up = () => { if (probe.current) probe.current.mouseup = true; summary('mouseup'); };
    const down = () => { summary('next-mousedown'); probe.current = null; };
    window.addEventListener('mousemove', move);
    document.documentElement.addEventListener('mouseleave', leave);
    window.addEventListener('mouseup', up);
    window.addEventListener('mousedown', down, true);
    let disposed = false;
    let off: (() => void) | undefined;
    void listen('native-drag-ended', () => summary('native-drag-ended')).then(fn => {
      if (disposed) fn(); else off = fn;
    }).catch(() => {});
    return () => {
      disposed = true;
      off?.();
      summary('unmount');
      window.removeEventListener('mousemove', move);
      document.documentElement.removeEventListener('mouseleave', leave);
      window.removeEventListener('mouseup', up);
      window.removeEventListener('mousedown', down, true);
    };
  }, []);
  return { hint, dismissHint };
}
