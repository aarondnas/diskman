// Colour of a backup ("visor"): the status lights of the Diskman rack that marks it (DiskmanHead.svelte).
import type { Backup } from '../api/types.ts';

/** Visor colour when a backup has none of its own: the amber of the app logo. Never stored. */
export const DEFAULT_VISOR = '#f2b84b';

/** Names of the offered colours, text keys under card.colors.* */
export type VisorName = 'amber' | 'orange' | 'red' | 'pink' | 'violet' | 'blue' | 'teal' | 'green';

/** Choices offered for a backup, all readable on the dark rack; [colour, name] */
export const VISORS: [colour: string, name: VisorName][] = [
  ['#f2b84b', 'amber'],
  ['#f08a3c', 'orange'],
  ['#e5604d', 'red'],
  ['#e46aa6', 'pink'],
  ['#9b7be0', 'violet'],
  ['#4b9be6', 'blue'],
  ['#3fb8a8', 'teal'],
  ['#5fbf6e', 'green'],
];

/** The visor colour of a backup: its own, or the default. */
export const visorOf = (backup: Pick<Backup, 'color'> | null | undefined) => (backup?.color || DEFAULT_VISOR).toLowerCase();
