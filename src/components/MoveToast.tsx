import { useEffect } from 'react';

interface Props {
  from: string;
  to: string;
  error?: boolean;
  onDone: () => void;
}

/**
 * Kurze Bestaetigungs-Meldung nach einem per Maus-Drag ausgeloesten Verschieben
 * (Datei oder Ordner), bzw. nach dem Anlegen eines Ordners. Analog zur
 * `.toast`-Optik im HTML-Mockup, aber mit den Projekt-Tokens statt fester
 * Farbwerte, damit Hell/Dunkel-Theme automatisch passt. Verschwindet nach 3
 * Sekunden von selbst (`onDone` raeumt den `moveToast`-State in `App.tsx`
 * auf); ein erneuter Move waehrenddessen ersetzt die Props und startet den
 * Timer ueber den Effekt-Cleanup neu.
 *
 * `error`: fuer fehlgeschlagene move_file_to_folder/move_folder/create_folder
 * Aufrufe wiederverwendet (statt die Fehlermeldung nur unsichtbar in das
 * Settings-only `catalogBackupError` zu routen) - `to` traegt in diesem Fall
 * die Fehlermeldung statt eines Zielpfads, kein Pfeil, roter Akzent.
 */
export function MoveToast({ from, to, error = false, onDone }: Props) {
  useEffect(() => {
    const timer = setTimeout(onDone, 3000);
    return () => clearTimeout(timer);
  }, [from, to, error, onDone]);

  return (
    <div
      className={`fixed bottom-4 left-1/2 -translate-x-1/2 z-50 max-w-[min(90vw,420px)] px-4 py-2.5 rounded-[6px] border text-[12.5px] font-medium shadow-[var(--shadow)] flex items-center gap-1.5 ${
        error
          ? 'border-red-400 bg-[var(--panel)] text-red-400'
          : 'border-[var(--line-strong)] bg-[var(--ink)] text-[var(--bg)]'
      }`}
      role="status"
    >
      {error ? (
        <>
          <span className="font-semibold flex-none">✕</span>
          <span className="font-semibold overflow-hidden text-ellipsis whitespace-nowrap">{from}:</span>
          <span className="opacity-90 overflow-hidden text-ellipsis whitespace-nowrap">{to}</span>
        </>
      ) : (
        <>
          <span className="font-semibold overflow-hidden text-ellipsis whitespace-nowrap">{from}</span>
          <span className="opacity-70 flex-none">→</span>
          <span className="opacity-90 overflow-hidden text-ellipsis whitespace-nowrap">{to}</span>
        </>
      )}
    </div>
  );
}
