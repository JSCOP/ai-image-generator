import { describe, expect, it } from 'vitest';
import { addPoint, endSegment, newestSegment, strokeSegments, type Pt } from './brush';

describe('brush strokes', () => {
  it('drops points that add only jitter', () => {
    const points: Pt[] = [];
    expect(addPoint(points, { x: 0, y: 0 })).toBe(true);
    expect(addPoint(points, { x: 0.3, y: 0.2 })).toBe(false);
    expect(addPoint(points, { x: 2, y: 0 })).toBe(true);
    expect(points).toHaveLength(2);
  });

  it('curves through midpoints, so consecutive pieces meet without corners', () => {
    const points = [
      { x: 0, y: 0 },
      { x: 10, y: 0 },
      { x: 10, y: 10 },
    ];
    expect(newestSegment(points)).toEqual({ from: { x: 5, y: 0 }, ctrl: { x: 10, y: 0 }, to: { x: 10, y: 5 } });
    expect(endSegment(points)).toEqual({ from: { x: 10, y: 5 }, ctrl: { x: 10, y: 10 }, to: { x: 10, y: 10 } });
    const segs = strokeSegments(points);
    // dot, start, middle curve, end: each piece starts where the previous ended.
    expect(segs).toHaveLength(4);
    for (let i = 1; i < segs.length; i++) expect(segs[i].from).toEqual(segs[i - 1].to);
    expect(segs[segs.length - 1].to).toEqual({ x: 10, y: 10 });
  });

  it('a single click is a dot', () => {
    expect(strokeSegments([{ x: 3, y: 4 }])).toEqual([{ from: { x: 3, y: 4 }, ctrl: { x: 3, y: 4 }, to: { x: 3, y: 4 } }]);
  });
});
