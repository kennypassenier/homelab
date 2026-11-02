// The small context every Host section factory receives from host.js.

/**
 * @typedef {object} HostEnv
 * @property {AbortController} abort aborted when the page is left
 * @property {(() => void)[]} stops cleanups run when the page is left
 * @property {{navigate: (href: string) => void, focus?: "checks"}} ctx
 */
export {};
