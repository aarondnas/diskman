// Types of the translations, derived from the English reference (locales/en.ts): every language
// is a `Messages`, so a missing or extra key is a type error, and t() only accepts existing keys.
import type en from './locales/en.ts';

/** Plural forms, chosen by Intl.PluralRules from the `count` parameter. */
export interface Plural {
  zero?: string;
  one?: string;
  two?: string;
  few?: string;
  many?: string;
  other: string;
}

/** One text per system where the wording differs (src/platform). */
export interface PerSystem {
  windows: string | Plural;
  linux: string | Plural;
}

/** A text: plain, with plural forms, or one per system (which may have plural forms). */
export type Text = string | Plural | PerSystem;

/** Keys of a text object; an object with other keys is a group of texts (e.g. issues.reasons.other). */
type TextObjectKey = keyof Plural | keyof PerSystem;

type IsText<V> = V extends string ? true : [keyof V] extends [TextObjectKey] ? true : false;

/** The shape of a language: the keys of en.ts, each text free to be plain, plural or per system. */
type Shape<T> = { [K in keyof T]: IsText<T[K]> extends true ? Text : Shape<T[K]> };

export type Messages = Shape<typeof en>;

type Keys<T, Prefix extends string = ''> = {
  [K in keyof T & string]: IsText<T[K]> extends true ? `${Prefix}${K}` : Keys<T[K], `${Prefix}${K}.`>;
}[keyof T & string];

/** Every text key, e.g. 'backups.empty'. */
export type TextKey = Keys<typeof en>;

/** Values for the {name} placeholders of a text; `count` also picks the plural form. */
export type TextParams = Record<string, string | number>;
