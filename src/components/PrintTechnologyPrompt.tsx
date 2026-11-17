import { useEffect, useRef, useState } from 'react';
import { usePrintTechnology } from '../hooks/PrintTechnologyContext';
import { PrintTechnologyDialog } from './PrintTechnologyDialog';
import { PrintTechnologyNotice } from './PrintTechnologyNotice';

/**
 * Asks "What do you print with?" once. A new installation gets the dialog; an
 * installation that existed before only gets a notice, so the update never
 * interrupts work. `wait` holds it back while another start-up question is open.
 */
export function PrintTechnologyPrompt({ firstRun, wait, onChoose, onSettled }: { firstRun: boolean; wait: boolean; onChoose: () => void; onSettled?: () => void }) {
  const { asked, loading } = usePrintTechnology();
  const [done, setDone] = useState(false);
  const settledRef = useRef(onSettled);
  settledRef.current = onSettled;
  useEffect(() => {
    if (!wait && !loading && (asked || done)) settledRef.current?.();
  }, [wait, loading, asked, done]);
  const finish = () => setDone(true);
  if (asked || wait || loading || done) return null;
  return firstRun ? <PrintTechnologyDialog onDone={finish} />
    : <PrintTechnologyNotice onChoose={onChoose} onDone={finish} />;
}
