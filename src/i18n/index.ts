import { createContext, useContext } from "react";
import en from "./en.json";
import ru from "./ru.json";

export type Lang = "en" | "ru";
export type Key = keyof typeof en;
// Typed so `tsc` fails if ru.json is missing a key from en.json.
const dicts: Record<Lang, Record<Key, string>> = { en, ru };

export const makeT = (lang: Lang) => (key: Key, vars?: Record<string, string | number>) => {
  let s = dicts[lang][key] ?? en[key] ?? key;
  for (const [k, v] of Object.entries(vars ?? {})) s = s.replaceAll(`{${k}}`, String(v));
  return s;
};

export const LangCtx = createContext<{ lang: Lang; t: ReturnType<typeof makeT> }>({ lang: "en", t: makeT("en") });
export const useT = () => useContext(LangCtx);
