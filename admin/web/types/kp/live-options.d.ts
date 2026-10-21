/**
 * Call `onChange` when one of `attributes` of `element` gets a different value.
 *
 * @param {Element} element
 * @param {readonly string[]} attributes  the attribute names the module reads (`data-kp-…`)
 * @param {(names: string[]) => void} onChange  the names whose value differs from before, in the order they changed
 * @returns {() => void} stop
 */
export declare function watchOptions(
  element: Element,
  attributes: readonly string[],
  onChange: (names: string[]) => void,
): () => void;
