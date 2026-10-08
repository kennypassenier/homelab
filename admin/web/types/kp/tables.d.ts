// The kp-themes module chassis-rs serves at /static/kp/js/tables.js (kp-themes 10.0.0),
// copied from kp-themes js/tables.d.ts: chassis serves the module, not its types.
/** The wrapper this module upgrades. A contract value [TH26]. */
export declare const WRAP_SELECTOR = ".kp-table-wrap";
/** A wrapper the consumer keeps out of it: `data-kp-region="off"`. */
export declare const OPT_OUT = '[data-kp-region="off"]';
export type TableRegionHandle = {
  element: HTMLElement;
  label: () => string;
  setLabel: (text: string) => void;
};
/**
 * Make every table's scroll box a named region a keyboard can reach.
 *
 * @param {ParentNode} root
 * @param {{ label?: (wrap: HTMLElement, table: HTMLTableElement | null) => string | undefined, selector?: string }} [options]
 * @returns {(() => void) & { handles: TableRegionHandle[] }} detach
 */
export declare function attachTableRegions(
  root?: ParentNode,
  {
    label,
    selector,
  }?: {
    label?: (
      wrap: HTMLElement,
      table: HTMLTableElement | null,
    ) => string | undefined;
    selector?: string;
  },
): (() => void) & {
  handles: TableRegionHandle[];
};
