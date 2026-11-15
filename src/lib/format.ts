// Display formatting for sizes, durations and UTC offsets.

export function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v < 10 ? 1 : 0)} ${units[i]}`;
}

/** `m:ss` for a duration in seconds. */
export function fmtDuration(seconds: number): string {
  return `${Math.floor(seconds / 60)}:${String(Math.round(seconds % 60)).padStart(2, "0")}`;
}

/** `+HH:MM` / `-HH:MM` for an offset in minutes east of UTC. */
export function fmtOffset(minutes: number): string {
  const sign = minutes < 0 ? "-" : "+";
  const a = Math.abs(minutes);
  return `${sign}${String(Math.floor(a / 60)).padStart(2, "0")}:${String(a % 60).padStart(2, "0")}`;
}

/** This device's current UTC offset, e.g. `+05:30`. */
export function deviceOffset(): string {
  return fmtOffset(-new Date().getTimezoneOffset());
}

/** Offsets in real use, UTC−12:00 to UTC+14:00: every half hour plus the three :45 zones. */
export function utcOffsets(): string[] {
  const quarterZones = [5 * 60 + 45, 8 * 60 + 45, 12 * 60 + 45];
  const out: string[] = [];
  for (let m = -12 * 60; m <= 14 * 60; m += 15) {
    if (m % 30 !== 0 && !quarterZones.includes(Math.abs(m))) continue;
    out.push(fmtOffset(m));
  }
  return out;
}
