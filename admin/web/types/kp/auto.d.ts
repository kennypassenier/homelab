// The kp-themes module chassis-rs serves at /static/kp/js/auto.js (kp-themes 11.0.0),
// copied from kp-themes js/auto.d.ts: chassis serves the module, not its types.
export type Need = {
  /**
   * the module, as `detach.modules` reports it
   */
  name: string;
  /**
   * every selector through which the module's attach functions act
   */
  when: string;
  load: () => Promise<any>;
  attach: (module: any, root: ParentNode) => unknown[];
  /**
   * attaches after every other module the same attachAll() fetched, and is asked for then
   */
  late?: boolean;
};
/**
 * @typedef {object} Need
 * @property {string} name the module, as `detach.modules` reports it
 * @property {string} when every selector through which the module's attach functions act
 * @property {() => Promise<any>} load
 * @property {(module: any, root: ParentNode) => unknown[]} attach
 * @property {boolean} [late] attaches after every other module the same attachAll() fetched, and is asked for then
 */
/** @type {Need[]} */
export declare const NEEDS: Need[];
/** Set on <html> once the boot's `attachAll()` has attached everything the page needed: the names fetched, space-separated. */
export declare const READY_ATTRIBUTE = "data-kp-auto-ready";
/**
 * Attach every behaviour under `root`, fetching only the modules it needs.
 * Returns one detach for all of it; `ready` resolves once every needed
 * module has attached, and `modules` names what was fetched.
 *
 * @param {ParentNode} [root]
 * @returns {(() => void) & { ready: Promise<void>, modules: string[] }}
 */
export declare function attachAll(root?: ParentNode): (() => void) & {
  ready: Promise<void>;
  modules: string[];
};
