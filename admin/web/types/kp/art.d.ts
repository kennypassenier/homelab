/** An element that gets a drawing. */
export declare const ART = "[data-kp-art]";
/** The kinds a drawing can show. */
export declare const ART_KINDS: readonly [
  "none",
  "nomatch",
  "error",
  "offline",
  "done",
  "locked",
];
export type ArtKind =
  "none" | "nomatch" | "error" | "offline" | "done" | "locked";
export type ArtInk = "rule" | "line" | "ink" | "red";
export type ArtStroke = [ArtInk, string];
/** Every set the package has, by the name a register shows. @type {Record<string, Record<ArtKind, ArtStroke[]>>} */
export declare const ART_SETS: Record<string, Record<ArtKind, ArtStroke[]>>;
/**
 * The drawing for one kind: an svg with every set's group, hidden from a screen reader.
 * @param {Document} doc
 * @param {ArtKind} kind
 * @returns {SVGSVGElement}
 */
export declare function drawArt(doc: Document, kind: ArtKind): SVGSVGElement;
/**
 * Give every element with `data-kp-art` under `root` its drawing, and follow a change of the kind.
 * @param {ParentNode} [root]
 * @returns {() => void} detach: the drawings go
 */
export declare function attachArt(root?: ParentNode): () => void;
