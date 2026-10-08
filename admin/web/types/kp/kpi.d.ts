// The kp-themes module chassis-rs serves at /static/kp/js/kpi.js (kp-themes 10.0.0),
// copied from kp-themes js/kpi.d.ts: chassis serves the module, not its types.
/** An inline SVG drawn from the numbers in its attribute. */
export declare const SPARK = "svg[data-kp-spark]";
/**
 * A sparkline's two paths in a `width` × `height` box: the line, and the
 * area under it. Fewer than two finite numbers draw nothing; a flat series
 * is drawn through the middle.
 * @param {readonly number[]} values oldest first
 * @param {{ width?: number, height?: number }} [box]
 * @returns {{ line: string, area: string }}
 */
export declare function sparkPaths(
  values: readonly number[],
  {
    width,
    height,
  }?: {
    width?: number;
    height?: number;
  },
): {
  line: string;
  area: string;
};
/**
 * Draw one sparkline into `svg` from `values`. `parts` names its two paths'
 * classes (`<parts>-area`, `<parts>-line`): a KPI tile's by default, the
 * time chart's spark variant passes its own (js/chart.js).
 * @param {SVGSVGElement} svg
 * @param {readonly number[]} values
 * @param {{ parts?: string }} [options]
 */
export declare function drawSparkline(
  svg: SVGSVGElement,
  values: readonly number[],
  {
    parts,
  }?: {
    parts?: string;
  },
): void;
/**
 * Draw every `svg[data-kp-spark="12 14 13 …"]` under `root`, and redraw one
 * whenever its numbers change.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachSparklines(root?: ParentNode): () => void;
/** A KPI tile that turns a filter on this page on or off. */
export declare const KPI_TOGGLE = "button.kp-kpi--toggle";
/** Fired on the tile, bubbling, after it was pressed: `detail.pressed`. */
export declare const KPI_TOGGLE_EVENT = "kp-kpi-toggle";
/**
 * Wire every KPI toggle under `root`: a click flips `aria-pressed` and fires
 * `kp-kpi-toggle`; the page does the filtering. A tile marked
 * `data-kp-kpi-owned` is left to the page (a framework that keeps the
 * pressed state itself).
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachKpiToggles(root?: ParentNode): () => void;
/** A used-of-total bar: anywhere, or at the bottom of a key figure. */
export declare const METER = ".kp-meter, .kp-kpi__meter";
/**
 * What a meter draws for a share and a mark: the fill (0 to 1), whether the
 * share runs past the end, the mark's place (0 to 1, null without one) and
 * whether it lies past the end, and the `aria-valuenow` (0 to 100). A share
 * or a mark below 0 counts as 0; one that is not a number is not measured.
 * @param {number | null | undefined} value 0..∞ (1 is full)
 * @param {number | null | undefined} [mark] 0..∞
 * @returns {{ measured: boolean, fill: number, over: boolean, mark: number | null, markOver: boolean, now: number | null }}
 */
export declare function meterParts(
  value: number | null | undefined,
  mark?: number | null | undefined,
): {
  measured: boolean;
  fill: number;
  over: boolean;
  mark: number | null;
  markOver: boolean;
  now: number | null;
};
/**
 * The words a meter is read by: the real share, never clamped
 * (`130% booked for tonight`), and the mark's share with its label, as
 * `62% full; 80% the target level`. No number reads `meterNotMeasured`.
 * @param {number | null | undefined} value 0..∞ (1 is full)
 * @param {number | null | undefined} [mark] 0..∞
 * @param {{ unit?: string, label?: string, markLabel?: string }} [words] `label` is `meterUsed` unless given
 * @returns {string}
 */
