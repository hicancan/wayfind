use std::{
    cmp::Ordering,
    collections::{BTreeMap, BinaryHeap},
};

use crate::{
    geo, graph::STEPS, maneuvers, Endpoint, Error, Graph, Mode, Position, Result, Route,
    RouteRequest, RouteResponse, RouteSegment, Snap,
};

#[derive(Clone, Debug)]
pub(crate) struct Traversal {
    pub edge: usize,
    pub from: f64,
    pub to: f64,
}

impl Traversal {
    fn distance(&self, graph: &Graph) -> f64 {
        (self.to - self.from).abs() * graph.edges[self.edge].metres
    }
    fn cost(&self, graph: &Graph, mode: Mode, penalties: &[f64]) -> f64 {
        self.distance(graph) / graph.edges[self.edge].speed(mode) * penalties[self.edge]
    }
}

struct Resolved {
    true_position: Option<Position>,
    snap: Option<Snap>,
    nodes: Vec<usize>,
}

struct Terminal {
    node: usize,
    traversal: Option<Traversal>,
    cost: f64,
}

#[derive(Clone)]
struct Entry {
    node: usize,
    cost: f64,
    estimate: f64,
}
impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.estimate == other.estimate && self.node == other.node
    }
}
impl Eq for Entry {}
impl Ord for Entry {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .estimate
            .total_cmp(&self.estimate)
            .then_with(|| other.node.cmp(&self.node))
    }
}
impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Graph {
    fn resolve(&self, endpoint: &Endpoint, request: &RouteRequest) -> Result<Resolved> {
        match endpoint {
            Endpoint::Target { id } => Ok(Resolved {
                true_position: None,
                snap: None,
                nodes: self
                    .targets
                    .get(id)
                    .ok_or_else(|| Error::UnknownTarget(id.clone()))?
                    .clone(),
            }),
            Endpoint::Position { position } => Ok(Resolved {
                true_position: Some(*position),
                nodes: vec![],
                snap: Some(self.snap(
                    *position,
                    request.mode,
                    request.avoid_steps,
                    request.max_snap_distance_m,
                )?),
            }),
        }
    }

    fn terminals(
        &self,
        endpoint: &Resolved,
        origin: bool,
        mode: Mode,
        penalties: &[f64],
    ) -> Vec<Terminal> {
        if let Some(snap) = &endpoint.snap {
            let edge = &self.edges[snap.edge];
            [(edge.a, 0.0), (edge.b, 1.0)]
                .into_iter()
                .map(|(node, fraction)| {
                    let traversal = if origin {
                        Traversal {
                            edge: snap.edge,
                            from: snap.fraction,
                            to: fraction,
                        }
                    } else {
                        Traversal {
                            edge: snap.edge,
                            from: fraction,
                            to: snap.fraction,
                        }
                    };
                    let cost = traversal.cost(self, mode, penalties) + snap.distance_m / 1.25;
                    Terminal {
                        node,
                        traversal: Some(traversal),
                        cost,
                    }
                })
                .collect()
        } else {
            endpoint
                .nodes
                .iter()
                .map(|node| Terminal {
                    node: *node,
                    traversal: None,
                    cost: 0.0,
                })
                .collect()
        }
    }

    fn search(
        &self,
        origin: &Resolved,
        destination: &Resolved,
        request: &RouteRequest,
        penalties: &[f64],
    ) -> Result<Vec<Traversal>> {
        let starts = self.terminals(origin, true, request.mode, penalties);
        let goals = self.terminals(destination, false, request.mode, penalties);
        let speed = if request.mode == Mode::Bike {
            4.2
        } else {
            1.25
        };
        let heuristic = |node: usize| {
            goals
                .iter()
                .map(|goal| {
                    geo::haversine(self.nodes[node], self.nodes[goal.node]) * self.heuristic_scale
                        / speed
                        + goal.cost
                })
                .fold(f64::INFINITY, f64::min)
        };
        let mut best_cost = f64::INFINITY;
        let mut best_path = None;
        if let (Some(a), Some(b)) = (&origin.snap, &destination.snap) {
            if a.edge == b.edge {
                let path = Traversal {
                    edge: a.edge,
                    from: a.fraction,
                    to: b.fraction,
                };
                best_cost =
                    path.cost(self, request.mode, penalties) + (a.distance_m + b.distance_m) / 1.25;
                best_path = Some(vec![path]);
            }
        }
        let mut distance = vec![f64::INFINITY; self.nodes.len()];
        let mut parents: Vec<Option<(usize, Traversal)>> = vec![None; self.nodes.len()];
        let mut seeds: Vec<Option<Traversal>> = vec![None; self.nodes.len()];
        let mut queue = BinaryHeap::new();
        for start in starts {
            if start.cost < distance[start.node] {
                distance[start.node] = start.cost;
                seeds[start.node] = start.traversal;
                queue.push(Entry {
                    node: start.node,
                    cost: start.cost,
                    estimate: start.cost + heuristic(start.node),
                });
            }
        }
        while let Some(entry) = queue.pop() {
            if entry.estimate >= best_cost {
                break;
            }
            if entry.cost > distance[entry.node] {
                continue;
            }
            for goal in goals.iter().filter(|goal| goal.node == entry.node) {
                let cost = entry.cost + goal.cost;
                if cost < best_cost {
                    let mut path = Vec::new();
                    let mut at = entry.node;
                    while let Some((previous, traversal)) = &parents[at] {
                        path.push(traversal.clone());
                        at = *previous;
                    }
                    if let Some(seed) = &seeds[at] {
                        path.push(seed.clone());
                    }
                    path.reverse();
                    if let Some(last) = &goal.traversal {
                        path.push(last.clone());
                    }
                    best_cost = cost;
                    best_path = Some(path);
                }
            }
            for &(next, edge_id) in &self.adjacency[entry.node] {
                let edge = &self.edges[edge_id];
                if !edge.usable(request.mode, request.avoid_steps) {
                    continue;
                }
                let from = if edge.a == entry.node { 0.0 } else { 1.0 };
                let traversal = Traversal {
                    edge: edge_id,
                    from,
                    to: 1.0 - from,
                };
                let cost = entry.cost + traversal.cost(self, request.mode, penalties);
                if cost < distance[next] {
                    distance[next] = cost;
                    parents[next] = Some((entry.node, traversal));
                    queue.push(Entry {
                        node: next,
                        cost,
                        estimate: cost + heuristic(next),
                    });
                }
            }
        }
        best_path.ok_or(Error::NoRoute)
    }

    pub fn route(&self, request: &RouteRequest) -> Result<RouteResponse> {
        if !(1..=3).contains(&request.alternatives)
            || !request.max_snap_distance_m.is_finite()
            || request.max_snap_distance_m < 0.0
        {
            return Err(Error::Invalid(
                "alternatives must be 1..=3 and snap distance finite and nonnegative".into(),
            ));
        }
        let origin = self.resolve(&request.origin, request)?;
        let destination = self.resolve(&request.destination, request)?;
        let mut penalties = vec![1.0; self.edges.len()];
        let best = self.search(&origin, &destination, request, &penalties)?;
        let first = self.materialize(&best, &origin, &destination, request);
        let best_duration = first.duration_s;
        let mut routes = vec![first];
        let mut paths = vec![best.clone()];
        let mut last = best;
        // Bounded penalty searches keep requests predictable on dense graphs.
        for _ in 0..16 {
            if routes.len() >= request.alternatives as usize {
                break;
            }
            for traversal in &last {
                penalties[traversal.edge] += 1.0;
            }
            let candidate = self.search(&origin, &destination, request, &penalties)?;
            let route = self.materialize(&candidate, &origin, &destination, request);
            let acceptable = route.duration_s <= best_duration * 1.4 + 1e-6
                && paths
                    .iter()
                    .all(|earlier| self.overlap(&candidate, earlier) <= 0.75);
            last = candidate.clone();
            if acceptable {
                paths.push(candidate);
                routes.push(route);
            }
        }
        Ok(RouteResponse { routes })
    }

    fn overlap(&self, a: &[Traversal], b: &[Traversal]) -> f64 {
        // A shortest positive-cost path cannot repeat an edge, except terminal
        // partials. Merge intervals so those partials never double count.
        let intervals = |path: &[Traversal]| {
            let mut map: BTreeMap<usize, Vec<(f64, f64)>> = BTreeMap::new();
            for t in path {
                map.entry(t.edge)
                    .or_default()
                    .push((t.from.min(t.to), t.from.max(t.to)));
            }
            for values in map.values_mut() {
                values.sort_by(|a, b| a.0.total_cmp(&b.0));
                let mut merged: Vec<(f64, f64)> = Vec::new();
                for &(low, high) in values.iter() {
                    if let Some(last) = merged.last_mut().filter(|last| last.1 >= low) {
                        last.1 = last.1.max(high);
                    } else {
                        merged.push((low, high));
                    }
                }
                *values = merged;
            }
            map
        };
        let (a, b) = (intervals(a), intervals(b));
        let length = |map: &BTreeMap<usize, Vec<(f64, f64)>>| {
            map.iter()
                .map(|(id, items)| {
                    items
                        .iter()
                        .map(|(x, y)| (y - x) * self.edges[*id].metres)
                        .sum::<f64>()
                })
                .sum::<f64>()
        };
        let denominator = length(&a).min(length(&b));
        if denominator < 1e-6 {
            return 1.0;
        }
        let mut shared = 0.0;
        for (id, left) in &a {
            if let Some(right) = b.get(id) {
                for &(lo, hi) in left {
                    for &(rl, rh) in right {
                        shared += (hi.min(rh) - lo.max(rl)).max(0.0) * self.edges[*id].metres;
                    }
                }
            }
        }
        shared / denominator
    }

    fn materialize(
        &self,
        path: &[Traversal],
        origin: &Resolved,
        destination: &Resolved,
        request: &RouteRequest,
    ) -> Route {
        let mut route = Route {
            mode: request.mode,
            geometry: Vec::new(),
            geometry_offsets_m: Vec::new(),
            distance_m: 0.0,
            duration_s: 0.0,
            segments: Vec::new(),
            maneuvers: Vec::new(),
        };
        let start = origin
            .true_position
            .or_else(|| path.first().map(|t| self.edges[t.edge].point(t.from)))
            .or_else(|| destination.snap.as_ref().map(|s| s.position))
            .unwrap_or_else(|| {
                self.nodes[origin
                    .nodes
                    .iter()
                    .find(|n| destination.nodes.contains(n))
                    .copied()
                    .unwrap_or(origin.nodes[0])]
            });
        route.geometry.push(start);
        route.geometry_offsets_m.push(0.0);
        if let (Some(true_point), Some(snap)) = (origin.true_position, &origin.snap) {
            append(
                &mut route,
                vec![true_point, snap.position],
                snap.distance_m,
                snap.distance_m / 1.25,
                None,
                None,
                false,
            );
        }
        for traversal in path {
            if (traversal.to - traversal.from).abs() < 1e-12 {
                continue;
            }
            let edge = &self.edges[traversal.edge];
            let distance = traversal.distance(self);
            append(
                &mut route,
                edge.slice(traversal.from, traversal.to),
                distance,
                distance / edge.speed(request.mode),
                Some(traversal.edge),
                edge.name.clone(),
                edge.flags & STEPS != 0,
            );
        }
        if let (Some(true_point), Some(snap)) = (destination.true_position, &destination.snap) {
            append(
                &mut route,
                vec![snap.position, true_point],
                snap.distance_m,
                snap.distance_m / 1.25,
                None,
                None,
                false,
            );
        }
        route.maneuvers = maneuvers::generate(self, &route, request);
        route
    }
}

#[allow(clippy::too_many_arguments)]
fn append(
    route: &mut Route,
    points: Vec<Position>,
    distance_m: f64,
    duration_s: f64,
    edge: Option<usize>,
    name: Option<String>,
    steps: bool,
) {
    if distance_m < 1e-8 && points.iter().all(|p| geo::haversine(points[0], *p) < 1e-8) {
        return;
    }
    let start = route.geometry.len() - 1;
    let cumulative = geo::cumulative(&points);
    let length = cumulative.last().copied().unwrap_or(0.0);
    for (i, p) in points.iter().enumerate().skip(1) {
        route.geometry.push(*p);
        let fraction = if length > 0.0 {
            cumulative[i] / length
        } else {
            i as f64 / (points.len() - 1) as f64
        };
        route
            .geometry_offsets_m
            .push(route.distance_m + distance_m * fraction);
    }
    route.segments.push(RouteSegment {
        geometry_start: start,
        geometry_end: route.geometry.len() - 1,
        distance_m,
        duration_s,
        edge,
        name,
        steps,
    });
    route.distance_m += distance_m;
    route.duration_s += duration_s;
}

#[cfg(test)]
mod tests;
