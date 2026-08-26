import { createContext, useContext, useEffect, useMemo, type ReactNode } from "react";

import type { Language } from "../types";
import { en, type Dictionary } from "./en";
import { fa } from "./fa";

const dictionaries: Record<Language, Dictionary> = { en, fa };

export type TranslationKey = keyof Dictionary;

type Values = Record<string, string | number>;

interface Translator {
  language: Language;
  rtl: boolean;
  t: (key: TranslationKey, values?: Values) => string;
  finding: (code: string) => string;
  relative: (unixSeconds: number) => string;
  date: (unixSeconds: number) => string;
}

const context = createContext<Translator | null>(null);

export function I18nProvider({
  language,
  children,
}: {
  language: Language;
  children: ReactNode;
}) {
  const value = useMemo<Translator>(() => {
    const dictionary = dictionaries[language] ?? en;
    const locale = language === "fa" ? "fa-IR" : "en-GB";

    const t = (key: TranslationKey, values?: Values) => {
      const template = dictionary[key] ?? en[key] ?? String(key);
      if (!values) return template;
      return template.replace(/\{(\w+)\}/g, (match, name: string) =>
        name in values ? String(values[name]) : match,
      );
    };

    const span = (seconds: number) => {
      if (seconds < 45) return t("time.justNow");
      if (seconds < 3600) return t("time.minutes", { count: Math.round(seconds / 60) });
      if (seconds < 86400) return t("time.hours", { count: Math.round(seconds / 3600) });
      return t("time.days", { count: Math.round(seconds / 86400) });
    };

    return {
      language,
      rtl: language === "fa",
      t,
      finding: (code) => t(`finding.${code}` as TranslationKey),
      relative: (unixSeconds) => {
        const elapsed = Math.max(0, Math.floor(Date.now() / 1000) - unixSeconds);
        if (elapsed < 45) return t("time.justNow");
        return t("time.ago", { span: span(elapsed) });
      },
      date: (unixSeconds) =>
        new Date(unixSeconds * 1000).toLocaleDateString(locale, {
          year: "numeric",
          month: "short",
          day: "numeric",
        }),
    };
  }, [language]);

  useEffect(() => {
    document.documentElement.lang = language;
    document.documentElement.dir = language === "fa" ? "rtl" : "ltr";
  }, [language]);

  return <context.Provider value={value}>{children}</context.Provider>;
}

export function useI18n(): Translator {
  const value = useContext(context);
  if (!value) throw new Error("useI18n was called outside of I18nProvider");
  return value;
}
