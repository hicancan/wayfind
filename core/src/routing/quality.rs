//! Candidate validation, using original travel times, never penalty costs.
use std::collections::HashSet;

use rstar::{RTree, RTreeObject, AABB};

use super::*;

const LOCAL_FRACTION: f64 = 0.25;
const LOCAL_TOLERANCE: f64 = 1.1;
const REVERSAL_STRETCH_M: f64 = 30.0;
const RETURN_RADIUS_M: f64 = 15.0;
const RETURN_TRAVEL_M: f64 = 60.0;
const EPS: f64 = 1e-6;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Rejection {
    Stretch,
    Sharing,
    PublicRoad,
    RepeatedNode,
    Reversal,
    NearReturn,
    LocalOptimality,
}

#[derive(Clone, Copy)]
struct Sample {
    edge: usize,
    fraction: f64,
    cost: f64,
}

impl Graph {
    pub(super) fn alternative_quality(
        &self,
        path: &[Traversal],
        route: &Route,
        request: &RouteRequest,
        best_duration: f64,
    ) -> Option<Rejection> {
        self.no_uturn(path, route).or_else(|| {
            (!self.locally_optimal(path, request, best_duration))
                .then_some(Rejection::LocalOptimality)
        })
    }

    fn no_uturn(&self, path: &[Traversal], route: &Route) -> Option<Rejection> {
        let mut visited = HashSet::new();
        let mut first = true;
        for t in path.iter().filter(|t| t.from != t.to) {
            let e = &self.edges[t.edge];
            let node = |f| {
                if f == 0.0 {
                    Some(e.a)
                } else if f == 1.0 {
                    Some(e.b)
                } else {
                    None
                }
            };
            if first {
                if let Some(n) = node(t.from) {
                    visited.insert(n);
                }
                first = false;
            }
            if let Some(n) = node(t.to) {
                if !visited.insert(n) {
                    return Some(Rejection::RepeatedNode);
                }
            }
        }
        // Compare arms separated by up to 30 m, including shape vertices and
        // short cross-links. Checking just adjacent turns misses two 90° turns.
        let arms: Vec<_> = route
            .geometry
            .windows(2)
            .enumerate()
            .filter(|(_, p)| geo::haversine(p[0], p[1]) > EPS)
            .map(|(i, p)| (i, geo::bearing(p[0], p[1])))
            .collect();
        for (k, &(i, after)) in arms.iter().enumerate() {
            for &(j, before) in arms[..k].iter().rev() {
                if route.geometry_offsets_m[i] - route.geometry_offsets_m[j + 1]
                    > REVERSAL_STRETCH_M + EPS
                {
                    break;
                }
                if geo::angle(before, after).abs() > 150.0 + EPS {
                    return Some(Rejection::Reversal);
                }
            }
        }
        near_return(route).then_some(Rejection::NearReturn)
    }

    // Every traversal boundary plus interior samples at intervals <= T/8.
    // All sampled endpoint pairs with cost <= T are checked, not just length-T
    // windows: a tiny detour can hide inside a tolerable longer window.
    fn local_samples(&self, path: &[Traversal], mode: Mode, limit: f64) -> Vec<Sample> {
        let mut samples = Vec::new();
        let mut cost = 0.0;
        for t in path {
            let duration = t.distance(self) / self.edges[t.edge].speed(mode);
            samples.push(Sample {
                edge: t.edge,
                fraction: t.from,
                cost,
            });
            let parts = (duration / (limit / 8.0)).ceil().max(1.0) as usize;
            for part in 1..=parts {
                let f = part as f64 / parts as f64;
                samples.push(Sample {
                    edge: t.edge,
                    fraction: t.from + (t.to - t.from) * f,
                    cost: cost + duration * f,
                });
            }
            cost += duration;
        }
        samples
    }

    fn locally_optimal(
        &self,
        path: &[Traversal],
        request: &RouteRequest,
        best_duration: f64,
    ) -> bool {
        let limit = best_duration * LOCAL_FRACTION;
        if limit <= EPS {
            return true;
        }
        let samples = self.local_samples(path, request.mode, limit);
        for (i, start) in samples.iter().enumerate() {
            let ends: Vec<_> = samples[i + 1..]
                .iter()
                .take_while(|end| end.cost - start.cost <= limit + EPS)
                .filter(|end| end.cost - start.cost > EPS)
                .collect();
            let Some(last) = ends.last() else {
                continue;
            };
            // One bounded Dijkstra serves every window from this start. Only a
            // shortcut below window/1.1 can invalidate it; no parents needed.
            let bound = (last.cost - start.cost) / LOCAL_TOLERANCE;
            let distances = self.local_distances(*start, request, bound);
            for end in ends {
                let e = &self.edges[end.edge];
                let seconds = e.metres / e.speed(request.mode);
                let mut shortest = (distances[e.a] + end.fraction * seconds)
                    .min(distances[e.b] + (1.0 - end.fraction) * seconds);
                if start.edge == end.edge {
                    shortest = shortest.min((end.fraction - start.fraction).abs() * seconds);
                }
                if end.cost - start.cost > shortest * LOCAL_TOLERANCE + EPS {
                    return false;
                }
            }
        }
        true
    }

