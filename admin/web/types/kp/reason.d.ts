/** A control that carries a reason. */
export declare const DISABLED_REASON = "[data-kp-disabled-reason]";
/**
 * Wire every control under `root` that carries `data-kp-disabled-reason`.
 *
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachDisabledReasons(root?: ParentNode): () => void;
