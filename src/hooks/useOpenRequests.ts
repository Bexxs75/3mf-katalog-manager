import { useCallback, useEffect, useState } from 'react';
import { listen } from '../lib/events';
import { OPEN_FILES_EVENT, takeOpenRequests, type OpenRequest } from '../lib/api/openRequests';

/**
 * Collects files handed over by the operating system ("Open with", a second start with a
 * file argument): once at startup (a cold start is earlier than the window) and on every
 * `open-files-requested` event. The requests wait here until a consumer (the view-only
 * dialog) takes them with `consume`; nothing is opened by this hook itself.
 */
export function useOpenRequests() {
  const [requests, setRequests] = useState<OpenRequest[]>([]);

  useEffect(() => {
    if (import.meta.env.VITE_TARGET === 'web') return;
    // Local flag, not a ref: StrictMode turns the first registration into a dead one.
    let active = true;
    let unlisten: (() => void) | undefined;
    const collect = () => {
      takeOpenRequests()
        .then((taken) => {
          if (active && taken.length > 0) setRequests((current) => [...current, ...taken]);
        })
        .catch((error) => console.warn('[open] collecting handed-over files failed:', error));
    };
    // Listening first, so an event between the first collect and the registration is not lost.
    listen(OPEN_FILES_EVENT, collect).then((fn) => {
      if (!active) fn();
      else unlisten = fn;
    });
    collect();
    return () => {
      active = false;
      unlisten?.();
    };
  }, []);

  /** Hands the waiting requests to the consumer and clears them. */
  const consume = useCallback(() => {
    const waiting = requests;
    setRequests([]);
    return waiting;
  }, [requests]);

  return { requests, consume };
}
