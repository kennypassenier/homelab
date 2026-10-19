/** An opener. */
export declare const PREVIEW = "[data-kp-preview]";
/** A glyph. */
export declare const DOC_GLYPH = ".kp-docglyph";
/** The kinds of a document glyph. */
export declare const DOC_KINDS: readonly [
  "pdf",
  "doc",
  "sheet",
  "image",
  "text",
  "unknown",
];
/** How long a frame may take before the preview is called failed, in ms. */
export declare const PREVIEW_TIMEOUT_MS = 15000;
/** The kind of glyph a file name or type means. Pure. @param {string} nameOrType */
export declare function docKindOf(
  nameOrType: string,
): "doc" | "image" | "pdf" | "sheet" | "text" | "unknown";
/** The plain name of a file: the last part of the address, decoded, no query. Pure. @param {string} url */
export declare function plainFileName(url: string): string;
/** What the preview should draw a file as. Pure over the extension. @param {string} url @param {string} [contentType] */
export declare function previewTypeOf(
  url: string,
  contentType?: string,
): "image" | "pdf" | "text";
export type PreviewOptions = {
  url: string;
  type?: "auto" | "pdf" | "image" | "text";
  engine?: "native" | "none";
  size?: "wide" | "full";
  /**
   * open, download, print, copy-link
   */
  actions?: readonly string[];
  title?: string;
  page?: number;
  sandbox?: "strict" | "off";
  /**
   * the saved file name
   */
  name?: string;
  opener?: Element | null;
  /**
   * documents the viewer can step through
   */
  group?: readonly {
    url: string;
    title?: string;
    name?: string;
  }[];
  /**
   * where in the group this one is
   */
  index?: number;
  doc?: Document;
};
/**
 * @typedef {object} PreviewOptions
 * @property {string} url
 * @property {'auto' | 'pdf' | 'image' | 'text'} [type]
 * @property {'native' | 'none'} [engine]
 * @property {'wide' | 'full'} [size]
 * @property {readonly string[]} [actions] open, download, print, copy-link
 * @property {string} [title]
 * @property {number} [page]
 * @property {'strict' | 'off'} [sandbox]
 * @property {string} [name] the saved file name
 * @property {Element | null} [opener]
 * @property {readonly { url: string, title?: string, name?: string }[]} [group] documents the viewer can step through
 * @property {number} [index] where in the group this one is
 * @property {Document} [doc]
 */
/**
 * Open a document in the viewer.
 * @param {PreviewOptions} options
 * @returns {import('./viewer.js').ViewerHandle}
 */
export declare function openDocPreview(
  options: PreviewOptions,
): import("./viewer.js").ViewerHandle;
/**
 * Draw every glyph under `root`.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachDocGlyphs(root?: ParentNode): () => void;
/**
 * Build a glyph.
 * @param {Document} doc @param {'pdf' | 'doc' | 'sheet' | 'image' | 'text' | 'unknown'} kind @param {{ size?: 'sm' | 'md' | 'lg', lines?: number, state?: 'ready' | 'missing' }} [options]
 */
export declare function docGlyph(
  doc: Document,
  kind: "pdf" | "doc" | "sheet" | "image" | "text" | "unknown",
  options?: {
    size?: "sm" | "md" | "lg";
    lines?: number;
    state?: "ready" | "missing";
  },
): HTMLSpanElement;
/**
 * Wire every opener under `root`.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachDocPreviews(root?: ParentNode): () => void;
