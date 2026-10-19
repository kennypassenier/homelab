// The kp-themes module chassis-rs serves at /static/kp/js/reason.js (kp-themes 11.0.0),
// copied from kp-themes js/reason.d.ts: chassis serves the module, not its types.
/** A control that carries a reason. */
export declare const DISABLED_REASON = "[data-kp-disabled-reason]";
/**
 * Wire every control under `root` that carries `data-kp-disabled-reason`.
 *
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachDisabledReasons(root?: ParentNode): () => void;
