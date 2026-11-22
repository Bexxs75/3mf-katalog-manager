import { uiSettings } from '../lib/uiSettings';
import { useEffect, useRef, useState } from 'react';
import { currentVersion, releaseNotes } from '../lib/releaseNotes';
import { LAST_SEEN_VERSION_KEY, releaseNoteKey, seenVersionOnClose, selectReleaseVersions } from '../lib/releaseNotesSelection';

function readMarker(): string | null {
  try { return uiSettings.getItem(LAST_SEEN_VERSION_KEY); }
  catch { return null; }
}
function remember() {
  try {
    const stored = readMarker();
    const next = seenVersionOnClose(currentVersion, stored);
    if (next !== stored) uiSettings.setItem(LAST_SEEN_VERSION_KEY, next);
  } catch (error) { console.warn('[whats-new] remembering version failed:', error); }
}
export function useWhatsNew({ wait, firstRun, web, reopenBlocked = false }: { wait: boolean; firstRun: boolean; web: boolean; reopenBlocked?: boolean }) {
  const [versions, setVersions] = useState<string[]>([]);
  const [settled, setSettled] = useState(false);
  const decided = useRef(false);
  useEffect(() => {
    if (wait || decided.current) return;
    decided.current = true;
    const marker = readMarker();
    const fresh = web ? marker === null : firstRun;
    const selected = selectReleaseVersions(Object.keys(releaseNotes), currentVersion, marker, fresh);
    if (fresh) remember();
    setVersions(selected);
    if (!selected.length) setSettled(true);
  }, [wait, firstRun, web]);
  return {
    versions, settled,
    reopen: () => {
      if (!decided.current || wait || reopenBlocked) return;
      const key = releaseNoteKey(currentVersion);
      setVersions(releaseNotes[key] ? [key] : []);
    },
    close: () => { remember(); setVersions([]); setSettled(true); },
  };
}
