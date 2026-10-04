import { useState, type CSSProperties } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { canonicalTag, sortTagsForDisplay, tagLabel } from '../lib/autoTags';
import { AutocompleteInput } from './AutocompleteInput';

interface Props {
  allTags: string[];
  tags: string[];
  onAddTag: (tag: string) => void;
  inputClassName?: string;
  inputStyle?: CSSProperties;
  stripHash?: boolean;
  onEscape?: () => void;
}

// Shared tag rules for the compact panel, comfortable panel and detail page.
export function TagInput({ allTags, tags, onAddTag, inputClassName, inputStyle, onEscape, stripHash = false }: Props) {
  const [draft, setDraft] = useState('');
  const { language } = useLanguage();
  const t = useT();
  const assigned = new Set(tags.map(canonicalTag));
  const options = sortTagsForDisplay(
    [...new Set(allTags)].filter(tag => !assigned.has(canonicalTag(tag))).map(label => ({ label, count: 0, colorHue: 0 })),
    language,
  ).map(tag => tag.label);

  return <AutocompleteInput
    value={draft}
    onChange={setDraft}
    options={options}
    onSubmit={raw => {
      const trimmed = raw.trim();
      const value = canonicalTag(stripHash ? trimmed.replace(/^#/, '') : trimmed);
      if (value && !assigned.has(value)) onAddTag(value);
      setDraft('');
    }}
    onEscape={onEscape}
    formatOption={label => tagLabel(label, language)}
    placeholder={t('addTagPlaceholder')}
    aria-label={t('addTagPlaceholder')}
    listAriaLabel={t('tagSuggestionsAria')}
    inputClassName={inputClassName}
    inputStyle={inputStyle}
  />;
}
