// The Settings page's shared state. `mount` builds one of these and hands it
// to the section modules, which read and write it instead of closing over
// each other's variables.

/**
 * @typedef {import("../../editforms.js").HostField} HostField
 * @typedef {{commit: string, subject: string, at: number}} CommitRef
 * @typedef {import("../../settingsview.js").Show} Show
 * @typedef {{sha256: string, path: string, fields: HostField[], unknown?: string[]}} HostPage
 */

/**
 * @typedef {object} Ctx
 * @property {HTMLElement} root
 * @property {AbortController} abort
 * @property {(() => void)[]} stops cleanups run when the page is left
 * @property {{q: string, show: Show}} S the search and the Show choice
 * @property {string | null} section the section to open at
 * @property {() => void} keepUrl
 * @property {HTMLElement} band the alert band under the header
 * @property {(what: string, error: import("../../doctor.js").RouteError | null, retry: () => void) => void} failed
 * @property {HostPage | null} page the host.toml as last read
 * @property {Set<string>} typed keys whose name was typed to confirm
 * @property {boolean} stale host.toml changed on the host while changes were staged
 * @property {{el: HTMLElement}} head the page header
 * @property {HTMLElement} ago the "read … ago" line
 * @property {HTMLElement} barSlot the Save bar's slot
 * @property {HTMLElement} errBox
 * @property {HTMLElement} hostBar
 * @property {HTMLElement} found the search's count
 * @property {{search: HTMLInputElement, el: HTMLElement}} tb
 * @property {{set: (v: string) => void}} showSeg
 * @property {HTMLElement} groupBox
 * @property {{card: HTMLElement, data: HTMLElement}} hsCard
 * @property {{card: HTMLElement, data: HTMLElement}} wc
 * @property {HTMLElement} wcBody
 * @property {(() => void) | null} detachBar
 * @property {HTMLElement | null} saveBtn
 * @property {(() => void) | null} detachRows
 * @property {(() => void) | null} unbridge
 * @property {() => void} filter
 * @property {(quiet?: boolean) => Promise<boolean>} loadHost
 * @property {(quiet?: boolean) => Promise<boolean>} loadRepo
 * @property {(answer: any) => void} saved
 * @property {() => void} refreshReadLine
 * @property {(key: string) => void} focusKey
 * @property {() => Promise<void>} attachBar
 */

export {};
