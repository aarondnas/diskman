// Translations of the UI. One file per language in ./locales; English is the reference and the
// fallback for missing keys. A new language: add the file, import it below and list it in LANGUAGES.
//
// Usage: t('backups.empty') or t('list.waiting', { place: 2 }). A value can be an object with
// plural forms ({ one, other }), chosen by Intl.PluralRules from the `count` parameter, or with one
// text per system ({ windows, linux }), chosen by the system the UI runs on (src/platform); both
// can be nested, the system first. Every language has the keys of en.ts (types.ts checks that).
// t() reads reactive state, so everything rendered with it switches language immediately.

import { OS } from '../platform/index.ts';
import { loadPreference, savePreference } from '../utils/storage.ts';
import type { Messages, PerSystem, Plural, Text, TextKey, TextParams } from './types.ts';
import en from './locales/en.ts';
import de from './locales/de.ts';
import fr from './locales/fr.ts';
import es from './locales/es.ts';
import it from './locales/it.ts';
import nl from './locales/nl.ts';
import pt from './locales/pt.ts';

export type { TextKey, TextParams } from './types.ts';

const DICTS = { en, de, fr, es, it, nl, pt } satisfies Record<string, Messages>;

/** A language code with a translation, e.g. 'de'. */
export type Language = keyof typeof DICTS;

/** Selectable languages; `name` in the language itself, so everyone finds their own. */
export const LANGUAGES: { code: Language; name: string }[] = [
  { code: 'en', name: 'English' },
  { code: 'de', name: 'Deutsch' },
  { code: 'fr', name: 'Français' },
  { code: 'es', name: 'Español' },
  { code: 'it', name: 'Italiano' },
  { code: 'nl', name: 'Nederlands' },
  { code: 'pt', name: 'Português' },
];

/** Preference value for "follow the system language". */
export const SYSTEM = 'system';

/** A language code or SYSTEM. */
export type LanguagePreference = Language | typeof SYSTEM;

const isLanguage = (code: unknown): code is Language => typeof code === 'string' && Object.hasOwn(DICTS, code);

// Copy of the preference so the next start shows the right language before the config is loaded
const STORAGE_KEY = 'language';

/** First supported language of the operating system, otherwise English. */
export function systemLanguage(): Language {
  const wanted = navigator.languages?.length ? navigator.languages : [navigator.language];
  for (const tag of wanted) {
    const base = String(tag ?? '').slice(0, 2).toLowerCase();
    if (isLanguage(base)) return base;
  }
  return 'en';
}

function resolve(pref: LanguagePreference): Language {
  return pref !== SYSTEM && isLanguage(pref) ? pref : systemLanguage();
}

const initial = loadPreference<LanguagePreference>(
  STORAGE_KEY,
  SYSTEM,
  (v): v is LanguagePreference => v === SYSTEM || isLanguage(v),
);
const state = $state<{ pref: LanguagePreference; locale: Language }>({ pref: initial, locale: resolve(initial) });
document.documentElement.lang = state.locale;

export const i18n = {
  /** Language in use, e.g. "de". */
  get locale(): Language {
    return state.locale;
  },
  /** Chosen preference: a language code or SYSTEM. */
  get preference(): LanguagePreference {
    return state.pref;
  },
};

/** Applies a preference (language code, SYSTEM or null for SYSTEM) and remembers it locally. */
export function setLanguage(pref: string | null | undefined) {
  state.pref = isLanguage(pref) ? pref : SYSTEM;
  state.locale = resolve(state.pref);
  document.documentElement.lang = state.locale;
  savePreference(STORAGE_KEY, state.pref);
}

type Node = Text | { [key: string]: Node } | undefined;

function lookup(dict: Messages, key: string): Text | undefined {
  const node = key.split('.').reduce<Node>((n, part) => (n as Record<string, Node> | undefined)?.[part], dict as Node);
  return node as Text | undefined;
}

const pluralRules: Partial<Record<Language, Intl.PluralRules>> = {};

export function t(key: TextKey, params: TextParams = {}): string {
  const locale = state.locale;
  let value: Text | undefined = lookup(DICTS[locale], key) ?? lookup(en, key);
  if (value == null) return key;
  // A text per system
  if (typeof value === 'object' && OS in value) value = (value as PerSystem)[OS];
  if (typeof value === 'object') {
    const rules = (pluralRules[locale] ??= new Intl.PluralRules(locale));
    const forms = value as Plural;
    value = forms[rules.select(Number(params.count ?? 0))] ?? forms.other;
  }
  return value.replace(/\{(\w+)\}/g, (match, name: string) => (name in params ? String(params[name]) : match));
}
