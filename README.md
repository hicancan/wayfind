# wayfind

A place-independent Rust kernel for walking and cycling on small networks.
Load a graph, route between positions or multi-entrance targets, then feed GPS
fixes into a navigation session. All semantics and Simplified Chinese guidance
live in `core`; WASM, Kotlin and TypeScript share the same JSON protocol.

```text
core/     graph validation, spatial index, A*, alternatives, guidance, navigation
wasm/     wasm-bindgen Router and Navigator
native/   UniFFI 0.32 Router and Navigator for Android (Kotlin) and Windows (C#)
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
  "flags": {"walk": 1, "bike": 2, "steps": 4, "connector": 8, "public": 16},
  "targets": {"start": [0], "end": [1]},
  "landmarks": [{"name": "甲广场", "at": [0.0005, 0.0001]}]
}
```

`landmarks` is optional (omitted means none). Each entry names a visible
building, square, gate or other feature at a valid WGS84 `[longitude, latitude]`.
Names must contain non-whitespace text; malformed entries, invalid coordinates
and `null` are errors. Graphs, target ids, road names and landmarks come from
callers; no place data is bundled.

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

`alternatives` is the total requested count (1–3, default 1). Following the
local-optimality, stretch and sharing criteria of [Abraham et al., *Alternative
routes in road networks*](https://doi.org/10.1145/2444016.2444019), penalty-search
candidates must pass all of these checks in `core`:

- At most 40% slower than the best route, using original travel times.
- At most 75% shared network length relative to the shorter path, against every
  earlier accepted route.
- Sampled local optimality: network windows costing up to T = 25% of the best
  route's travel time must cost at most 1.10 times their own shortest-path cost.
  Samples include all traversal boundaries and edge interiors at intervals no
  greater than T/8. Every sampled endpoint pair within T is tested with bounded
  Dijkstra, respecting mode and stair restrictions and using unpenalized costs.
  Off-network endpoint connectors contribute to the best time but are not
  themselves searchable network windows.
- No repeated graph node, bearing reversal greater than 150° between arms
  separated by up to 30 m of travel (including shapes and short cross-links), or
  return within 15 m of an earlier point after more than 60 m of travel.
  Proximity checks compare continuous geometry segments, including their
  interiors, with a 0.1 mm conservative numerical allowance.

The best route remains the unpenalized shortest path. Sixteen penalty searches
are attempted; this is not exhaustive enumeration. If fewer candidates pass,
fewer routes are returned. Local optimality is sampled, not a proof over every
continuous subpath; the geometric checks also reject detours where missing
graph links leave no shorter network path.

Route metrics use declared lengths; `geometry_offsets_m` maps vertices to those
lengths. Maneuvers include bearings, offsets, next-maneuver distance and Chinese
instructions. Turn bands are 15°/45°/135°, sampled over up to 10 m within each
adjoining segment. Departure names its target and initial compass direction;
arrival names its target and the entrance side, inferred from the final
connector's bearing (or ahead when no side can be inferred). Turn/continue
instructions include distance to the next decision; the final one says how far
to arrival. Spoken distances round to 10 m below 100 m, otherwise 50 m, and
never say zero metres. Exact distances remain in the numeric fields.

`route.via` describes the route consistently across clients. Accumulated lengths
include unnamed network segments. If a named way dominates, its name is used;
otherwise the landmark covering the greatest nearby unnamed route length is
used, weighted by proximity within 80 m and sampled every 10 m. Endpoint targets
are excluded. Ties are deterministic; without a nearby landmark it says 沿途道路.
Callers may name paths from sourced area outlines rather than nearby road names.

Turns choose the nearest landmark within 35 m. Equal distances prefer the
longer name, then lexical order, since the format carries no size or prominence
metadata. `maneuver.landmark` exposes its name, coordinates and relation:
`ahead`, `passed`, or `north`/`east`/`south`/`west`. Ahead/passed use the incoming
bearing; cardinal relations describe the path's side of the landmark (e.g.
`east` renders “在甲广场东侧”). Depart/arrive never choose a landmark.
`target_id` identifies a depart/arrive target, otherwise null.

Opposite turns on joins of at most 8 m cancel into continuing along the route
or a slight turn. Unnamed joins of up to 25 m between nearly parallel arms
receive the same treatment. Entrance spurs and short joins do not trigger
minor turns below 60°; genuine junctions retain their turns and forks.
Maneuver starts are at least 10 m apart except arrival. Close decisions that
cannot cancel are retained in `following_actions` (kind, modifier, exact route
offset, road name and landmark) and spoken together, including stairs. A turn
folded into departure excludes the origin itself from its landmark choices.
`leads_to_arrival` marks the last instruction before arrival.

Navigation timestamps are monotonic, nonnegative safe-integer milliseconds;
heading may be null. Sessions tolerate jitter, constrain subsequent matching
to a forward window and keep progress monotonic. They announce at 100 m
(bike: 200 m), 30 m and near the maneuver, once per stage; missed stages are
coalesced. Off-route means over `max(20, 2 × accuracy)` metres away for three
consecutive fixes or eight seconds. Arrival latches within 15 m of the true
destination. ETA uses remaining planned segment times. Rerouting is the caller's
responsibility; sessions do not fuse sensors or infer stair elevation.
Voice prompts use the same landmark and target semantics, e.g.
“100 米后在甲广场东侧右转”, “前方右转”, and “已到达end附近，入口在您的左侧”. Successive voice
prompts require at least 10 m of progress except the arrival announcement.

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
pwsh native/build.ps1
```

Set `WAYFIND_GRAPH_FILE` to a local graph for the optional integration test;
unset or absent files skip it. It checks all target pairs in the main walking
component and times 1,000 reproducible random routes, including geometry and
maneuvers, with an 800 ms release ceiling. External data is never committed.
The same configuration enables a 300-pair reproducible alternatives audit,
requesting three routes per pair. Every returned alternative is checked for
U-turns/returns and local optimality; an unrestricted A* independently verifies
the sampled window costs. With `-- --nocapture`, the audit prints the number of
returned alternatives, requests returning one/two/three routes, and unique
rejected candidates per request by first failing rule (stretch, sharing,
repeated node, reversal, near-return, local optimality). Repeated penalty
attempts at the same candidate count once.
For optional guidance regression checks, also set `WAYFIND_SAMPLE_ORIGIN` to a
target id and `WAYFIND_SAMPLE_DESTINATIONS` to a JSON array of destination ids.
The walking routes must have at most 10 maneuvers, spaced by at least 10 m
except arrival, and a distance in every turn instruction. Unset configuration
or an absent graph skips this check. For human review of any three routes:

```powershell
cargo run --locked --release -p wayfind-core --example guidance -- $env:WAYFIND_GRAPH_FILE start end-a end-b end-c
```

The example prints readable instruction lists and route offsets. Add `--json`
before the graph path for full response JSON with unrounded metrics.

Android outputs are `native/build/jniLibs/{arm64-v8a,x86_64}`, the Windows library `native/build/windows/wayfind_native.dll`, and
`native/build/kotlin`, package `top.hicancan.wayfind`. Override `-NdkPath`,
`-JdkPath` or `-OutputDirectory` as needed. Add the native libraries and generated
Kotlin to an app with JNA's Android AAR. Kotlin `Router(graphJson).route(requestJson)`
returns response JSON; `Navigator(routeJson).update(lon, lat, accuracyM, headingDeg,
timestampMs)` returns update JSON. Errors become exceptions on both platforms.

Licensed under **AGPL-3.0-only**; see [LICENSE](LICENSE).
