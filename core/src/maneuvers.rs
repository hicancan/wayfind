use crate::{
    geo, graph::CONNECTOR, instructions, Endpoint, Graph, LandmarkRelation, Maneuver,
    ManeuverAction, ManeuverLandmark, ManeuverType, Modifier, Route, RouteRequest,
};

pub(crate) fn classify(degrees: f64) -> Modifier {
    match (degrees < 0.0, degrees.abs()) {
        (_, d) if d < 15.0 => Modifier::Ahead,
        (true, d) if d < 45.0 => Modifier::SlightLeft,
        (false, d) if d < 45.0 => Modifier::SlightRight,
        (true, d) if d < 135.0 => Modifier::Left,
        (false, d) if d < 135.0 => Modifier::Right,
        (true, _) => Modifier::SharpLeft,
        (false, _) => Modifier::SharpRight,
    }
}

pub(crate) fn generate(graph: &Graph, route: &Route, request: &RouteRequest) -> Vec<Maneuver> {
    let geometry_offsets = geo::cumulative(&route.geometry);
    let total = geometry_offsets.last().copied().unwrap_or(0.0);
    let bearings = |index: usize| {
        let at = geometry_offsets[index];
        // Do not sample across a neighbouring decision on a tiny join: that
        // would smear two opposite turns into unrelated slight turns.
        let low = route
            .segments
            .iter()
            .find(|s| s.geometry_end == index)
            .map_or(0.0, |s| geometry_offsets[s.geometry_start]);
        let high = route
            .segments
            .iter()
            .find(|s| s.geometry_start == index)
            .map_or(total, |s| geometry_offsets[s.geometry_end]);
        let before = geo::point_at(&route.geometry, &geometry_offsets, (at - 10.0).max(low));
        let after = geo::point_at(&route.geometry, &geometry_offsets, (at + 10.0).min(high));
        let p = route.geometry[index];
        let incoming = if at > 1e-6 {
            geo::bearing(before, p)
        } else {
            geo::bearing(p, after)
        };
        let outgoing = if at < total - 1e-6 {
            geo::bearing(p, after)
        } else {
            incoming
        };
        (incoming, outgoing)
    };
    let make = |kind, modifier, index: usize, name: Option<String>| {
        let (before, after) = bearings(index);
        Maneuver {
            kind,
            modifier,
            position: route.geometry[index],
            bearing_before: before,
            bearing_after: after,
            offset_m: route.geometry_offsets_m[index],
            distance_to_next_m: 0.0,
            way_name: name,
            target_id: None,
            landmark: None,
            leads_to_arrival: false,
            following_actions: Vec::new(),
            instruction: String::new(),
        }
    };
    let first_name = route.segments.first().and_then(|s| s.name.clone());
    let mut result = vec![make(ManeuverType::Depart, Modifier::Ahead, 0, first_name)];
    let mut minor_joins = Vec::new();
    if route.segments.first().is_some_and(|s| s.steps) {
        result.push(make(
            ManeuverType::Stairs,
            Modifier::Ahead,
            0,
            route.segments[0].name.clone(),
        ));
    }
    for pair in route.segments.windows(2) {
        let (incoming, outgoing) = (&pair[0], &pair[1]);
        let index = outgoing.geometry_start;
        let (before, after) = bearings(index);
        let modifier = classify(geo::angle(before, after));
        if outgoing.steps && !incoming.steps {
            result.push(make(
                ManeuverType::Stairs,
                Modifier::Ahead,
                index,
                outgoing.name.clone(),
            ));
            continue;
        }
        // True-position connectors have no network decision to announce.
        if incoming.edge.is_none() || outgoing.edge.is_none() {
            continue;
        }
        let delta = geo::angle(before, after).abs();
        // Small entrance spurs and short unnamed links are joins rather than
        // independent junctions. Keep substantial changes of direction.
        let short_join = |id: usize| {
            let edge = &graph.edges[id];
            edge.flags & CONNECTOR != 0
                || edge.metres <= 10.0
                || (edge.metres <= 25.0 && (edge.name.is_none() || incoming.name == outgoing.name))
        };
        let node = incoming.edge.and_then(|id| {
            let edge = &graph.edges[id];
            [edge.a, edge.b]
                .into_iter()
                .find(|n| geo::haversine(graph.nodes[*n], route.geometry[index]) < 0.05)
        });
        let only_joins = incoming.edge.is_some_and(short_join)
            || outgoing.edge.is_some_and(short_join)
            || node.is_some_and(|n| {
                let branches: Vec<_> = graph.adjacency[n]
                    .iter()
                    .filter(|(_, id)| {
                        Some(*id) != incoming.edge
                            && Some(*id) != outgoing.edge
                            && graph.edges[*id].usable(request.mode, request.avoid_steps)
                    })
                    .collect();
                !branches.is_empty() && branches.iter().all(|(_, id)| short_join(*id))
            });
        if only_joins && delta < 60.0 - 1e-6 {
            minor_joins.push(route.geometry_offsets_m[index]);
        }
        let ambiguous = incoming.edge.is_some_and(|id| {
            let edge = &graph.edges[id];
            let p = route.geometry[index];
            let node = if geo::haversine(p, graph.nodes[edge.a]) < 0.05 {
                Some(edge.a)
            } else if geo::haversine(p, graph.nodes[edge.b]) < 0.05 {
                Some(edge.b)
            } else {
                None
            };
            node.is_some_and(|node| {
                graph.adjacency[node]
                    .iter()
                    .filter(|(_, candidate)| {
                        if Some(*candidate) == incoming.edge {
                            return false;
                        }
                        let edge = &graph.edges[*candidate];
                        if !edge.usable(request.mode, request.avoid_steps) {
                            return false;
                        }
                        let fraction = if edge.a == node { 0.0 } else { 1.0 };
                        let step = (10.0 / edge.geometric_length().max(1e-6)).min(1.0);
                        let to = if fraction == 0.0 { step } else { 1.0 - step };
                        geo::angle(before, geo::bearing(p, edge.point(to))).abs() < 45.0
                    })
                    .count()
                    >= 2
            })
        });
        if modifier != Modifier::Ahead {
            result.push(make(
                ManeuverType::Turn,
                modifier,
                index,
                outgoing.name.clone(),
            ));
        } else if incoming.name != outgoing.name || ambiguous {
            result.push(make(
                ManeuverType::Continue,
                Modifier::Ahead,
                index,
                outgoing.name.clone(),
            ));
        }
    }
    let mut side = Modifier::Ahead;
    if let Some(last) = route.segments.last().filter(|s| {
        s.edge.is_none()
            || s.edge
                .is_some_and(|id| graph.edges[id].flags & CONNECTOR != 0)
    }) {
        let index = last.geometry_start;
        let at = geometry_offsets[index];
        if at > 0.1 && last.distance_m > 0.1 {
            let before = geo::point_at(&route.geometry, &geometry_offsets, (at - 10.0).max(0.0));
            let turn = geo::angle(
                geo::bearing(before, route.geometry[index]),
                geo::bearing(route.geometry[index], route.geometry[last.geometry_end]),
            );
            if turn < -15.0 {
                side = Modifier::Left;
            } else if turn > 15.0 {
                side = Modifier::Right;
            }
        }
    }
    result.push(make(
        ManeuverType::Arrive,
        side,
        route.geometry.len() - 1,
        None,
    ));
    if let Endpoint::Target { id } = &request.origin {
        result[0].target_id = Some(id.clone());
    }
    simplify(&mut result, route, &minor_joins, graph);
    if let Endpoint::Target { id } = &request.destination {
        if let Some(last) = result.last_mut() {
            last.target_id = Some(id.clone());
        }
    }
    for index in 0..result.len() {
        result[index].distance_to_next_m = result
            .get(index + 1)
            .map_or(0.0, |next| next.offset_m - result[index].offset_m);
        result[index].leads_to_arrival = result
            .get(index + 1)
            .is_some_and(|m| m.kind == ManeuverType::Arrive);
        if result[index].kind == ManeuverType::Turn {
            result[index].landmark = landmark(graph, &result[index], None);
        }
        result[index].instruction = instructions::chinese(&result[index], route.mode);
    }
    result
}

