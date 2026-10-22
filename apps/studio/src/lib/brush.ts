// Brush strokes of the editor: points smoothed into quadratic curves through their midpoints, kept as data so a stroke
// can be drawn live (one segment per new point) and replayed the same way for undo/redo.

export type Tool = 'mask' | 'draw' | 'erase';

export interface Pt {
  x: number;
  y: number;
}

/** Quadratic curve from `from` to `to`, bent toward `ctrl`. */
export interface Seg {
  from: Pt;
  ctrl: Pt;
  to: Pt;
}

export interface Stroke {
  tool: Tool;
  color: string;
  /** Diameter in source pixels. */
  width: number;
  points: Pt[];
}

/** Points closer than this (source pixels) add nothing but jitter. */
export const MIN_STEP = 0.75;

const mid = (a: Pt, b: Pt): Pt => ({ x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 });

/** Adds `p` unless it is within MIN_STEP of the last point; true when added. */
export function addPoint(points: Pt[], p: Pt): boolean {
  const last = points[points.length - 1];
  if (last && Math.hypot(p.x - last.x, p.y - last.y) < MIN_STEP) return false;
  points.push(p);
  return true;
}

/** The segment the newest point completes while drawing; the stroke's last piece waits for `endSegment`. */
export function newestSegment(points: Pt[]): Seg | null {
  const n = points.length;
  if (n === 1) return { from: points[0], ctrl: points[0], to: points[0] };
  if (n === 2) return { from: points[0], ctrl: points[0], to: mid(points[0], points[1]) };
  return { from: mid(points[n - 3], points[n - 2]), ctrl: points[n - 2], to: mid(points[n - 2], points[n - 1]) };
}

/** From the last midpoint to the last point, drawn on release. */
export function endSegment(points: Pt[]): Seg | null {
  const n = points.length;
  if (n < 2) return null;
  return { from: mid(points[n - 2], points[n - 1]), ctrl: points[n - 1], to: points[n - 1] };
}

/** Every segment of a finished stroke: the same pieces live drawing produced, in order. */
export function strokeSegments(points: Pt[]): Seg[] {
  const out: Seg[] = [];
  for (let i = 1; i <= points.length; i++) {
    const seg = newestSegment(points.slice(0, i));
    if (seg) out.push(seg);
  }
  const end = endSegment(points);
  if (end) out.push(end);
  return out;
}

/** Mask layer colour (shown at half opacity). */
export const MASK_COLOR = '#ff4d6d';

/** Draws segments with round ends onto the layer a stroke belongs to (erase clears). */
export function paintSegments(ctx: CanvasRenderingContext2D, stroke: Stroke, segments: Seg[]): void {
  ctx.globalCompositeOperation = stroke.tool === 'erase' ? 'destination-out' : 'source-over';
  ctx.strokeStyle = stroke.tool === 'mask' ? MASK_COLOR : stroke.color;
  ctx.lineWidth = stroke.width;
  ctx.lineCap = 'round';
  ctx.lineJoin = 'round';
  ctx.beginPath();
  for (const s of segments) {
    ctx.moveTo(s.from.x, s.from.y);
    // A dot (all three points equal) still needs a visible round cap.
    if (s.from.x === s.to.x && s.from.y === s.to.y) ctx.lineTo(s.to.x + 0.01, s.to.y);
    else ctx.quadraticCurveTo(s.ctrl.x, s.ctrl.y, s.to.x, s.to.y);
  }
  ctx.stroke();
}
