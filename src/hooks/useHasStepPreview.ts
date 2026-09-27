import { useEffect, useState } from 'react';
import { hasStepPreview } from '../lib/api/stepPreview';

/**
 * Null while the initial check is still in flight. Callers that decide
 * whether to skip a STEP file entirely (e.g. the background snapshot queue)
 * should treat null as "not confirmed yet" and hold off rather than guess.
 */
export function useHasStepPreview(): boolean | null {
  const [value, setValue] = useState<boolean | null>(null);

  useEffect(() => {
    let cancelled = false;
    hasStepPreview()
      // Fail open on an IPC hiccup: assume STEP support is there so a broken
      // check falls back to today's behavior (try the real viewer) instead
      // of permanently hiding it behind the hint.
      .catch(() => true)
      .then((result) => {
        if (!cancelled) setValue(result);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return value;
}
