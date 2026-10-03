// redesign-presets (3.71.0, Kenny's approved demo presets.html): the
// Presets gallery's words and order, pure so they are tested without a
// browser.

/**
 * @typedef {{name: string, description: string, ram_mb: number,
 *   cores: number, disk_gb: number, cores_set?: boolean,
 *   disk_set?: boolean, apps: string[], gpu?: boolean, vpn?: boolean}}
 *   PresetView
 */

/**
 * The preset an empty stack starts from: the first one with no apps
 * (`presets/custom`, "Empty stack — add apps later"). It is drawn as the
 * dashed "Empty stack" card at the end, not as a preset of its own.
 * @param {PresetView[]} list
 * @returns {PresetView | null}
 */
export const emptyPreset = (list) =>
  list.find((p) => (p.apps ?? []).length === 0) ?? null;

/**
 * The gallery's cards: every preset but the empty one, filtered by the
 * search (name, description or an app) and sorted A–Z or largest first.
 * @param {PresetView[]} list
 * @param {string} q lower-case
 * @param {"name" | "ram"} sort
 */
export function galleryCards(list, q, sort) {
  const empty = emptyPreset(list);
  return list
    .filter((p) => p !== empty)
    .filter(
      (p) =>
        !q ||
        [p.name, p.description, ...(p.apps ?? [])]
          .join(" ")
          .toLowerCase()
          .includes(q),
    )
    .sort((a, b) =>
      sort === "ram"
        ? b.ram_mb - a.ram_mb || a.name.localeCompare(b.name)
        : a.name.localeCompare(b.name),
    );
}

/**
 * Memory in the demo's words: 512 MB, 1 GB, 4 GB.
 * @param {number} mb
 */
export const ramText = (mb) =>
  mb >= 1024 && mb % 1024 === 0
    ? `${mb / 1024} GB`
    : mb >= 1024
      ? `${(mb / 1024).toFixed(1)} GB`
      : `${mb} MB`;

/**
 * The toolbar's count: "8 presets · next free container 107", or "2 of 8
 * presets" while a search narrows them.
 * @param {number} shown
 * @param {number} total
 * @param {string} q
 * @param {number | null} next
 */
export const countText = (shown, total, q, next) =>
  q
    ? `${shown} of ${total} presets`
    : `${total} preset${total === 1 ? "" : "s"}${next == null ? "" : ` · next free container ${next}`}`;

/**
 * Whether a preset sets its own cores / disk (an older dashboard did not
 * say: then the value is shown as the preset's).
 * @param {PresetView} p
 */
export const sets = (p) => ({
  cores: p.cores_set !== false,
  disk: p.disk_set !== false,
});
