// Live view cursor (live-cursor, Kenny 2026-09-29): the pure path and
// timing of Claude's simulated pointer.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  EDGE,
  aimAt,
  bandScroll,
  clipBand,
  inBand,
  arriveLead,
  clickDue,
  glideFraction,
  pathAt,
  safeBand,
} from "../js/drivecursor.js";

test("the glide starts where the pointer was and lands before 0", () => {
  const total = 5000;
  const lead = arriveLead(total);
  assert.ok(lead > 0 && lead <= 600, `lead ${lead}`);
  assert.equal(glideFraction(total, total, total), 0);
  assert.equal(glideFraction(total, lead, total), 1);
  assert.equal(glideFraction(total, 0, total), 1);
  const mid = glideFraction(total, (total + lead) / 2, total);
  assert.ok(Math.abs(mid - 0.5) < 1e-9, `mid ${mid}`);
  // A tab that saw the announcement late still glides, over what is left.
  assert.equal(glideFraction(2000, 2000, total), 0);
  assert.equal(glideFraction(2000, lead, total), 1);
  // Too late to glide, or reduced motion: it jumps.
  assert.equal(glideFraction(lead, lead, total), 1);
  assert.equal(glideFraction(total, total, total, true), 1);
  // The countdown's duration comes from the announcement: 3 s works too.
  assert.equal(glideFraction(3000, arriveLead(3000), 3000), 1);
});

test("the path eases from start to target and aims inside the window", () => {
  const a = { x: 100, y: 500 };
  const b = { x: 700, y: 100 };
  assert.deepEqual(pathAt(a, b, 0), a);
  assert.deepEqual(pathAt(a, b, 1), b);
  assert.deepEqual(pathAt(a, b, 0.5), { x: 400, y: 300 });
  // Eased: slower than linear at the start.
  assert.ok(pathAt(a, b, 0.1).x - a.x < 60);
  // The centre of the target; one scrolled off screen is aimed at the edge.
  const box = { left: 40, top: 60, width: 100, height: 20 };
  assert.deepEqual(aimAt(box, 1400, 900), { x: 90, y: 70 });
  const below = { left: 40, top: 1500, width: 100, height: 20 };
  assert.deepEqual(aimAt(below, 390, 844), { x: 90, y: 844 - EDGE });
  // The click comes just before 0, never while paused.
  assert.equal(clickDue(1000, 5000, false), false);
  assert.equal(clickDue(100, 5000, false), true);
  assert.equal(clickDue(100, 5000, true), false);
});

test("the pointer is kept in a safe band above the announce bar", () => {
  const box = (/** @type {number} */ top, h = 30) => ({
    left: 100,
    top,
    width: 200,
    height: h,
  });
  // 800 px window, a 56 px sticky bar on top, a 90 px announce bar below.
  const band = safeBand(800, 56, 90);
  assert.deepEqual(band, { top: 80, bottom: 686 });
  // Well inside: no scroll.
  assert.equal(inBand(box(300), band), true);
  assert.equal(bandScroll(box(300), band), 0);
  // Visible in the window but under the announce bar (the 18:14 case):
  // scrolled down until its centre is the band's middle.
  assert.equal(inBand(box(700), band), false);
  assert.equal(bandScroll(box(700), band), 715 - 383);
  // Under the top bar, or above the window: scrolled up.
  assert.equal(bandScroll(box(40), band), 55 - 383);
  assert.equal(bandScroll(box(-400), band) < 0, true);
  // Far below the fold (fix-163's case) still scrolls.
  assert.equal(bandScroll(box(2400), band), 2415 - 383);
  // A target taller than half the band: its top a sixth of the way down.
  assert.equal(inBand(box(100, 2000), band), true);
  assert.equal(bandScroll(box(600, 2000), band), 600 - (80 + 101));
  // No bars: only the margin; a window too short gets the whole window.
  assert.deepEqual(safeBand(800, 0, 0), { top: 24, bottom: 776 });
  assert.deepEqual(safeBand(150, 56, 90), { top: 0, bottom: 150 });
});

test("inside the dialog the band is the overlap with its scrolling body", () => {
  const band = safeBand(800, 56, 0);
  // The dialog body shows 200..600 of the window.
  assert.deepEqual(clipBand(band, 200, 600), { top: 212, bottom: 588 });
  // A body wider than the band keeps the band.
  assert.deepEqual(clipBand(band, 0, 900), band);
  // A body too small for a band gets its own visible part.
  assert.deepEqual(clipBand(band, 700, 740), { top: 700, bottom: 740 });
  const inner = clipBand(band, 200, 600);
  const field = { left: 0, top: 580, width: 300, height: 36 };
  assert.equal(bandScroll(field, inner), 598 - 400);
});
