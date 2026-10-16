import { useCanEditMetadata } from '../lib/webSession';
import { useState, type CSSProperties } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { canonicalTag, sortTagsForDisplay, tagLabel } from '../lib/autoTags';
import { AutocompleteInput } from './AutocompleteInput';

interface Props {
  allTags: string[];
  value?: string;
  onChange?: (value: string) => void;
  autoFocus?: boolean;
  tags: string[];
  onAddTag: (tag: string) => void;
  inputClassName?: string;
  inputStyle?: CSSProperties;
  stripHash?: boolean;
  onEscape?: () => void;
}

// Shared tag rules for the compact panel, comfortable panel and detail page.
export function TagInput({ allTags, tags, onAddTag, inputClassName, inputStyle, onEscape, stripHash = false, value, onChange, autoFocus }: Props) {
  const canEdit = useCanEditMetadata();
  const [localDraft, setLocalDraft] = useState('');
  const draft = value ?? localDraft;
  const setDraft = onChange ?? setLocalDraft;
  const { language } = useLanguage();
  const t = useT();
  const assigned = new Set(tags.map(canonicalTag));
  const options = sortTagsForDisplay(
    [...new Set(allTags)].filter(tag => !assigned.has(canonicalTag(tag))).map(label => ({ label, count: 0, colorHue: 0 })),
    language,
  ).map(tag => tag.label);

  if (!canEdit) return null;
  return <AutocompleteInput
    autoFocus={autoFocus}
    maxLength={100}
    maxSuggestionsWhenEmpty={200}
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