export declare function meterText(
  value: number | null | undefined,
  mark?: number | null | undefined,
  {
    unit,
    label,
    markLabel,
  }?: {
    unit?: string;
    label?: string;
    markLabel?: string;
  },
): string;
export type Meter = {
  /**
   * the share, 0 up (1 is full; above 1 the bar is full and says so); null is not measured
   */
  value?: number | null;
  /**
   * a tick at this share (a target, a limit, what is booked), 0 up; null is none
   */
  mark?: number | null;
  /**
   * the fill's colour; a tone on the tile around it does the same
   */
  tone?: "warning" | "destructive" | null;
  /**
   * what the share is, in the words a screen reader hears ("full"); `meterUsed` unless given
   */
  label?: string;
  /**
   * what the mark is ("the target level")
   */
  markLabel?: string;
  /**
   * the track pulses at its height, with no fill and no mark
   */
  loading?: boolean;
};
/**
 * @typedef {object} Meter
 * @property {number | null} [value] the share, 0 up (1 is full; above 1 the bar is full and says so); null is not measured
 * @property {number | null} [mark] a tick at this share (a target, a limit, what is booked), 0 up; null is none
 * @property {'warning' | 'destructive' | null} [tone] the fill's colour; a tone on the tile around it does the same
 * @property {string} [label] what the share is, in the words a screen reader hears ("full"); `meterUsed` unless given
 * @property {string} [markLabel] what the mark is ("the target level")
 * @property {boolean} [loading] the track pulses at its height, with no fill and no mark
 */
/**
 * Write a `.kp-meter`'s share, mark, tone and ARIA in one call: `--kp-value`
 * and `--kp-mark`, `role="meter"` with `aria-valuenow` (0 to 100) and an
 * `aria-valuetext` that names the real share (meterText()). Above 1 the
 * fill is full and the meter carries `data-kp-over`; a mark above 1 sits at
 * the end with a ▸ and carries it too. The mark is a `.kp-meter__mark`
 * child, made the first time a meter has one.
 * @param {HTMLElement} el
 * @param {Meter} meter
 */
export declare function setMeter(
  el: HTMLElement,
  { value, mark, tone, label, markLabel, loading }: Meter,
): void;
/** A strip whose column count comes from a list of allowed counts. */
export declare const KPI_STRIP = ".kp-kpis[data-kp-kpis-columns]";
/**
 * How many columns a strip of `n` tiles takes at `width`: the first allowed
 * count whose tiles are at least `minTilePx` wide; then, if that leaves one
 * tile alone on the last row, the next smaller allowed count (two or more)
 * that does not; failing that the count stays and the last tile spans its
 * row. `all` in the list is `n`.
 * @param {number} n
 * @param {number} width the strip's inner width, px
 * @param {{ allowed?: (number | 'all')[] | string, minTilePx?: number, gapPx?: number }} [options]
 * @returns {{ columns: number, spanLast: boolean }}
 */
export declare function kpiColumns(
  n: number,
  width: number,
  {
    allowed,
    minTilePx,
    gapPx,
  }?: {
    allowed?: (number | "all")[] | string;
    minTilePx?: number;
    gapPx?: number;
  },
): {
  columns: number;
  spanLast: boolean;
};
/**
 * Put one strip on its column count now: kpiColumns() over its shown
 * tiles, its inner width (or `width`), `--kp-kpi-min` (9rem) and its column
 * gap; writes `--kp-kpis-columns` and `data-kp-kpis-span-last`. A tile's
 * label is one line and never cut, so while one does not fit its tile the
 * strip takes its next smaller allowed count: the tile grows to its label,
 * and every tile in the row with it [fix-99]. The count thus respects the
 * widest label as well as `--kp-kpi-min`, at desk width and in a phone
 * pane alike [fix-101].
 * @param {HTMLElement} strip
 * @param {number} [width] px; else measured
 */
export declare function fitKpiStrip(strip: HTMLElement, width?: number): void;
/**
 * Keep every `.kp-kpis[data-kp-kpis-columns="all 3 2 1"]` under `root` on an
 * allowed column count, by the strip's own width (not the window's), with no
 * tile alone on a row; and again when tiles come, go or hide, and for a
 * strip added later. Before this runs a strip keeps the package's auto-fit.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachKpiStrips(root?: ParentNode): () => void;
