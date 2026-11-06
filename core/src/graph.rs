use std::collections::BTreeMap;

use rstar::{RTree, RTreeObject, AABB};
use serde::{Deserialize, Serialize};

use crate::{geo, Error, Mode, Position, Result, Snap};

pub(crate) const WALK: u32 = 1;
pub(crate) const BIKE: u32 = 2;
pub(crate) const STEPS: u32 = 4;
pub(crate) const CONNECTOR: u32 = 8;
pub(crate) const PUBLIC: u32 = 16;
fn public_flag() -> u32 {
    PUBLIC
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Flags {
    pub walk: u32,
    pub bike: u32,
    pub steps: u32,
    pub connector: u32,
    /// Marks a road outside the area being navigated, used only when a route must leave it.
    #[serde(default = "public_flag")]
    pub public: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    #[test]
    fn snap_interior_shape_reverse_and_max_distance() {
        let mut d = data(&[(0.0, 0.0), (100.0, 100.0)], &[(0, 1, 200.0, 3, 0)]);
        d.shapes[0] = vec![p(100.0, 0.0)];
        let g = Graph::from_data(d).unwrap();
        let s = g.snap(p(103.0, 50.0), Mode::Walk, false, 5.0).unwrap();
        close(s.distance_m, 3.0, 0.01);
        close(s.offset_m, 150.0, 0.01);
        close(geo::haversine(s.position, p(100.0, 50.0)), 0.0, 0.01);
        assert!(matches!(
            g.snap(p(103.0, 50.0), Mode::Walk, false, 2.0),
            Err(Error::Snap)
        ));
        let s = g.snap(p(-10.0, 0.0), Mode::Walk, false, 11.0).unwrap();
        close(s.offset_m, 0.0, 0.01);
    }

    #[test]
    fn snap_modes_steps_and_unusable_nearer_edges() {
        let g = graph(
            &[(0.0, 0.0), (100.0, 0.0), (0.0, 10.0), (100.0, 10.0)],
            &[(0, 1, 100.0, 7, -1), (2, 3, 100.0, 3, -1)],
        );
        assert_eq!(
            g.snap(p(50.0, 1.0), Mode::Walk, false, 20.0).unwrap().edge,
            0
        );
        assert_eq!(
            g.snap(p(50.0, 1.0), Mode::Walk, true, 20.0).unwrap().edge,
            1
        );
        assert_eq!(
            g.snap(p(50.0, 1.0), Mode::Bike, false, 20.0).unwrap().edge,
            1
        );
        assert!(g.snap(p(0.0, 0.0), Mode::Walk, false, f64::NAN).is_err());
        assert!(g.snap([181.0, 0.0], Mode::Walk, false, 100.0).is_err());
    }

    #[test]
    fn spherical_index_handles_date_line_and_high_latitude() {
        for (a, b, point) in [
            ([179.999, 0.0], [-179.999, 0.0], [180.0, 0.00001]),
            ([20.0, 89.9], [20.01, 89.9], [20.005, 89.90001]),
        ] {
            let mut d = data(&[(0.0, 0.0), (1.0, 0.0)], &[(0, 1, 200.0, 3, -1)]);
            d.nodes = vec![a, b];
            let s = Graph::from_data(d)
                .unwrap()
                .snap(point, Mode::Walk, false, 3.0)
                .unwrap();
            close(s.fraction, 0.5, 0.001);
            assert!(s.distance_m < 2.0);
        }
    }

    #[test]
    fn invalid_graphs_are_errors_extra_fields_are_ignored() {
        let valid = data(&[(0.0, 0.0), (100.0, 0.0)], &[(0, 1, 100.0, 3, -1)]);
        let mut value = serde_json::to_value(&valid).unwrap();
        value["future"] = serde_json::json!({"anything":true});
        assert!(Graph::from_json(&value.to_string()).is_ok());
        let edits: Vec<fn(&mut GraphData)> = vec![
            |d| d.format = "wayfind-graph/2".into(),
            |d| d.shapes.clear(),
            |d| d.nodes[0] = [0.0, 91.0],
            |d| d.edges[0].2 = -1.0,
            |d| d.edges[0].2 = f64::NAN,
            |d| d.edges[0].1 = 999,
            |d| d.edges[0].4 = -2,
            |d| d.edges[0].4 = 2,
            |d| d.flags.walk = 16,
            |d| {
                d.targets.insert("bad".into(), vec![]);
            },
            |d| {
                d.targets.insert("bad".into(), vec![999]);
            },
            |d| d.shapes[0].push([f64::INFINITY, 0.0]),
        ];
        for edit in edits {
            let mut d = valid.clone();
            edit(&mut d);
            assert!(Graph::from_data(d).is_err());
        }
        assert!(Graph::from_json("{}").is_err());
    }

    #[test]
    fn optional_landmarks_validate_names_coordinates_and_structure() {
        let d = data(&[(0.0, 0.0), (100.0, 0.0)], &[(0, 1, 100.0, 3, -1)]);
        assert!(d.landmarks.is_empty());
        let base = serde_json::to_value(d).unwrap();
        let mut omitted = base.clone();
        omitted.as_object_mut().unwrap().remove("landmarks");
        assert!(Graph::from_json(&omitted.to_string())
            .unwrap()
            .landmarks
            .is_empty());
        for bad in [
            serde_json::json!(null),
            serde_json::json!({}),
            serde_json::json!([{"name":"  ","at":[0,0]}]),
            serde_json::json!([{"name":"甲广场","at":[181,0]}]),
            serde_json::json!([{"name":"甲广场","at":[0,91]}]),
            serde_json::json!([{"name":"甲广场"}]),
            serde_json::json!([{"name":42,"at":[0,0]}]),
            serde_json::json!([{"name":"甲广场","at":[0]}]),
        ] {
            let mut value = base.clone();
            value["landmarks"] = bad;
            assert!(Graph::from_json(&value.to_string()).is_err());
        }
        for at in [[f64::NAN, 0.0], [0.0, f64::INFINITY]] {
            let mut d: GraphData = serde_json::from_value(base.clone()).unwrap();
            d.landmarks.push(Landmark {
                name: "甲广场".into(),
                at,
            });
            assert!(Graph::from_data(d).is_err());
        }
    }
}

/// The sole graph interchange format. Serde deliberately ignores extra fields.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphData {
    pub format: String,
    pub nodes: Vec<Position>,
    pub edges: Vec<(usize, usize, f64, u32, i64)>,
    pub shapes: Vec<Vec<Position>>,
    pub names: Vec<String>,
    pub flags: Flags,
    pub targets: BTreeMap<String, Vec<usize>>,
    #[serde(default)]
    pub landmarks: Vec<Landmark>,
}

