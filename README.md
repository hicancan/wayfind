# wayfind

A place-independent Rust kernel for walking and cycling on small networks.
Load a graph, route between positions or multi-entrance targets, then feed GPS
fixes into a navigation session. All semantics and Simplified Chinese guidance
live in `core`; WASM, Kotlin and TypeScript share the same JSON protocol.

```text
core/     graph validation, spatial index, A*, alternatives, guidance, navigation
wasm/     wasm-bindgen Router and Navigator
android/  UniFFI 0.32 Router and Navigator, Kotlin/native build
js/       typed ESM npm package and real WASM tests
```

Only `wayfind-graph/1` JSON is accepted. Unknown top-level fields are ignored.
Coordinates are `[longitude, latitude]` in WGS84; edges are undirected and use
the supplied nonnegative metre lengths. Shapes contain each edge's interior
points in a → b order. A name index of `-1` means unnamed.

```json
{
  "format": "wayfind-graph/1",
  "nodes": [[0, 0], [0.001, 0]],
  "edges": [[0, 1, 111, 3, 0]],
  "shapes": [[]],
  "names": ["甲路"],
  "flags": {"walk": 1, "bike": 2, "steps": 4, "connector": 8},
  "targets": {"start": [0], "end": [1]}
}
```

Flags grant walk/bike access; bike always excludes stairs. `avoid_steps` also
excludes them for walkers. Time costs use 1.25 m/s walking, 4.2 m/s cycling,
0.6 m/s on stairs, and 1.25 m/s on connectors in either mode. Positions snap
to usable edges using a spherical R-tree index. Off-network connectors to the
true endpoints are included in route geometry, distance and time.

```ts
import { initialize, Router, Navigator } from 'wayfind';
await initialize(); // Node: pass WASM bytes read with fs.readFile instead
const router = new Router(graphJson);
const { routes } = router.route({
  origin: { type: 'position', position: [0.0001, 0] },
  destination: { type: 'target', id: 'end' },
  mode: 'walk', avoid_steps: false, max_snap_distance_m: 100, alternatives: 3,
});
const navigator = new Navigator(routes[0]);
const update = navigator.update(0.0001, 0, 5, null, 0);
console.log(update.next, update.voice_prompts, update.off_route, update.arrived);
navigator.free();
router.free();
```

`alternatives` is the total requested count (1–3, default 1). Penalty searches
return fewer routes if necessary: each must be at most 40% slower than the best
and share at most 75% of the shorter network length with any earlier route.
Sixteen bounded searches are attempted; this is not exhaustive enumeration.
Route metrics use declared lengths; `geometry_offsets_m` maps vertices to those
lengths. Maneuvers include bearings, offsets, next-maneuver distance and Chinese
instructions. Turn bands are 15°/45°/135°, sampled over approximately 10 m.

Navigation timestamps are monotonic, nonnegative safe-integer milliseconds;
heading may be null. Sessions tolerate jitter, constrain subsequent matching
to a forward window and keep progress monotonic. They announce at 100 m
(bike: 200 m), 30 m and near the maneuver, once per stage; missed stages are
coalesced. Off-route means over `max(20, 2 × accuracy)` metres away for three
consecutive fixes or eight seconds. Arrival latches within 15 m of the true
destination. ETA uses remaining planned segment times. Rerouting is the caller's
responsibility; sessions do not fuse sensors or infer stair elevation.

On Windows use PowerShell 7, Rust stable with `wasm32-unknown-unknown`,
`aarch64-linux-android` and `x86_64-linux-android`, wasm-pack, wasm-bindgen
0.2.122, cargo-ndk, Node, Android NDK and JDK. Run `pwsh ./build.ps1` to verify
everything, or individually:

```powershell
cargo test --workspace
cargo test --release
npm ci --prefix js
npm run build --prefix js
npm test --prefix js
pwsh android/build.ps1
```

Set `WAYFIND_GRAPH_FILE` to a local graph for the optional integration test;
unset or absent files skip it. It checks all target pairs in the main walking
component and times 1,000 reproducible random routes, including geometry and
maneuvers, with an 800 ms release ceiling. External data is never committed.

Android outputs are `android/build/jniLibs/{arm64-v8a,x86_64}` and
`android/build/kotlin`, package `top.hicancan.wayfind`. Override `-NdkPath`,
`-JdkPath` or `-OutputDirectory` as needed. Add the native libraries and generated
Kotlin to an app with JNA's Android AAR. Kotlin `Router(graphJson).route(requestJson)`
returns response JSON; `Navigator(routeJson).update(lon, lat, accuracyM, headingDeg,
timestampMs)` returns update JSON. Errors become exceptions on both platforms.

Licensed under **AGPL-3.0-only**; see [LICENSE](LICENSE).
