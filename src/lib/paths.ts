// Path helpers that work with both Windows and POSIX separators.

/** Last path component: `C:\a\b.jpg` → `b.jpg`. */
export function baseName(p: string): string {
  return p.split(/[\\/]/).pop() ?? p;
}

/** Parent folder, or the path itself when it has no parent. */
export function dirName(p: string): string {
  const i = Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/"));
  return i > 0 ? p.slice(0, i) : p;
}

/** Upper-case extension without the dot, or "" when there is none. */
export function extOf(p: string): string {
  const b = baseName(p);
  const i = b.lastIndexOf(".");
  return i > 0 ? b.slice(i + 1).toUpperCase() : "";
}
