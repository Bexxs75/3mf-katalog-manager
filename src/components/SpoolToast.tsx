import { useEffect, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { AutocompleteInput } from './AutocompleteInput';

interface Props {
  label: string;
  /** New location after unloading (home location); `null` = no location. */
  location: string | null;
  knownLocations: string[];
  /** If missing, the toast shows only `label` (e.g. "2 spools PETG · Red created"). */
  onChangeLocation?: (location: string) => Promise<unknown>;
  onDone: () => void;
}

const AUTO_CLOSE_MS = 5000;

/**
 * Toast after unloading a spool: "PLA Black → back to Shelf 2 · Change".
 * While dragging, a question would interrupt the flow, so it silently goes
 * to the home location but can be corrected here. Closes after 5 s,
 * except during the correction.
 */
export function SpoolToast({ label, location, knownLocations, onChangeLocation, onDone }: Props) {
  const t = useT();
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(location ?? '');

  useEffect(() => {
    if (editing) return;
    const timer = setTimeout(onDone, AUTO_CLOSE_MS);
    return () => clearTimeout(timer);
  }, [editing, onDone, label, location]);

  const save = () => {
    const next = draft.trim();
    if (!next || !onChangeLocation) return;
    onChangeLocation(next).then(onDone, () => {});
  };

  return (
    <div
      role="status"
      className="fixed bottom-4 left-1/2 -translate-x-1/2 z-50 max-w-[min(92vw,460px)] px-4 py-2.5 rounded-[6px] border border-[var(--line-strong)] bg-[var(--ink)] text-[var(--bg)] text-[12.5px] shadow-[var(--shadow)] flex items-center gap-2"
    >
      <span className="font-semibold truncate">{label}</span>
      {onChangeLocation &&
        (editing ? (
          <>
            <span className="w-44 text-[var(--ink)]">
              <AutocompleteInput
                value={draft}
                onChange={setDraft}
                options={knownLocations}
                placeholder={t('printersLocationPlaceholder')}
                className="w-full h-7 px-2 rounded-[4px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[12px] outline-0"
              />
            </span>
            <button type="button" onClick={save} className="font-bold underline cursor-pointer">
              OK
            </button>
          </>
        ) : (
          <>
            <span className="opacity-80 truncate">
              {location ? `${t('printersReturnedTo')} ${location}` : t('printersReturnedToStorage')}
            </span>
            <button
              type="button"
              onClick={() => setEditing(true)}
              className="font-bold underline cursor-pointer flex-none"
            >
              {t('printersChangeLocation')}
            </button>
          </>
        ))}
    </div>
  );
}
