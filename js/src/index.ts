import init, { Router as WasmRouter, Navigator as WasmNavigator, type InitInput } from '../wasm/wayfind_wasm.js';
import type { GraphData, RouteRequest, RouteResponse, Route, NavigationUpdate } from './types.js';
export type * from './types.js';

let initialization: Promise<void> | undefined;

/** Initialize once. Browsers may omit input; Node should pass the WASM bytes. */
export function initialize(input?: InitInput | Promise<InitInput>): Promise<void> {
  initialization ??= init({ module_or_path: input }).then(() => undefined).catch(error => {
    initialization = undefined;
    throw error;
  });
  return initialization;
}

/** Owns a WASM graph. Call free() when finished; initialize() must resolve first. */
export class Router {
  private readonly inner: WasmRouter;
  constructor(graph: GraphData | string) {
    this.inner = new WasmRouter(typeof graph === 'string' ? graph : JSON.stringify(graph));
  }
  route(request: RouteRequest): RouteResponse {
    return JSON.parse(this.inner.route(JSON.stringify(request))) as RouteResponse;
  }
  free(): void { this.inner.free(); }
}

/** A session owns its route, independent of the router that produced it. */
export class Navigator {
  private readonly inner: WasmNavigator;
  constructor(route: Route | string) {
    this.inner = new WasmNavigator(typeof route === 'string' ? route : JSON.stringify(route));
  }
  /** timestampMs is a monotonic, nonnegative safe integer. Heading is optional. */
  update(lon: number, lat: number, accuracyM: number, headingDeg: number | null | undefined, timestampMs: number): NavigationUpdate {
    return JSON.parse(this.inner.update(lon, lat, accuracyM, headingDeg ?? undefined, timestampMs)) as NavigationUpdate;
  }
  free(): void { this.inner.free(); }
}
