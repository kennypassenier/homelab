/** A cap whose text is the platform's modifier key. */
export declare const MOD_CAP = '.kp-kbd[data-kp-key="mod"]';
/** A line of shortcuts. */
export declare const KEYS = ".kp-keys";
export type KeyPair = [string | readonly string[], string];
/**
 * Write a line of shortcuts into `el`, replacing what it held. The element
 * becomes a `.kp-keys` group named "Keyboard shortcuts" unless it already has
 * a name.
 * @param {Element} el
 * @param {readonly KeyPair[]} pairs
 */
export declare function setKeys(el: Element, pairs: readonly KeyPair[]): void;
/**
 * Fill every `.kp-kbd[data-kp-key="mod"]` under `root` with the platform's
 * modifier (⌘ or Ctrl).
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachKeys(root?: ParentNode): () => void;
