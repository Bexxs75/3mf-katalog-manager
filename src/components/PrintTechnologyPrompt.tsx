import { usePrintTechnology } from '../hooks/PrintTechnologyContext';
import { PrintTechnologyDialog } from './PrintTechnologyDialog';
import { PrintTechnologyNotice } from './PrintTechnologyNotice';

/**
 * Asks "What do you print with?" once. A new installation gets the dialog; an
 * installation that existed before only gets a notice, so the update never
 * interrupts work. `wait` holds it back while another start-up question is open.
 */
export function PrintTechnologyPrompt({ firstRun, wait, onChoose }: { firstRun: boolean; wait: boolean; onChoose: () => void }) {
  const { asked } = usePrintTechnology();
  if (asked || wait) return null;
  return firstRun ? <PrintTechnologyDialog onDone={() => {}} /> : <PrintTechnologyNotice onChoose={onChoose} />;
}
