// The colour a log source takes. kp-themes 12's bundle does not export
// `sourceColour` / `sourceIndex` (kp js/log.js has them), so this is a copy
// of that hash until a kp-themes release exports it (see kp-gaps/homelab.md).
// Same FNV-1a as kp-tui, so a source keeps its colour in the terminal too.

/** The 32-bit FNV-1a offset basis. */
const FNV_OFFSET_BASIS = 0x811c_9dc5;
/** The 32-bit FNV-1a prime. */
const FNV_PRIME = 0x0100_0193;
/** How many chart colours the register has. */
const CHART_COLOURS = 5;

/**
 * @param {string} name
 * @returns {number} 1 to 5, the chart colour the name takes
 */
export function sourceIndex(name) {
  let hash = FNV_OFFSET_BASIS;
  for (const byte of new TextEncoder().encode(name)) {
    hash ^= byte;
    hash = Math.imul(hash, FNV_PRIME) >>> 0;
  }
  return (hash % CHART_COLOURS) + 1;
}

/**
 * The value to give a colour property for `name`, by reference to the
 * register's chart colour, so a theme switch moves it.
 * @param {string} name
 */
export const sourceColour = (name) => `var(--chart-${sourceIndex(name)})`;