fn landmark(graph: &Graph, maneuver: &Maneuver, exclude: Option<&str>) -> Option<ManeuverLandmark> {
    // Distance is primary. At effectively the same distance, a fuller name
    // gives the walker more context; lexical order makes ties deterministic.
    let (_, selected) = graph
        .landmarks
        .iter()
        .filter(|landmark| Some(landmark.name.as_str()) != exclude)
        .filter_map(|landmark| {
            let distance = geo::haversine(maneuver.position, landmark.at);
            (distance <= 35.0).then_some((distance, landmark))
        })
        .min_by(|(a, x), (b, y)| {
            a.total_cmp(b)
                .then_with(|| y.name.chars().count().cmp(&x.name.chars().count()))
                .then_with(|| x.name.cmp(&y.name))
        })?;
    let angle = geo::angle(
        maneuver.bearing_before,
        geo::bearing(maneuver.position, selected.at),
    );
    let relation = if angle.abs() <= 45.0 {
        LandmarkRelation::Ahead
    } else if angle.abs() >= 135.0 {
        LandmarkRelation::Passed
    } else {
        match ((geo::bearing(selected.at, maneuver.position) + 45.0) / 90.0) as usize % 4 {
            0 => LandmarkRelation::North,
            1 => LandmarkRelation::East,
            2 => LandmarkRelation::South,
            _ => LandmarkRelation::West,
        }
    };
    Some(ManeuverLandmark {
        name: selected.name.clone(),
        at: selected.at,
        relation,
    })
}

