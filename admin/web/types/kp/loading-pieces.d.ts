// The kp-themes module chassis-rs serves at /static/kp/js/loading-pieces.js (kp-themes 10.1.1),
// copied from kp-themes js/loading-pieces.d.ts: chassis serves the module, not its types.
/** The custom property a register names its pieces in. */
export declare const PIECES_PROPERTY = "--kp-loading-pieces";
/** The attribute a part carries while its pieces are there. */
export declare const PIECES_ATTRIBUTE = "data-kp-pieces";
/** The parts that wait, by the shape the pieces take on them. */
export declare const PIECE_CARRIERS: {
  readonly row: ".kp-skeleton:not(.kp-skeleton--circle, .kp-skeleton--block), .kp-progressbar[data-kp-indeterminate] > .kp-progressbar__track, :is(.kp-meter, .kp-kpi__meter)[data-kp-loading]";
  readonly day: ':is(.kp-calendar--busy-days, .kp-calendar--busy-whole) .kp-calendar__day[data-kp-tone="loading"], .kp-skeleton--circle';
  readonly plate: '.kp-skeleton--block, .kp-datatable__busy-panel, .kp-calendar--busy-whole:has(> .kp-calendar__grid[aria-busy="true"]) > .kp-calendar__busy, .kp-menu-button > .kp-menu[aria-busy="true"], .kp-kpi[aria-busy="true"], .kp-card[aria-busy="true"]';
  readonly spinner: ".kp-spinner";
};
/**
 * Watch `root` for parts that wait and give each the pieces its theme
 * names, for as long as it waits; a theme switch, a part that starts or
 * stops waiting and a part that changes size are followed.
 * @param {ParentNode} [root]
 * @returns {() => void} detach: every piece goes, the parts are as they came
 */
export declare function attachLoadingPieces(root?: ParentNode): () => void;
