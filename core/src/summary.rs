//! Route identity is shared by native and web clients, and includes unnamed ways.
use crate::{geo, Endpoint, Graph, Route, RouteRequest};
use std::collections::BTreeMap;

pub(crate) fn via(graph: &Graph, route: &Route, request: &RouteRequest) -> String {
    let mut lengths = BTreeMap::<Option<&str>, f64>::new();
    for s in &route.segments {
        if s.edge.is_some() {
            *lengths
                .entry(s.name.as_deref().filter(|n| !n.trim().is_empty()))
                .or_default() += s.distance_m;
        }
    }
    if let Some((Some(name), _)) = lengths
        .iter()
        .max_by(|a, b| a.1.total_cmp(b.1).then_with(|| b.0.cmp(a.0)))
    {
        return (*name).to_owned();
    }
    let is_endpoint = |name: &str| {
        [&request.origin, &request.destination]
            .iter()
            .any(|e| matches!(e, Endpoint::Target { id } if id == name))
    };
    // Count actual route metres near each landmark, weighted by proximity.
    // Sampling prevents a long bent segment's midpoint from misrepresenting it.
    let mut scores = BTreeMap::<&str, f64>::new();
    for s in route
        .segments
        .iter()
        .filter(|s| s.edge.is_some() && s.name.as_ref().is_none_or(|n| n.trim().is_empty()))
    {
        for i in s.geometry_start..s.geometry_end {
            let metres = route.geometry_offsets_m[i + 1] - route.geometry_offsets_m[i];
            let count = (metres / 10.0).ceil().max(1.0) as usize;
            for k in 0..count {
                let p = geo::interpolate(
                    route.geometry[i],
                    route.geometry[i + 1],
                    (k as f64 + 0.5) / count as f64,
                );
                for l in graph.landmarks.iter().filter(|l| !is_endpoint(&l.name)) {
                    let weight = (1.0 - geo::haversine(p, l.at) / 80.0).max(0.0);
                    if weight > 0.0 {
                        *scores.entry(&l.name).or_default() += metres / count as f64 * weight;
                    }
                }
            }
        }
    }
    scores
        .into_iter()
        .max_by(|a, b| {
            a.1.total_cmp(&b.1)
                .then_with(|| a.0.len().cmp(&b.0.len()))
                .then_with(|| b.0.cmp(a.0))
        })
        .map(|(name, _)| name.to_owned())
        .unwrap_or_else(|| "沿途道路".into())
}

#[cfg(test)]
mod tests {
    use crate::{testutil::*, Graph, Landmark};

    #[test]
    fn unnamed_majority_uses_passed_landmark_instead_of_short_named_approach() {
        let mut d = data(
            &[(0., 0.), (20., 0.), (20., 100.), (20., 200.), (40., 200.)],
            &[
                (0, 1, 20., 3, 0),
                (1, 2, 100., 3, -1),
                (2, 3, 100., 3, -1),
                (3, 4, 20., 3, 0),
            ],
        );
        d.landmarks = vec![
            Landmark {
                name: "甲园".into(),
                at: p(25., 100.),
            },
            Landmark {
                name: "start".into(),
                at: p(20., 100.),
            },
            Landmark {
                name: "远处广场".into(),
                at: p(500., 100.),
            },
        ];
        let g = Graph::from_data(d).unwrap();
        assert_eq!(
            one(&g, &request(target("start"), target("end"))).via,
            "甲园"
        );
        assert_eq!(
            one(&g, &request(target("end"), target("start"))).via,
            "甲园"
        );
    }

    #[test]
    fn named_way_accumulates_across_segments_and_unknown_stays_honest() {
        let g = graph(
            &[(0., 0.), (100., 0.), (200., 0.), (210., 0.)],
            &[(0, 1, 100., 3, 1), (1, 2, 100., 3, 1), (2, 3, 10., 3, 0)],
        );
        assert_eq!(
            one(&g, &request(target("start"), target("end"))).via,
            "乙路"
        );
        let g = graph(&[(0., 0.), (100., 0.)], &[(0, 1, 100., 3, -1)]);
        assert_eq!(
            one(&g, &request(target("start"), target("end"))).via,
            "沿途道路"
        );
    }
}
