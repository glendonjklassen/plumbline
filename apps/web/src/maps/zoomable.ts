// Pinch-zoom + pan for the map canvases (the Compose shells' zoomable
// pattern): scale clamped to [1, 4], offset bounded so the content can't be
// flung off-screen, pinned at 1×. Desktop keeps wheel+drag when zoomed.

export interface ZoomState {
  scale: number;
  x: number;
  y: number;
}

export function zoomable(node: HTMLElement, onChange: (z: ZoomState) => void) {
  const z: ZoomState = { scale: 1, x: 0, y: 0 };
  const pointers = new Map<number, { x: number; y: number }>();
  let lastDist = 0;

  const clamp = () => {
    z.scale = Math.min(Math.max(z.scale, 1), 4);
    const maxX = (z.scale - 1) * node.clientWidth;
    const maxY = (z.scale - 1) * node.clientHeight;
    z.x = Math.min(Math.max(z.x, -maxX), 0);
    z.y = Math.min(Math.max(z.y, -maxY), 0);
    if (z.scale === 1) {
      z.x = 0;
      z.y = 0;
    }
    onChange({ ...z });
  };

  const down = (e: PointerEvent) => {
    pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (pointers.size === 2) {
      const [a, b] = [...pointers.values()];
      lastDist = Math.hypot(a.x - b.x, a.y - b.y);
    }
  };
  const move = (e: PointerEvent) => {
    const prev = pointers.get(e.pointerId);
    if (!prev) return;
    const cur = { x: e.clientX, y: e.clientY };
    if (pointers.size === 2) {
      pointers.set(e.pointerId, cur);
      const [a, b] = [...pointers.values()];
      const dist = Math.hypot(a.x - b.x, a.y - b.y);
      if (lastDist > 0) {
        const rect = node.getBoundingClientRect();
        const cx = (a.x + b.x) / 2 - rect.left;
        const cy = (a.y + b.y) / 2 - rect.top;
        const factor = dist / lastDist;
        z.x = cx - (cx - z.x) * factor;
        z.y = cy - (cy - z.y) * factor;
        z.scale *= factor;
        clamp();
      }
      lastDist = dist;
    } else if (pointers.size === 1 && z.scale > 1) {
      z.x += cur.x - prev.x;
      z.y += cur.y - prev.y;
      pointers.set(e.pointerId, cur);
      clamp();
    }
  };
  const up = (e: PointerEvent) => {
    pointers.delete(e.pointerId);
    lastDist = 0;
  };
  const wheel = (e: WheelEvent) => {
    if (!e.ctrlKey) return;
    e.preventDefault();
    const rect = node.getBoundingClientRect();
    const cx = e.clientX - rect.left;
    const cy = e.clientY - rect.top;
    const factor = e.deltaY < 0 ? 1.15 : 1 / 1.15;
    z.x = cx - (cx - z.x) * factor;
    z.y = cy - (cy - z.y) * factor;
    z.scale *= factor;
    clamp();
  };

  node.addEventListener("pointerdown", down);
  node.addEventListener("pointermove", move);
  node.addEventListener("pointerup", up);
  node.addEventListener("pointercancel", up);
  node.addEventListener("wheel", wheel, { passive: false });
  return {
    destroy() {
      node.removeEventListener("pointerdown", down);
      node.removeEventListener("pointermove", move);
      node.removeEventListener("pointerup", up);
      node.removeEventListener("pointercancel", up);
      node.removeEventListener("wheel", wheel);
    },
  };
}