fn simplify(result: &mut Vec<Maneuver>, route: &Route, minor_joins: &[f64], graph: &Graph) {
    let mut collapsed_jogs = Vec::new();
    let mut i = 1;
    while i + 1 < result.len() {
        let (a, b) = (&result[i], &result[i + 1]);
        let first = geo::angle(a.bearing_before, a.bearing_after);
        let second = geo::angle(b.bearing_before, b.bearing_after);
        let net = geo::angle(a.bearing_before, b.bearing_after);
        let gap = b.offset_m - a.offset_m;
        // A slightly longer unnamed join can also be a crossing between
        // parallel paths. Only collapse it when the arms nearly align.
        let unnamed_join = gap <= 25.0
            && a.way_name == b.way_name
            && route
                .segments
                .iter()
                .filter(|s| {
                    let start = route.geometry_offsets_m[s.geometry_start];
                    start >= a.offset_m - 1e-6 && start < b.offset_m - 1e-6
                })
                .all(|s| s.name.is_none() && !s.steps && s.distance_m <= 25.0);
        if a.kind == ManeuverType::Turn
            && b.kind == ManeuverType::Turn
            && first * second < 0.0
            && net.abs() < 45.0
            && (gap <= 8.0 || unnamed_join)
        {
            if classify(net) == Modifier::Ahead {
                result.drain(i..=i + 1);
                i = i.saturating_sub(1).max(1);
            } else {
                collapsed_jogs.push(result[i].offset_m);
                result[i].bearing_after = result[i + 1].bearing_after;
                result[i].modifier = classify(net);
                result[i].way_name = result[i + 1].way_name.clone();
                result.remove(i + 1);
            }
            continue;
        }
        i += 1;
    }
    // Cancel pairs before suppressing shallow joins so a small residual angle
    // does not accidentally leave the first, larger turn behind.
    result.retain(|m| {
        !minor_joins.contains(&m.offset_m)
            || collapsed_jogs.contains(&m.offset_m)
            || matches!(
                m.kind,
                ManeuverType::Depart | ManeuverType::Arrive | ManeuverType::Stairs
            )
    });
    // Fold close decisions into one instruction, retaining stairs and both
    // actions when they cannot safely be cancelled. Arrival remains separate.
    let mut spaced: Vec<Maneuver> = Vec::with_capacity(result.len());
    for maneuver in result.drain(..) {
        if let Some(previous) = spaced.last_mut() {
            if maneuver.kind != ManeuverType::Arrive && maneuver.offset_m - previous.offset_m < 10.0
            {
                previous.following_actions.push(ManeuverAction {
                    kind: maneuver.kind,
                    modifier: maneuver.modifier,
                    offset_m: maneuver.offset_m,
                    way_name: maneuver.way_name.clone(),
                    landmark: if maneuver.kind == ManeuverType::Turn {
                        landmark(
                            graph,
                            &maneuver,
                            if previous.kind == ManeuverType::Depart {
                                previous.target_id.as_deref()
                            } else {
                                None
                            },
                        )
                    } else {
                        None
                    },
                });
                continue;
            }
        }
        spaced.push(maneuver);
    }
    *result = spaced;
}

#[cfg(test)]
mod guidance_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn angle_bands_both_directions_and_boundaries() {
        for (a, expected) in [
            (0.0, Modifier::Ahead),
            (14.9, Modifier::Ahead),
            (15.0, Modifier::SlightRight),
            (44.9, Modifier::SlightRight),
            (45.0, Modifier::Right),
            (134.9, Modifier::Right),
            (135.0, Modifier::SharpRight),
            (-15.0, Modifier::SlightLeft),
            (-45.0, Modifier::Left),
            (-135.0, Modifier::SharpLeft),
            (-179.0, Modifier::SharpLeft),
        ] {
            assert_eq!(classify(a), expected);
        }
        assert_eq!(geo::angle(350.0, 10.0), 20.0);
        assert_eq!(geo::angle(10.0, 350.0), -20.0);
    }
}