    fn local_distances(&self, start: Sample, request: &RouteRequest, bound: f64) -> Vec<f64> {
        let mut distance = vec![f64::INFINITY; self.nodes.len()];
        let mut queue = BinaryHeap::new();
        let edge = &self.edges[start.edge];
        let seconds = edge.metres / edge.speed(request.mode);
        for (node, cost) in [
            (edge.a, start.fraction * seconds),
            (edge.b, (1.0 - start.fraction) * seconds),
        ] {
            if cost <= bound && cost < distance[node] {
                distance[node] = cost;
                queue.push(Entry {
                    node,
                    cost,
                    estimate: cost,
                });
            }
        }
        while let Some(entry) = queue.pop() {
            if entry.cost > distance[entry.node] {
                continue;
            }
            for &(next, id) in &self.adjacency[entry.node] {
                let e = &self.edges[id];
                if !e.usable(request.mode, request.avoid_steps) {
                    continue;
                }
                let cost = entry.cost + e.metres / e.speed(request.mode);
                if cost <= bound && cost < distance[next] {
                    distance[next] = cost;
                    queue.push(Entry {
                        node: next,
                        cost,
                        estimate: cost,
                    });
                }
            }
        }
        distance
    }
}

struct SpatialSegment {
    a: [f64; 3],
    b: [f64; 3],
    start: f64,
    end: f64,
}

impl RTreeObject for SpatialSegment {
    type Envelope = AABB<[f64; 3]>;
    fn envelope(&self) -> Self::Envelope {
        AABB::from_corners(
            std::array::from_fn(|i| self.a[i].min(self.b[i]) - RETURN_RADIUS_M),
            std::array::from_fn(|i| self.a[i].max(self.b[i]) + RETURN_RADIUS_M),
        )
    }
}

fn near_return(route: &Route) -> bool {
    if route.distance_m <= RETURN_TRAVEL_M {
        return false;
    }
    let mut index = RTree::<SpatialSegment>::new();
    for (i, points) in route.geometry.windows(2).enumerate() {
        // Approximate spherical arcs with <= 50 m chords (sag < 0.00005 m).
        // Continuous segment comparisons catch returns between sparse vertices.
        let parts = (geo::haversine(points[0], points[1]) / 50.0)
            .ceil()
            .max(1.0) as usize;
        for part in 0..parts {
            let from = part as f64 / parts as f64;
            let to = (part + 1) as f64 / parts as f64;
            let offset = |f| {
                route.geometry_offsets_m[i]
                    + f * (route.geometry_offsets_m[i + 1] - route.geometry_offsets_m[i])
            };
            let segment = SpatialSegment {
                a: geo::xyz(geo::interpolate(points[0], points[1], from)),
                b: geo::xyz(geo::interpolate(points[0], points[1], to)),
                start: offset(from),
                end: offset(to),
            };
            if segment.end - segment.start > RETURN_TRAVEL_M + EPS
                && segments_return(&segment, &segment)
            {
                return true;
            }
            if index
                .locate_in_envelope_intersecting(&segment.envelope())
                .any(|old| {
                    segment.end - old.start > RETURN_TRAVEL_M + EPS
                        && segments_return(old, &segment)
                })
            {
                return true;
            }
            index.insert(segment);
        }
    }
    false
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}

// Minimize squared chord distance over (u,v) in [0,1]² with travel separation
// >60 m. This clips the square to a convex polygon, then checks its boundary
// and the unconstrained quadratic minimum. No vertex-only proximity loophole.
fn segments_return(old: &SpatialSegment, new: &SpatialSegment) -> bool {
    let (a, b, d) = (sub(old.b, old.a), sub(new.a, new.b), sub(old.a, new.a));
    let separation = |p: [f64; 2]| {
        new.start + p[1] * (new.end - new.start)
            - old.start
            - p[0] * (old.end - old.start)
            - RETURN_TRAVEL_M
            - EPS
    };
    let square = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mut polygon = Vec::new();
    for i in 0..4 {
        let (p, q) = (square[i], square[(i + 1) % 4]);
        let (sp, sq) = (separation(p), separation(q));
        if sp >= 0.0 {
            polygon.push(p);
        }
        if (sp >= 0.0) != (sq >= 0.0) {
            let t = sp / (sp - sq);
            polygon.push([p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1])]);
        }
    }
    let delta = |p: [f64; 2]| std::array::from_fn(|i| d[i] + p[0] * a[i] + p[1] * b[i]);
    // Tiny conservative allowance covers chord sag and floating-point error.
    let close = |p| dot(delta(p), delta(p)) <= (RETURN_RADIUS_M + 0.0001).powi(2);
    let (aa, ab, bb, ad, bd) = (dot(a, a), dot(a, b), dot(b, b), dot(a, d), dot(b, d));
    let determinant = aa * bb - ab * ab;
    if determinant > 1e-12 {
        let p = [
            (ab * bd - bb * ad) / determinant,
            (ab * ad - aa * bd) / determinant,
        ];
        if p.iter().all(|x| (0.0..=1.0).contains(x)) && separation(p) >= 0.0 && close(p) {
            return true;
        }
    }
    for i in 0..polygon.len() {
        let (p, q) = (polygon[i], polygon[(i + 1) % polygon.len()]);
        let direction = sub(delta(q), delta(p));
        let squared = dot(direction, direction);
        let t = if squared > 0.0 {
            (-dot(delta(p), direction) / squared).clamp(0.0, 1.0)
        } else {
            0.0
        };
        if close([p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1])]) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests;
