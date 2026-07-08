import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import {
  defaultLanguage,
  normalizeLanguage,
  translate,
  type Language,
} from "@/lib/i18n";

const languageStorageKey = "portico.language";

interface I18nContextValue {
  language: Language;
  setLanguage: (language: Language) => void;
  toggleLanguage: () => void;
  t: (key: string) => string;
}

const I18nContext = createContext<I18nContextValue | undefined>(undefined);

function readStoredLanguage(): Language {
  if (typeof window === "undefined") return defaultLanguage;
  return normalizeLanguage(window.localStorage.getItem(languageStorageKey));
}

export function I18nProvider({ children }: { children: ReactNode }) {
  const [language, setLanguageState] = useState<Language>(readStoredLanguage);

  useEffect(() => {
    window.localStorage.setItem(languageStorageKey, language);
    document.documentElement.lang = language === "zh" ? "zh-CN" : "en";
  }, [language]);

  const value = useMemo<I18nContextValue>(() => {
    function setLanguage(nextLanguage: Language) {
      setLanguageState(nextLanguage);
    }

    function toggleLanguage() {
      setLanguageState((current) => (current === "en" ? "zh" : "en"));
    }

    return {
      language,
      setLanguage,
      toggleLanguage,
      t: (key: string) => translate(language, key),
    };
  }, [language]);

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

// Hooks are intentionally colocated with the provider to keep the tiny i18n surface together.
// eslint-disable-next-line react-refresh/only-export-components
export function useTranslation() {
  const context = useContext(I18nContext);
  if (!context) {
    throw new Error("useTranslation must be used within I18nProvider");
  }
  return context;
}
