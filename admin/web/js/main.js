// The fleet page (skeleton): first paint from /data/fleet, then live over
// SSE. A `resync` (the browser fell behind, or reconnected) refetches.

import { measuredAgo, ramText, stackState } from "./fleet.js";

/** @type {any} */
let fleet = null;

/** @param {string} id */
const el = (id) => /** @type {HTMLElement} */ (document.getElementById(id));

/**
 * @param {string} tag
 * @param {string} text
 * @param {string} [cls]
 */
function cell(tag, text, cls) {
  const c = document.createElement(tag);
  c.textContent = text;
  if (cls) c.className = cls;
  return c;
}

/** @param {{label: string, tone: string}} st */
function stateCell(st) {
  const td = document.createElement("td");
  td.className = `state ${st.tone}`;
  td.append(cell("span", st.label));
  return td;
}

function render() {
  if (!fleet) return;
  const h = fleet.host;
  el("host").textContent =
    `${h.name} · cpu ${h.cpu_pct}% · ram ${h.ram_used_mb}/${h.ram_total_mb} MB · disk ${h.disk_pct}% · ` +
    `${fleet.counts.online}/${fleet.counts.stacks} online, ${fleet.counts.parked} parked`;
  const rows = fleet.stacks.map((/** @type {any} */ s) => {
    const tr = document.createElement("tr");
    const st = stackState(s);
    tr.append(
      cell("td", String(s.vmid), "num"),
      cell("td", s.name),
      stateCell(st),
      cell("td", `${s.apps_running}/${s.apps_total}`, "num"),
      cell("td", String(s.restarts ?? 0), "num"),
      cell("td", ramText(s), "num"),
    );
    return tr;
  });
  el("stacks").replaceChildren(...rows);
  tickMeasured();
}

function tickMeasured() {
  if (fleet)
    el("measured").textContent = measuredAgo(
      fleet.measured_at,
      Date.now() / 1000,
    );
}

/** @param {boolean} up @param {string} [text] */
function setLink(up, text) {
  const l = el("link");
  l.textContent = text ?? (up ? "host link up" : "host link down");
  l.dataset.up = String(up);
}

async function load() {
  const r = await fetch("/data/fleet", {
    headers: { accept: "application/json" },
  });
  if (!r.ok) {
    setLink(false, `could not read the fleet (${r.status})`);
    return;
  }
  const body = await r.json();
  fleet = body.fleet;
  if (body.link_error) setLink(false, `host link down: ${body.link_error}`);
  else if (body.host_version) setLink(true, `host ${body.host_version}`);
  render();
}

const events = new EventSource("/events");
events.addEventListener("fleet", (e) => {
  fleet = JSON.parse(/** @type {MessageEvent} */ (e).data).fleet;
  render();
});
events.addEventListener("link", (e) => {
  const d = JSON.parse(/** @type {MessageEvent} */ (e).data);
  setLink(d.up, d.up ? `host ${d.host_version}` : `host link down: ${d.error}`);
});
events.addEventListener("resync", () => void load());

setInterval(tickMeasured, 1000);
void load();
