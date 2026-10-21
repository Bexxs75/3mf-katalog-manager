import { useT } from '../i18n/LanguageContext';
import type { SpoolGroup } from '../lib/filamentGroups';
import type { FilamentStockStatus } from '../lib/filamentStatus';
import type { SpoolKind } from '../types';

/** Texts that describe where a group is, shared by the card view and the list. */
export function useGroupPlaceText(group: SpoolGroup): { label: string; note: string | null } {
  const t = useT();
  const { place } = group;
  if (place.kind === 'storage') return { label: place.location || t('noValue'), note: null };
  const slots = place.slots.map((n) => t('filamentGroupSlot').replace('{n}', String(n))).join(', ');
  return { label: place.label, note: [slots, t('filamentGroupBooksHere')].filter(Boolean).join(' · ') };
}

/** Status chip text; a healthy group in a printer reads "Loaded" instead of "In stock". */
export function useGroupStatusLabel(): (status: FilamentStockStatus, loaded: boolean) => string {
  const t = useT();
  return (status, loaded) =>
    status === 'empty' ? t('filamentStatusEmpty') : status === 'low' ? t('filamentStatusLow') : loaded ? t('filamentGroupLoaded') : t('filamentStatusOk');
}

/** Keyed texts of the group header, by kind (spool or bottle wording). */
export function useGroupKindTexts(kind: SpoolKind) {
  const t = useT();
  return {
    count: kind === 'resin' ? t('resinGroupCount') : t('filamentGroupCount'),
    emptyCount: kind === 'resin' ? t('resinGroupEmptyCount') : t('filamentGroupEmptyCount'),
    show: kind === 'resin' ? t('resinGroupShow') : t('filamentGroupShow'),
    hide: kind === 'resin' ? t('resinGroupHide') : t('filamentGroupHide'),
  };
}
