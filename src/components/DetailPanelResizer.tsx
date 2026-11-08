import { useRef } from 'react';
import { useT } from '../i18n/LanguageContext';
import { clampDetailPanelWidth } from '../lib/detailPanelWidthPreference';

interface Props {
  width: number;
  workspaceWidth: number;
  setWidth: (px: number, persist?: boolean) => void;
  reset: () => void;
}

export function DetailPanelResizer({ width, workspaceWidth, setWidth, reset }: Props) {
  const t = useT();
  const drag = useRef<{ id: number; x: number; width: number } | null>(null);
  const clamp = (px: number) => clampDetailPanelWidth(px, workspaceWidth);
  const maximum = clamp(720);
  return <div role="separator" aria-orientation="vertical" tabIndex={0}
    aria-label={t('detailPanelResizeLabel')} title={t('detailPanelResizeHint')}
    aria-valuenow={width} aria-valuemin={300} aria-valuemax={maximum}
    className="detail-panel-resizer"
    onDoubleClick={reset}
    onKeyDown={event => {
      const step = event.shiftKey ? 64 : 16;
      let next: number;
      switch (event.key) {
        case 'ArrowLeft': next = width + step; break;
        case 'ArrowRight': next = width - step; break;
        case 'Home': next = 300; break;
        case 'End': next = maximum; break;
        case 'Enter': event.preventDefault(); event.stopPropagation(); reset(); return;
        default: return;
      }
      event.preventDefault();
      event.stopPropagation();
      setWidth(clamp(next));
    }}
    onPointerDown={event => {
      if (event.button !== 0 || drag.current) return;
      event.preventDefault();
      event.currentTarget.focus();
      event.currentTarget.setPointerCapture(event.pointerId);
      drag.current = { id: event.pointerId, x: event.clientX, width };
    }}
    onPointerMove={event => {
      const start = drag.current;
      if (start?.id === event.pointerId) setWidth(clamp(start.width + start.x - event.clientX), false);
    }}
    onPointerUp={event => {
      const start = drag.current;
      if (!start || start.id !== event.pointerId) return;
      drag.current = null;
      setWidth(clamp(start.width + start.x - event.clientX));
      event.currentTarget.releasePointerCapture(event.pointerId);
    }}
    onPointerCancel={() => {
      if (drag.current) setWidth(drag.current.width, false);
      drag.current = null;
    }}
    onLostPointerCapture={() => {
      if (drag.current) setWidth(drag.current.width, false);
      drag.current = null;
    }}
  />;
}
