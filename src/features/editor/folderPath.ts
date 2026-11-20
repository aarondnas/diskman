// Helpers to show a folder path as a tile: the folder name is the main label, the parent folders
// are only a hint below it.

/** Last folder name of a path; a bare drive like "E:\" has none. */
export function lastSegment(path: string): string {
  const last = path.trim().replace(/[\\/]+$/, '').split(/[\\/]/).pop() ?? '';
  return /^[a-z]:$/i.test(last) ? '' : last;
}

/** Appends the folder unless the path already ends with it (e.g. E:\Backup\FK for folder "FK"). */
export function withFolder(path: string, folder: string): string {
  const base = path.trim().replace(/[\\/]+$/, '');
  if (!folder || lastSegment(base).toLowerCase() === folder.toLowerCase()) return base;
  return `${base}\\${folder}`;
}

/** A path split for a tile: the folder name, a shortened parent, and whether it is a bare drive. */
export interface SplitPath {
  name: string;
  parent: string;
  drive: boolean;
}

/** Splits a path into the folder name and a shortened parent, e.g. "C:\…\aaron\Documents". */
export function splitPath(path: string): SplitPath {
  const clean = path.trim().replace(/[\\/]+$/, '');
  if (/^[a-z]:$/i.test(clean)) return { name: clean.toUpperCase(), parent: '', drive: true };
  const i = Math.max(clean.lastIndexOf('\\'), clean.lastIndexOf('/'));
  if (i < 0) return { name: clean, parent: '', drive: false };
  const parts = clean.slice(0, i).split(/[\\/]/);
  // UNC paths start with two empty parts ("\\server")
  const head = parts[0] === '' && parts[1] === '' ? `\\\\${parts[2] ?? ''}` : parts[0];
  const rest = parts[0] === '' && parts[1] === '' ? parts.slice(3) : parts.slice(1);
  const parent = rest.length > 2 ? `${head}\\…\\${rest.slice(-2).join('\\')}` : [head, ...rest].join('\\');
  return { name: clean.slice(i + 1), parent: parent.length === 2 ? `${parent}\\` : parent, drive: false };
}

/** Text from the system clipboard that looks like a folder path, e.g. from "Copy as path" in Explorer. */
export function parsePath(text: string | null | undefined): string {
  const p = (text ?? '').trim().replace(/^"(.*)"$/, '$1');
  return !p.includes('\n') && /^([a-z]:[\\/]|[a-z]:$|\\\\[^\\])/i.test(p) ? p : '';
}