/// A visible feature supplied by the graph's caller.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Landmark {
    pub name: String,
    pub at: Position,
}

pub(crate) struct Edge {
    pub a: usize,
    pub b: usize,
    pub metres: f64,
    pub flags: u32,
    pub name: Option<String>,
    pub points: Vec<Position>,
    pub offsets: Vec<f64>,
}

impl Edge {
    pub fn usable(&self, mode: Mode, avoid_steps: bool) -> bool {
        let permission = match mode {
            Mode::Walk => WALK,
            Mode::Bike => BIKE,
        };
        self.flags & permission != 0
            && (self.flags & STEPS == 0 || (mode == Mode::Walk && !avoid_steps))
    }

    pub fn speed(&self, mode: Mode) -> f64 {
        if self.flags & STEPS != 0 {
            0.6
        } else if self.flags & CONNECTOR != 0 || mode == Mode::Walk {
            1.25
        } else {
            4.2
        }
    }

    pub fn point(&self, fraction: f64) -> Position {
        geo::point_at(
            &self.points,
            &self.offsets,
            fraction * self.geometric_length(),
        )
    }

    pub fn geometric_length(&self) -> f64 {
        self.offsets.last().copied().unwrap_or(0.0)
    }

    pub fn slice(&self, from: f64, to: f64) -> Vec<Position> {
        let (low, high) = (from.min(to), from.max(to));
        let mut points = vec![self.point(low)];
        let length = self.geometric_length();
        for (p, offset) in self.points.iter().zip(&self.offsets) {
            if *offset > low * length + 1e-7 && *offset < high * length - 1e-7 {
                points.push(*p);
            }
        }
        points.push(self.point(high));
        if from > to {
            points.reverse();
        }
        points
    }
}

