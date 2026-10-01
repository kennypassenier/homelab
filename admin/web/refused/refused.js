// The refusal page's alarm (Kenny, 2026-09-28: "Gemiste kans … het Alarm
// component niet te gebruiken bij de weigering"). The guard answers 403 with
// a page whose <main id="refusal"> carries the three lines as data; this
// raises kp-themes' Alarm with them. Served at `/refused.js`
// (`access::REFUSAL_SCRIPT`), before any lock, like `/static/`.

import { showAlarm } from "/static/kp/js/alarm.js";

const main = document.getElementById("refusal");
if (main) {
  const { code = "", title = "Access refused", detail = "" } = main.dataset;
  void showAlarm({
    title,
    detail,
    code,
    mode: "ack",
    action: "Try again",
  }).then(() => {
    location.reload();
  });
}
