/** Every host that writes `data-kp-state` and fills its footprint with it. */
export declare const FOOTPRINT_HOSTS: string;
/** The cap on a held height, unless the host (or an ancestor) sets `--kp-footprint-max`. */
export declare const FOOTPRINT_MAX = "48rem";
/**
 * Hold one host's loaded height while it shows a state.
 *
 * @param {HTMLElement} host
 * @returns {() => void} detach
 */
export declare function holdFootprint(host: HTMLElement): () => void;
/**
 * Hold the footprint of every data component under `root`.
 *
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachFootprints(root?: ParentNode): () => void;