struct IndexedSegment {
    edge: usize,
    segment: usize,
    envelope: AABB<[f64; 3]>,
}

impl RTreeObject for IndexedSegment {
    type Envelope = AABB<[f64; 3]>;
    fn envelope(&self) -> Self::Envelope {
        self.envelope
    }
}

pub struct Graph {
    pub(crate) nodes: Vec<Position>,
    pub(crate) edges: Vec<Edge>,
    pub(crate) adjacency: Vec<Vec<(usize, usize)>>,
    pub(crate) targets: BTreeMap<String, Vec<usize>>,
    pub(crate) landmarks: Vec<Landmark>,
    index: RTree<IndexedSegment>,
    /// Declared lengths may be rounded or shorter than geodesic distances.
    pub(crate) heuristic_scale: f64,
}

impl Graph {
    pub fn from_json(json: &str) -> Result<Self> {
        Self::from_data(serde_json::from_str(json)?)
    }

    pub fn from_data(data: GraphData) -> Result<Self> {
        if data.format != "wayfind-graph/1" {
            return Err(Error::Invalid("unsupported graph format".into()));
        }
        if (
            data.flags.walk,
            data.flags.bike,
            data.flags.steps,
            data.flags.connector,
            data.flags.public,
        ) != (WALK, BIKE, STEPS, CONNECTOR, PUBLIC)
        {
            return Err(Error::Invalid(
                "flag mapping must be walk=1, bike=2, steps=4, connector=8, public=16".into(),
            ));
        }
        if data.shapes.len() != data.edges.len() {
            return Err(Error::Invalid("one shape is required per edge".into()));
        }
        for p in &data.nodes {
            geo::validate_position(*p)?;
        }
        for landmark in &data.landmarks {
            geo::validate_position(landmark.at)?;
            if landmark.name.trim().is_empty() {
                return Err(Error::Invalid("landmark name must not be blank".into()));
            }
        }
        for nodes in data.targets.values() {
            if nodes.is_empty() || nodes.iter().any(|n| *n >= data.nodes.len()) {
                return Err(Error::Invalid(
                    "targets must contain valid node indices".into(),
                ));
            }
        }
        let mut edges = Vec::with_capacity(data.edges.len());
        let mut adjacency = vec![Vec::new(); data.nodes.len()];
        let mut segments = Vec::new();
        let mut heuristic_scale: f64 = 1.0;
        for (id, ((a, b, metres, flags, name), shape)) in
            data.edges.into_iter().zip(data.shapes).enumerate()
        {
            if a >= data.nodes.len() || b >= data.nodes.len() || !metres.is_finite() || metres < 0.0
            {
                return Err(Error::Invalid(format!(
                    "invalid endpoints or length on edge {id}"
                )));
            }
            let name = if name == -1 {
                None
            } else {
                Some(
                    data.names
                        .get(
                            usize::try_from(name)
                                .map_err(|_| Error::Invalid("invalid name index".into()))?,
                        )
                        .ok_or_else(|| Error::Invalid("invalid name index".into()))?
                        .clone(),
                )
            };
            let mut points = vec![data.nodes[a]];
            for p in shape {
                geo::validate_position(p)?;
                points.push(p);
            }
            points.push(data.nodes[b]);
            let offsets = geo::cumulative(&points);
            for (segment, pair) in points.windows(2).enumerate() {
                let length = geo::haversine(pair[0], pair[1]);
                if length >= std::f64::consts::PI * geo::R - 1.0 {
                    return Err(Error::Invalid("antipodal segment is ambiguous".into()));
                }
                let (x, y) = (geo::xyz(pair[0]), geo::xyz(pair[1]));
                // Arc is at most this far outside the chord's envelope.
                let padding = geo::R * (1.0 - (length / (2.0 * geo::R)).cos()) + 1e-6;
                segments.push(IndexedSegment {
                    edge: id,
                    segment,
                    envelope: AABB::from_corners(
                        std::array::from_fn(|i| x[i].min(y[i]) - padding),
                        std::array::from_fn(|i| x[i].max(y[i]) + padding),
                    ),
                });
            }
            let direct = geo::haversine(data.nodes[a], data.nodes[b]);
            if direct > 0.0 {
                heuristic_scale = heuristic_scale.min(metres / direct);
            }
            adjacency[a].push((b, id));
            adjacency[b].push((a, id));
            edges.push(Edge {
                a,
                b,
                metres,
                flags,
                name,
                points,
                offsets,
            });
        }
        Ok(Self {
            nodes: data.nodes,
            edges,
            adjacency,
            targets: data.targets,
            landmarks: data.landmarks,
            index: RTree::bulk_load(segments),
            heuristic_scale,
        })
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
    pub fn targets(&self) -> &BTreeMap<String, Vec<usize>> {
        &self.targets
    }

    pub fn snap(
        &self,
        position: Position,
        mode: Mode,
        avoid_steps: bool,
        max_distance_m: f64,
    ) -> Result<Snap> {
        geo::validate_position(position)?;
        if !max_distance_m.is_finite() || max_distance_m < 0.0 {
            return Err(Error::Invalid(
                "snap distance must be finite and nonnegative".into(),
            ));
        }
        let center = geo::xyz(position);
        let radius = max_distance_m.min(2.0 * geo::R) + 1e-6;
        let envelope = AABB::from_corners(center.map(|x| x - radius), center.map(|x| x + radius));
        let mut best: Option<Snap> = None;
        for item in self.index.locate_in_envelope_intersecting(&envelope) {
            let edge = &self.edges[item.edge];
            if !edge.usable(mode, avoid_steps) {
                continue;
            }
            let (point, t, distance) = geo::project(
                position,
                edge.points[item.segment],
                edge.points[item.segment + 1],
            );
            if distance > max_distance_m + 1e-7 {
                continue;
            }
            if best.as_ref().is_some_and(|b| {
                distance > b.distance_m + 1e-7
                    || ((distance - b.distance_m).abs() <= 1e-7 && item.edge >= b.edge)
            }) {
                continue;
            }
            let start = edge.offsets[item.segment];
            let span = edge.offsets[item.segment + 1] - start;
            let fraction = if edge.geometric_length() > 0.0 {
                (start + t * span) / edge.geometric_length()
            } else {
                0.0
            };
            best = Some(Snap {
                position: point,
                edge: item.edge,
                offset_m: fraction * edge.metres,
                fraction,
                distance_m: distance,
            });
        }
        best.ok_or(Error::Snap)
    }

    /// Component labels for callers doing connectivity diagnostics.
    pub fn components(&self, mode: Mode, avoid_steps: bool) -> Vec<usize> {
        let mut labels = vec![usize::MAX; self.nodes.len()];
        let mut label = 0;
        for start in 0..self.nodes.len() {
            if labels[start] != usize::MAX {
                continue;
            }
            labels[start] = label;
            let mut stack = vec![start];
            while let Some(node) = stack.pop() {
                for &(next, edge) in &self.adjacency[node] {
                    if labels[next] == usize::MAX && self.edges[edge].usable(mode, avoid_steps) {
                        labels[next] = label;
                        stack.push(next);
                    }
                }
            }
            label += 1;
        }
        labels
    }
}
