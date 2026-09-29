// Live view cursor (live-cursor, Kenny 2026-09-29): the pure path and
// timing of Claude's simulated pointer.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  EDGE,
  aimAt,
  arriveLead,
  clickDue,
  glideFraction,
  offScreen,
  pathAt,
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

test("fix-163: a target below the fold is scrolled to before the glide", () => {
  const box = (/** @type {number} */ top, h = 30) => ({
    left: 100,
    top,
    width: 200,
    height: h,
  });
  assert.equal(offScreen(box(200), 1280, 800), false);
  // Below the fold, above the top, or cut off by the bottom edge.
  assert.equal(offScreen(box(900), 1280, 800), true);
  assert.equal(offScreen(box(-60), 1280, 800), true);
  assert.equal(offScreen(box(790), 1280, 800), true);
  // A target taller than the window counts as on screen once its top is.
  assert.equal(offScreen(box(40, 2000), 1280, 800), false);
  // Off to the side.
  assert.equal(
    offScreen({ left: 1400, top: 200, width: 50, height: 20 }, 1280, 800),
    true,
  );
});
