const INTERACTIVE = 'button, input, textarea, select, a, [role="dialog"]';

/**
 * Doppelklick auf Karte/Zeile oeffnet das Bearbeiten, aber nicht, wenn er auf
 * einem Knopf, Feld oder Popover landet. Ereignisse aus Portalen bubbeln im
 * React-Baum weiter, liegen aber nicht im DOM der Karte: auch die ignorieren.
 */
export function isFromInteractiveElement(event: { target: EventTarget | null; currentTarget: EventTarget | null }): boolean {
  const { target, currentTarget } = event;
  if (!(target instanceof Element)) return false;
  if (currentTarget instanceof Node && !currentTarget.contains(target)) return true;
  return target.closest(INTERACTIVE) !== null;
}
