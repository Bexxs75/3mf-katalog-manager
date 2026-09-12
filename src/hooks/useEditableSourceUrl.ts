import { useEffect, useRef, useState, type KeyboardEvent } from 'react';

interface EditableSourceUrlModel {
  id: string;
  sourceUrl: string | null;
}

/**
 * Teilt sich das Bearbeiten-Verhalten der "Quell-URL"-Zeile zwischen
 * DetailPanel (kompakt/komfortabel) und ModelDetailPage: Eingabefeld
 * anzeigen, per Enter/Blur speichern, per Escape verwerfen ohne beim
 * Blur trotzdem zu speichern (cancelingRef verhindert das).
 */
export function useEditableSourceUrl(
  model: EditableSourceUrlModel | null,
  onSave: (fileId: string, url: string | null) => void,
) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState('');
  const cancelingRef = useRef(false);

  useEffect(() => {
    setEditing(false);
    cancelingRef.current = false;
  }, [model?.id]);

  const startEditing = () => {
    if (!model) return;
    cancelingRef.current = false;
    setDraft(model.sourceUrl ?? '');
    setEditing(true);
  };

  const submit = () => {
    if (!model) return;
    const value = draft.trim();
    onSave(model.id, value || null);
    setEditing(false);
  };

  const cancel = () => {
    cancelingRef.current = true;
    setEditing(false);
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') submit();
    if (e.key === 'Escape') cancel();
  };

  const handleBlur = () => {
    if (cancelingRef.current) {
      cancelingRef.current = false;
      return;
    }
    submit();
  };

  return { editing, draft, setDraft, startEditing, handleKeyDown, handleBlur };
}
