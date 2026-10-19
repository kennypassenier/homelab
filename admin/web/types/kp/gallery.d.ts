/** A gallery. */
export declare const GALLERY = "[data-kp-gallery]";
/** Fired on the gallery when a picture was opened: `{ id, item }`. */
export declare const GALLERY_OPEN_EVENT = "kp-gallery-open";
/** Fired on the gallery when the lightbox closed: `{ id }`. */
export declare const GALLERY_CLOSE_EVENT = "kp-gallery-close";
/** Fired on the gallery when the Retry of a failed list was pressed. */
export declare const GALLERY_RETRY_EVENT = "kp-gallery-retry";
/** Fired on the gallery when pictures were chosen to add: `{ files }`. */
export declare const GALLERY_ADD_EVENT = "kp-gallery-add";
/** The states a gallery can be in. */
export declare const GALLERY_STATES: readonly [
  "ready",
  "loading",
  "empty",
  "error",
];
/** How long the controls stay before they fade, in ms. */
export declare const GALLERY_CHROME_MS = 3000;
export type GalleryItem = {
  id: string;
  /**
   * the original
   */
  src: string;
  /**
   * the small one (default the original)
   */
  thumb?: string;
  width?: number;
  height?: number;
  alt?: string;
  caption?: string;
  kind?: "image" | "video";
  /**
   * a video's still
   */
  poster?: string;
  uploader?: string;
  /**
   * when it was added, in ms
   */
  at?: number;
  bytes?: number;
};
export type GalleryAction = {
  label: string;
  tone?: "destructive" | "default";
  run: () => void;
  /**
   * a question: the package's confirmation dialog asks it first
   */
  confirm?: string;
};
export type GallerySpec = {
  items?: readonly GalleryItem[];
  state?: "ready" | "loading" | "empty" | "error";
  /**
   * the words of the empty and failed plates
   */
  message?: string;
  label?: string;
  actions?: (item: GalleryItem) => readonly GalleryAction[];
  /**
   * show the Add pictures tile (the gallery fires `kp-gallery-add`)
   */
  add?: boolean;
  /**
   * how many waiting tiles a loading gallery shows (default 8)
   */
  skeleton?: number;
};
export type Memory = {
  items: GalleryItem[];
  spec: GallerySpec;
  card: import("./busy-card.js").BusyCard | null;
  /**
   * the reason the add tile is off, else null
   */
  uploading: string | null;
  viewer: import("./viewer.js").ViewerHandle | null;
  openId: string | null;
};
/** The pictures of a gallery. @param {Element} el */
export declare function galleryItems(el: Element): {
  id: string;
  src: string;
  thumb?: string;
  width?: number;
  height?: number;
  alt?: string;
  caption?: string;
  kind?: "image" | "video";
  poster?: string;
  uploader?: string;
  at?: number;
  bytes?: number;
}[];
/** The picture a gallery last opened on this device, or null. @param {Element} el */
export declare function galleryLast(el: Element): string | null;
/**
 * Write a gallery.
 * @param {HTMLElement} el
 * @param {GallerySpec} spec
 */
export declare function setGallery(el: HTMLElement, spec: GallerySpec): void;
/**
 * Mark the add tile off with a reason while an upload runs, or on again with `null`.
 * @param {HTMLElement} el @param {string | null} reason
 */
export declare function setGalleryUploading(
  el: HTMLElement,
  reason: string | null,
): void;
/**
 * Open a picture in the lightbox.
 * @param {HTMLElement} el @param {string} id
 * @returns {import('./viewer.js').ViewerHandle | null}
 */
export declare function openGalleryItem(
  el: HTMLElement,
  id: string,
): import("./viewer.js").ViewerHandle | null;
/**
 * Wire every gallery under `root`: the hash the page opened with opens its picture.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachGalleries(root?: ParentNode): () => void;
/**
 * Open the picture the address names, if this gallery holds it (call after the items are set).
 * @param {HTMLElement} el
 * @returns {boolean}
 */
export declare function openGalleryFromHash(el: HTMLElement): boolean;
