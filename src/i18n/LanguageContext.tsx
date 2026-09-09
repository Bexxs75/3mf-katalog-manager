import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from 'react';
import type { Language, Translations } from './types';
import { de } from './de';
import { en } from './en';
import { es } from './es';
import { fr } from './fr';

const STORAGE_KEY = '3mf-katalog-language';

const DICTIONARIES: Record<Language, Translations> = { de, en, es, fr };

interface LanguageContextValue {
  language: Language;
  setLanguage: (next: Language) => void;
}

const LanguageContext = createContext<LanguageContextValue | null>(null);

export function LanguageProvider({ children }: { children: ReactNode }) {
  const [language, setLanguageState] = useState<Language>(() => {
    const stored = localStorage.getItem(STORAGE_KEY);
    return stored && stored in DICTIONARIES ? (stored as Language) : 'de';
  });

  useEffect(() => {
    document.documentElement.lang = language;
  }, [language]);

  const setLanguage = useCallback((next: Language) => {
    setLanguageState(next);
    localStorage.setItem(STORAGE_KEY, next);
  }, []);

  return (
    <LanguageContext.Provider value={{ language, setLanguage }}>
      {children}
    </LanguageContext.Provider>
  );
}

function useLanguageContext(): LanguageContextValue {
  const ctx = useContext(LanguageContext);
  if (!ctx) throw new Error('useLanguage/useT must be used within a LanguageProvider');
  return ctx;
}

export function useLanguage(): LanguageContextValue {
  return useLanguageContext();
}

export function useT() {
  const { language } = useLanguageContext();
  const dict = DICTIONARIES[language];
  return useCallback(<K extends keyof Translations>(key: K): Translations[K] => dict[key], [dict]);
}
