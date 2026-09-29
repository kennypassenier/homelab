// The two kp-themes bar helpers the dashboard uses (kp-themes 7.2.0,
// js/components.js); each wires every matching bar once and returns detach.
export function attachNavMenus(
  root?: ParentNode,
  options?: { strings?: Record<string, string>; ownedBy?: string },
): () => void;
export function attachNavToggles(
  root?: ParentNode,
  options?: { strings?: Record<string, string>; ownedBy?: string },
): () => void;
