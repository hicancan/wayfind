use crate::{geo, instructions, Graph, Maneuver, ManeuverType, Modifier, Route, RouteRequest};

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
        let before = geo::point_at(&route.geometry, &geometry_offsets, (at - 10.0).max(0.0));
        let after = geo::point_at(&route.geometry, &geometry_offsets, (at + 10.0).min(total));
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
            instruction: String::new(),
        }
    };
    let first_name = route.segments.first().and_then(|s| s.name.clone());
    let mut result = vec![make(ManeuverType::Depart, Modifier::Ahead, 0, first_name)];
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
    if let Some(last) = route.segments.last().filter(|s| s.edge.is_none()) {
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
    for index in 0..result.len() {
        result[index].distance_to_next_m = result
            .get(index + 1)
            .map_or(0.0, |next| next.offset_m - result[index].offset_m);
        result[index].instruction = instructions::chinese(&result[index], route.mode);
    }
    result
}

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
