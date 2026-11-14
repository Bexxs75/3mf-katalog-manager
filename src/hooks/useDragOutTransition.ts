import { useCallback, useEffect, useRef, useState } from 'react';
import type { ModelFile } from '../types';
import { isReadOnly } from '../lib/readOnly';
import { useRuntimeEnvironment } from './useRuntimeEnvironment';
import { beginDragOut, loadNativeDragEnabled, MAX_NATIVE_DRAG_FILES, nativeDragIdsFor, NativeDragState, useNativeDragEnabled } from '../lib/nativeDrag';
import { info } from '../lib/logger';
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
  const gesture = useRef<{ attempted: boolean } | null>(null);
  useEffect(() => { if (supported) void loadNativeDragEnabled(); }, [supported]);
  useEffect(() => {
    if (active) gesture.current = { attempted: false };
  }, [active]);
  useEffect(() => {
    const log = (message: string) => { void info(message).catch(() => {}); };
    const trigger = (event: MouseEvent, source: 'mousemove' | 'mouseleave') => {
      const current = latest.current;
      const p = gesture.current;
      if (!current.supported || !p) return;
      const outside = event.clientX < 0 || event.clientY < 0 || event.clientX >= window.innerWidth || event.clientY >= window.innerHeight;
      if (source === 'mousemove' && !outside) return;
      if (!current.active || p.attempted || !(event.buttons & 1)) return;
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
      log(`dragout handover trigger=${source} files=${filtered.length}`);
      current.onHandOver();
      void beginDragOut(filtered).catch(reportMetadataError);
    };
    const move = (e: MouseEvent) => trigger(e, 'mousemove');
    const leave = (e: MouseEvent) => trigger(e, 'mouseleave');
    const down = () => { gesture.current = null; };
    window.addEventListener('mousemove', move);
    document.documentElement.addEventListener('mouseleave', leave);
    window.addEventListener('mousedown', down, true);
    return () => {
      window.removeEventListener('mousemove', move);
      document.documentElement.removeEventListener('mouseleave', leave);
      window.removeEventListener('mousedown', down, true);
    };
  }, []);
  return { hint, dismissHint };
}
