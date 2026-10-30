import { readFile } from 'node:fs/promises';
import { beforeAll, describe, expect, it } from 'vitest';
import { initialize, Navigator, Router, type GraphData, type RouteRequest } from './index.js';

const graph: GraphData = {
  format: 'wayfind-graph/1', nodes: [[0, 0], [0.001, 0], [0.001, 0.001]],
  edges: [[0, 1, 111, 3, 0], [1, 2, 111, 3, 1]], shapes: [[], []],
  names: ['甲路', '乙路'], flags: { walk: 1, bike: 2, steps: 4, connector: 8 },
  targets: { start: [0], end: [2] },
};
const request: RouteRequest = { origin: { type: 'target', id: 'start' }, destination: { type: 'target', id: 'end' } };
beforeAll(async () => {
  const bytes = await readFile(new URL('../wasm/wayfind_wasm_bg.wasm', import.meta.url));
  await Promise.all([initialize(bytes), initialize(bytes)]);
});

describe('real WASM typed API', () => {
  it('rejects a parallel-road U-turn alternative and returns fewer routes', () => {
    const router = new Router({ ...graph,
      nodes: [[-200, 0], [-5, 0], [-5, -45], [5, -45], [5, 0], [200, 0]]
        .map(([x, y]) => [x / 111195.0802335329, y / 111195.0802335329] as [number, number]),
      edges: [[0, 5, 400, 3, -1], [0, 1, 195, 3, -1], [1, 2, 45, 3, -1],
        [2, 3, 10, 3, -1], [3, 4, 45, 3, -1], [4, 5, 195, 3, -1]],
      shapes: [[], [], [], [], [], []], targets: { start: [0], end: [5] },
    });
    try {
      const { routes } = router.route({ ...request, alternatives: 3 });
      expect(routes).toHaveLength(1);
      expect(routes[0].distance_m).toBe(400);
    } finally { router.free(); }
  });

  it('keeps three locally optimal alternatives on a grid', () => {
    const nodes: GraphData['nodes'] = [];
    const edges: GraphData['edges'] = [];
    for (let y = 0; y < 5; y++) {
      for (let x = 0; x < 5; x++) {
        const n = y * 5 + x;
        nodes.push([x * 100 / 111195.0802335329, y * 100 / 111195.0802335329]);
        if (x < 4) edges.push([n, n + 1, 100, 3, -1]);
        if (y < 4) edges.push([n, n + 5, 100, 3, -1]);
      }
    }
    const router = new Router({ ...graph, nodes, edges, shapes: edges.map(() => []),
      targets: { start: [0], end: [24] } });
    try {
      const { routes } = router.route({ ...request, alternatives: 3 });
      expect(routes).toHaveLength(3);
      expect(routes.map(r => r.distance_m)).toEqual([800, 800, 800]);
    } finally { router.free(); }
  });

  it('routes, returns shaped geometry and core Chinese instructions', () => {
    const router = new Router(graph);
    try {
      const route = router.route(request).routes[0];
      expect(route.distance_m).toBe(222);
      expect(route.duration_s).toBeCloseTo(177.6);
      expect(route.geometry).toEqual(graph.nodes);
      expect(route.maneuvers.map(m => m.type)).toEqual(['depart', 'turn', 'arrive']);
      expect(route.maneuvers[1].modifier).toBe('left');
      expect(route.maneuvers[1].instruction).toBe('左转进入乙路，前行 100 米到达');
      const nav = new Navigator(route);
      try {
        expect(nav.update(0, 0, 5, null, 0).voice_prompts[0].stage).toBe('depart');
        expect(nav.update(0.001, 0.001, 5, undefined, 180_000).arrived).toBe(true);
        expect(nav.update(0.001, 0.001, 5, 0, 181_000).voice_prompts).toEqual([]);
      } finally { nav.free(); }
    } finally { router.free(); }
  });

  it('propagates graph, request, snapping and navigation errors as exceptions', () => {
    expect(() => new Router('{}')).toThrow();
    expect(() => new Router(JSON.stringify({ ...graph, format: 'wayfind-graph/2' }))).toThrow();
    expect(() => new Router({ ...graph, landmarks: [{ name: '', at: [0, 0] }] })).toThrow();
    expect(() => new Router({ ...graph, landmarks: [{ name: '甲广场', at: [0, 91] }] })).toThrow();
    const router = new Router(graph);
    try {
      expect(() => router.route({ ...request, destination: { type: 'target', id: 'missing' } })).toThrow();
      expect(() => router.route({ ...request, origin: { type: 'position', position: [1, 1] } })).toThrow();
      const nav = new Navigator(router.route(request).routes[0]);
      try { expect(() => nav.update(NaN, 0, 5, null, 0)).toThrow(); }
      finally { nav.free(); }
      expect(() => new Navigator('{}')).toThrow();
    } finally { router.free(); }
  });

  it('round-trips landmark semantics, target names and the voice text family through WASM', () => {
    const router = new Router({ ...graph, landmarks: [{ name: '甲广场', at: [0.001, -0.0001] }] });
    try {
      const route = router.route(request).routes[0];
      expect(route.maneuvers[0].instruction).toBe('从start出发，沿甲路向东走 100 米');
      expect(route.maneuvers[1].landmark).toEqual({ name: '甲广场', at: [0.001, -0.0001], relation: 'north' });
      expect(route.maneuvers[1].instruction).toBe('在甲广场北侧左转进入乙路，前行 100 米到达');
      const nav = new Navigator(route);
      try {
        expect(nav.update(0.00015, 0, 1, null, 0).voice_prompts[0].stage).toBe('depart');
        const advance = nav.update(0.0003, 0, 1, null, 12_000).voice_prompts;
        expect(advance[0].text).toBe('80 米后在甲广场北侧左转进入乙路');
        const arrival = nav.update(0.001, 0.001, 1, null, 200_000).voice_prompts;
        expect(arrival[0].text).toBe('已到达end附近，入口在您的前方');
      } finally { nav.free(); }
    } finally { router.free(); }
  });

  it('uses position projections, supports bike mode and ignores graph extensions', () => {
    const router = new Router(JSON.stringify({ ...graph, extension: { version: 10 } }));
    try {
      const route = router.route({ origin: { type: 'position', position: [0.0002, 0.00005] },
        destination: { type: 'position', position: [0.0008, 0.00005] }, mode: 'bike', alternatives: 3 }).routes[0];
      expect(route.distance_m).toBeCloseTo(77.72, 1);
      expect(route.geometry).toHaveLength(4);
      expect(route.duration_s).toBeLessThan(route.distance_m / 1.25);
      expect(route.maneuvers.at(-1)?.modifier).toBe('left');
    } finally { router.free(); }
  });
});
