// kp-themes js/effects.js: the pointer reveals a theme draws (kp-themes 7.2.0).
export interface EffectsHandle {
  detach(): void;
}
export function attachEffects(
  root?: Document | Element,
  options?: Record<string, unknown>,
): EffectsHandle;
