// redesign-final-h2 (3.71.0's final review, 2026-10-04): an action dialog
// read its preview's raw words out ("No CLI line: cannot read
// /home/…/lxc-compose.yml (os error 2)", "No preview: exec _host") and
// kept its primary button armed. This turns a preview (or its refusal)
// into whether the action can run, one plain sentence why not, and the
// notes worth a person's eye. Pure, so `node --test` drives every case.

/**
 * @typedef {{action: string, submit: string, stack: string}} GateForm
 * @typedef {{cli?: string | null, cli_unavailable?: string | null,
 *   plan?: unknown, apply?: unknown, plan_unavailable?: string | null}}
 *   GatePreview
 * @typedef {{ready: boolean, reason: string, notes: string[]}} Gate
 */

/** The actions whose run is the plan their preview shows. */
const PLANNED = new Set(["deploy", "deploy-commit"]);

/** @param {string} s */
const capital = (s) => (s ? s[0].toUpperCase() + s.slice(1) : s);
/** @param {string} s */
const sentence = (s) => {
  const t = s.trim().replace(/[.\s]+$/, "");
  // A stack's name keeps its own spelling at the start of a sentence.
  return t ? `${/^[a-z0-9-]+ has /.test(t) ? t : capital(t)}.` : "";
};

/**
 * A host message in a person's words: no absolute paths (a stack's own
 * files keep their name), no "(os error N)", no Rust error chain words.
 * @param {string} text
 * @param {string} stack
 */
export function plainWords(text, stack) {
  const missingFile =
    /(has no|cannot read)[^:]*?\b(lxc-compose|service)\.yml\b/.test(text) ||
    (/No such file or directory/.test(text) &&
      /(lxc-compose|service)\.yml/.test(text));
  if (missingFile) {
    const file = /service\.yml/.test(text) ? "service.yml" : "lxc-compose.yml";
    return `${stack} has no stack file (${file}) in the repository, so there is nothing to deploy: add its files, or remove the stack with Deploy all changes`;
  }
  return text
    .replace(/\s*\(os error \d+\)/g, "")
    .replace(/(?:\/[\w.@+-]+)+\/(stacks\/[\w.@+/-]+)/g, "$1")
    .replace(/(?:\/[\w.@+-]+){2,}\/([\w.@+-]+)/g, "$1")
    .trim();
}

/**
 * @param {GateForm} form
 * @param {GatePreview | null} p the preview's answer, null when refused
 * @param {import("./doctor.js").RouteError | null} err the refusal
 * @returns {Gate}
 */
export function previewGate(form, p, err) {
  if (err) {
    const why = err.why.replace(
      new RegExp(`^${form.action.replace(/[-]/g, "[- ]")}\\b`, "i"),
      form.submit,
    );
    const fix = err.fix ? `: ${err.fix.trim().replace(/[.\s]+$/, "")}` : "";
    return {
      ready: false,
      reason: sentence(`${capital(plainWords(why, form.stack))}${fix}`),
      notes: [],
    };
  }
  if (!p) return { ready: false, reason: "", notes: [] };
  /** @type {string[]} */
  const notes = [];
  const plan = p.plan_unavailable
    ? sentence(plainWords(p.plan_unavailable, form.stack))
    : "";
  if (PLANNED.has(form.action) && plan)
    return { ready: false, reason: plan, notes: [] };
  if (plan) notes.push(plan);
  if (!p.cli && p.cli_unavailable) {
    const cli = sentence(plainWords(p.cli_unavailable, form.stack));
    if (cli && cli !== plan)
      notes.push(`There is no workstation command for this one: ${cli}`);
  }
  return { ready: true, reason: "", notes };
}

/**
 * A description's `inline code` as parts: plain strings and `{code}`.
 * @param {string} text
 * @returns {(string | {code: string})[]}
 */
export function codeSpans(text) {
  /** @type {(string | {code: string})[]} */
  const out = [];
  text.split("`").forEach((part, i) => {
    if (!part) return;
    out.push(i % 2 ? { code: part } : part);
  });
  return out;
}
