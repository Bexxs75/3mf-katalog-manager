import { useEffect, useRef, useState } from 'react';
import { canSaveSnapshots, useCanEditMetadata } from '../lib/webSession';
import { BackgroundSnapshotRenderer } from './BackgroundSnapshotRenderer';

interface Props {
  ids: string[];
  noWebGL: boolean;
  capture: (id: string, base64: string) => Promise<void> | void;
  skip: (id: string) => void;
}

/** One browser renderer and save at a time, with a breathing pause between files. */
export function WebBackgroundSnapshots({ ids, noWebGL, capture, skip }: Props) {
  const writable = useCanEditMetadata();
  const [visible, setVisible] = useState(() => document.visibilityState === 'visible');
  const [busy, setBusy] = useState(false);
  const active = useRef(false);
  const mounted = useRef(false);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => {
    mounted.current = true;
    const changed = () => setVisible(document.visibilityState === 'visible');
    document.addEventListener('visibilitychange', changed);
    return () => {
      mounted.current = false;
      clearTimeout(timer.current);
      document.removeEventListener('visibilitychange', changed);
    };
  }, []);
  const finish = () => {
    if (!mounted.current) return;
    timer.current = setTimeout(() => {
      active.current = false;
      setBusy(false);
    }, 250);
  };
  const failed = (id: string) => {
    if (active.current) return;
    active.current = true;
    setBusy(true);
    skip(id);
    finish();
  };
  if (!writable || noWebGL || !visible || busy || !ids.length) return null;
  return <BackgroundSnapshotRenderer key={ids[0]} fileId={ids[0]}
    onSnapshotCaptured={(id, base64) => {
      if (active.current || !canSaveSnapshots() || document.visibilityState !== 'visible') return;
      active.current = true;
      setBusy(true);
      void Promise.resolve().then(() => capture(id, base64)).catch(() => skip(id)).finally(finish);
    }}
    onError={failed} onContextLost={failed} />;
}
