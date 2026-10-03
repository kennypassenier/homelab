// fix-241 (2026-10-02, Jellyfin 12.1): one file as a snapshot holds it,
// read-only — the Backups page's "Show a file…" dialog. The host reads it
// with `restic dump`, capped at 1 MiB; nothing is restored and nothing is
// written. Pure, so what the dialog says about a cut or a binary file is
// pinned by a node test without a DOM or a fetch.

/**
 * @typedef {{owner: string, snapshot: string, path: string,
 *   shown_bytes: number, truncated: boolean, cap_bytes: number,
 *   binary: boolean, text?: string | null}} SnapshotFile
 * @typedef {{text: string, notes: string[], tone: "neutral" | "warn"}}
 *   SnapshotFileView
 */

/**
 * The dashboard route that asks the host for one file.
 * @param {string} stack
 * @param {string} owner the repository: the app whose row this is
 * @param {string} snapshot an id, or "" / "latest" for the newest
 * @param {string} path relative to the app's backed-up directory, or
 *   absolute under it
 */
export function snapshotFileUrl(stack, owner, snapshot, path) {
  const snap = snapshot === "" ? "latest" : snapshot;
  return (
    `/data/backups/${encodeURIComponent(stack)}/${encodeURIComponent(owner)}` +
    `/${encodeURIComponent(snap)}/file?path=${encodeURIComponent(path)}`
  );
}

/**
 * What the dialog shows for the host's answer: the text (empty when the
 * file is not text), and the notes above it — where it came from, and
 * whether it was cut at the cap or is not text.
 * @param {SnapshotFile} f
 * @returns {SnapshotFileView}
 */
export function snapshotFileView(f) {
  const notes = [
    `${f.path} from snapshot ${f.snapshot} of ${f.owner}, read only: nothing was restored.`,
  ];
  let tone = /** @type {"neutral" | "warn"} */ ("neutral");
  if (f.truncated) {
    tone = "warn";
    notes.push(
      `Cut: the file is larger than ${Math.round(f.cap_bytes / 1024)} KiB; only its first ${f.shown_bytes} bytes are shown.`,
    );
  }
  if (f.binary) {
    tone = "warn";
    notes.push(
      `Not text (${f.shown_bytes} bytes read: a database, an image or a directory), so nothing is shown.`,
    );
  }
  return { text: f.binary ? "" : (f.text ?? ""), notes, tone };
}
