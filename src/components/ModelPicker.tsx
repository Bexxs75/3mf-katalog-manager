import { useMemo, useState } from 'react';
import { useT } from '../i18n/LanguageContext';

export interface ModelOption {
  id: string;
  name: string;
}

interface Props {
  models: ModelOption[];
  onChange: (fileId: string | null) => void;
  onClose: () => void;
}

/** Suchauswahl über alle Katalogmodelle, inkl. „Kein Modell“. */
export function ModelPicker({ models, onChange, onClose }: Props) {
  const t = useT();
  const [q, setQ] = useState('');
  const hits = useMemo(() => {
    const needle = q.trim().toLowerCase();
    return (needle ? models.filter((m) => m.name.toLowerCase().includes(needle)) : models).slice(0, 50);
  }, [models, q]);
  return (
    <div
      className="absolute z-50 mt-1 w-[280px] rounded-md border border-[var(--line-strong)] bg-[var(--panel)] shadow-[var(--shadow)] p-1.5 flex flex-col gap-1"
      onKeyDown={(e) => e.key === 'Escape' && onClose()}
    >
      <input
        autoFocus
        value={q}
        onChange={(e) => setQ(e.target.value)}
        placeholder={t('printerJobModelSearch')}
        className="h-7 px-2 rounded border border-[var(--line-strong)] bg-[var(--panel-2)] text-[12.5px] outline-0 focus:border-[var(--accent)]"
      />
      <button type="button" onClick={() => onChange(null)} className="text-left px-2 py-1 text-[12.5px] text-[var(--ink-3)] hover:bg-[var(--panel-2)] rounded cursor-pointer">
        {t('printerJobModelNoModel')}
      </button>
      <ul className="max-h-52 overflow-y-auto">
        {hits.map((m) => (
          <li key={m.id}>
            <button type="button" onClick={() => onChange(m.id)} className="w-full text-left px-2 py-1 text-[12.5px] hover:bg-[var(--panel-2)] rounded truncate cursor-pointer">
              {m.name}
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
