use super::*;
use crate::testutil::*;

fn path_route(g: &Graph, path: &[Traversal], r: &RouteRequest) -> Route {
    g.materialize(
        path,
        &g.resolve(&r.origin, r).unwrap(),
        &g.resolve(&r.destination, r).unwrap(),
        r,
    )
}

fn forward(ids: &[usize]) -> Vec<Traversal> {
    ids.iter()
        .map(|&edge| Traversal {
            edge,
            from: 0.0,
            to: 1.0,
        })
        .collect()
}

#[test]
fn parallel_lines_joined_only_at_far_end_are_rejected_after_penalty_search() {
    let g = graph(
        &[
            (-200.0, 0.0),
            (-5.0, 0.0),
            (-5.0, -45.0),
            (5.0, -45.0),
            (5.0, 0.0),
            (200.0, 0.0),
        ],
        &[
            (0, 5, 400.0, 3, -1),
            (0, 1, 195.0, 3, -1),
            (1, 2, 45.0, 3, -1),
            (2, 3, 10.0, 3, -1),
            (3, 4, 45.0, 3, -1),
            (4, 5, 195.0, 3, -1),
        ],
    );
    let mut r = request(target("start"), target("end"));
    r.alternatives = 3;
    let detour = forward(&[1, 2, 3, 4, 5]);
    let route = path_route(&g, &detour, &r);
    assert!(route.duration_s < 400.0 / 1.25 * 1.4);
    assert_eq!(g.overlap(&detour, &forward(&[0])), 0.0);
    // The graph itself has no shortcut between the two arms. Local optimality
    // alone cannot reject it: geometric sanity must be independent of topology.
    assert!(g.locally_optimal(&detour, &r, 320.0));
    assert_eq!(g.no_uturn(&detour, &route), Some(Rejection::Reversal));
    let mut rejected = 0;
    let response = g
        .route_observed(&r, |p, reason| {
            if p.iter().map(|t| t.edge).eq([1, 2, 3, 4, 5]) {
                assert_eq!(reason, Some(Rejection::Reversal));
                rejected += 1;
            }
        })
        .unwrap();
    assert!(
        rejected > 0,
        "the penalty search must actually produce the regression candidate"
    );
    assert_eq!(response.routes.len(), 1);
    close(response.routes[0].distance_m, 400.0, 0.001);
}

#[test]
fn grid_keeps_three_genuine_alternatives_in_both_modes() {
    let nodes: Vec<_> = (0..5)
        .flat_map(|y| (0..5).map(move |x| (x as f64 * 100.0, y as f64 * 100.0)))
        .collect();
    let mut edges = vec![];
    for y in 0..5 {
        for x in 0..5 {
            let n = y * 5 + x;
            if x < 4 {
                edges.push((n, n + 1, 100.0, 3, -1));
            }
            if y < 4 {
                edges.push((n, n + 5, 100.0, 3, -1));
            }
        }
    }
    let g = graph(&nodes, &edges);
    for mode in [Mode::Walk, Mode::Bike] {
        let mut r = request(target("start"), target("end"));
        r.mode = mode;
        r.alternatives = 3;
        let mut accepted = 0;
        let response = g
            .route_observed(&r, |path, reason| {
                if reason.is_none() {
                    let route = path_route(&g, path, &r);
                    assert_eq!(g.no_uturn(path, &route), None);
                    assert!(g.locally_optimal(
                        path,
                        &r,
                        800.0 / if mode == Mode::Walk { 1.25 } else { 4.2 }
                    ));
                    accepted += 1;
                }
            })
            .unwrap();
        assert_eq!(accepted, 2);
        assert_eq!(response.routes.len(), 3);
        for route in response.routes {
            close(route.distance_m, 800.0, 0.001);
        }
    }
}

#[test]
fn local_windows_reject_small_shortcuts_and_respect_tolerance_and_access() {
    let mut d = data(
        &[
            (-200.0, 0.0),
            (0.0, 0.0),
            (30.0, 10.0),
            (60.0, 0.0),
            (260.0, 0.0),
        ],
        &[
            (0, 1, 200.0, 3, -1),
            (1, 2, 31.0, 3, -1),
            (2, 3, 31.0, 3, -1),
            (3, 4, 200.0, 3, -1),
            (1, 3, 50.0, 3, -1),
        ],
    );
    let path = forward(&[0, 1, 2, 3]);
    let mut r = request(target("start"), target("end"));
    let g = Graph::from_data(d.clone()).unwrap();
    assert_eq!(g.no_uturn(&path, &path_route(&g, &path, &r)), None);
    assert!(!g.locally_optimal(&path, &r, 360.0));
    // A length-T-only check would dilute the detour with optimal approach legs.
    assert_eq!(
        g.alternative_quality(&path, &path_route(&g, &path, &r), &r, 360.0),
        Some(Rejection::LocalOptimality)
    );
    d.edges[4].2 = 62.0 / 1.1;
    assert!(Graph::from_data(d.clone())
        .unwrap()
        .locally_optimal(&path, &r, 360.0));
    d.edges[4].2 -= 0.001;
    assert!(!Graph::from_data(d.clone())
        .unwrap()
        .locally_optimal(&path, &r, 360.0));
    d.edges[4].2 = 20.0;
    d.edges[4].3 = 5; // walking stairs: compare time, not metres
    let g = Graph::from_data(d).unwrap();
    assert!(!g.locally_optimal(&path, &r, 360.0));
    r.avoid_steps = true;
    assert!(g.locally_optimal(&path, &r, 360.0));
    r.mode = Mode::Bike;
    r.avoid_steps = false;
    assert!(g.locally_optimal(&path, &r, 360.0));
}

#[test]
fn local_windows_include_edge_interiors_and_partial_terminals() {
    let g = graph(
        &[(0.0, 0.0), (100.0, 0.0)],
        &[(0, 1, 100.0, 3, -1), (0, 1, 1.0, 3, -1)],
    );
    let r = request(position(10.0, 0.0), position(90.0, 0.0));
    // No graph node is on this partial path. A window near the ends can still
    // use the cheap parallel edge via both graph nodes.
    let path = vec![Traversal {
        edge: 0,
        from: 0.1,
        to: 0.9,
    }];
    assert!(!g.locally_optimal(&path, &r, 300.0));
    let reverse = vec![Traversal {
        edge: 0,
        from: 0.9,
        to: 0.1,
    }];
    assert!(!g.locally_optimal(&reverse, &r, 300.0));
    let short = vec![Traversal {
        edge: 0,
        from: 0.4,
        to: 0.6,
    }];
    assert!(g.locally_optimal(&short, &r, 300.0));
}

#[test]
fn repeated_nodes_zero_cost_edges_and_terminal_duplicates() {
    let g = graph(
        &[(0.0, 0.0), (100.0, 0.0), (200.0, 0.0)],
        &[(0, 1, 0.0, 3, -1), (1, 2, 100.0, 3, -1)],
    );
    let r = request(target("start"), target("end"));
    let path = vec![
        Traversal {
            edge: 0,
            from: 0.0,
            to: 1.0,
        },
        Traversal {
            edge: 0,
            from: 1.0,
            to: 0.0,
        },
    ];
    assert_eq!(
        g.no_uturn(&path, &path_route(&g, &path, &r)),
        Some(Rejection::RepeatedNode)
    );
    let path = vec![
        Traversal {
            edge: 0,
            from: 0.0,
            to: 0.0,
        },
        Traversal {
            edge: 0,
            from: 0.0,
            to: 1.0,
        },
        Traversal {
            edge: 1,
            from: 0.0,
            to: 1.0,
        },
    ];
    assert_eq!(g.no_uturn(&path, &path_route(&g, &path, &r)), None);
}

#[test]
fn reversals_inside_shapes_and_thresholds() {
    for (angle, rejected) in [(149.9_f64, false), (150.1, true)] {
        let rad = angle.to_radians();
        let mut d = data(
            &[(0.0, 0.0), (100.0 + 20.0 * rad.cos(), 20.0 * rad.sin())],
            &[(0, 1, 120.0, 3, -1)],
        );
        d.shapes[0] = vec![p(100.0, 0.0)];
        let g = Graph::from_data(d).unwrap();
        let r = request(target("start"), target("end"));
        assert_eq!(
            g.no_uturn(&forward(&[0]), &path_route(&g, &forward(&[0]), &r))
                == Some(Rejection::Reversal),
            rejected
        );
    }
}

#[test]
fn continuous_near_returns_include_crossings_and_obey_distance_and_travel_limits() {
    for (gap, expected) in [(14.9, true), (15.1, false)] {
        let g = graph(
            &[
                (0.0, 0.0),
                (100.0, 0.0),
                (100.0, 40.0),
                (gap, 40.0),
                (gap, gap),
            ],
            &[
                (0, 1, 100.0, 3, -1),
                (1, 2, 40.0, 3, -1),
                (2, 3, 100.0 - gap, 3, -1),
                (3, 4, 40.0 - gap, 3, -1),
            ],
        );
        let path = forward(&[0, 1, 2, 3]);
        let r = request(target("start"), target("end"));
        assert_eq!(
            g.no_uturn(&path, &path_route(&g, &path, &r)),
            expected.then_some(Rejection::NearReturn)
        );
    }
    let g = graph(
        &[
            (-100.0, 0.0),
            (100.0, 0.0),
            (100.0, 100.0),
            (0.0, 100.0),
            (0.0, -100.0),
        ],
        &[
            (0, 1, 200.0, 3, -1),
            (1, 2, 100.0, 3, -1),
            (2, 3, 100.0, 3, -1),
            (3, 4, 200.0, 3, -1),
        ],
    );
    let r = request(target("start"), target("end"));
    let path = forward(&[0, 1, 2, 3]);
    assert_eq!(
        g.no_uturn(&path, &path_route(&g, &path, &r)),
        Some(Rejection::NearReturn)
    );
    // Test the travel cutoff without also triggering bearing reversal checks.
    for (travel, expected) in [(60.0, false), (60.01, true)] {
        let g = graph(
            &[(0.0, 0.0), (20.0, 0.0), (20.0, 10.0), (0.0, 10.0)],
            &[
                (0, 1, travel / 3.0, 3, -1),
                (1, 2, travel / 3.0, 3, -1),
                (2, 3, travel / 3.0, 3, -1),
            ],
        );
        assert_eq!(
            near_return(&path_route(&g, &forward(&[0, 1, 2]), &r)),
            expected
        );
    }
    // Declared travel can exceed geometric length, including coincident nodes.
    let g = graph(&[(0.0, 0.0), (0.0, 0.0)], &[(0, 1, 100.0, 3, -1)]);
    assert!(near_return(&path_route(&g, &forward(&[0]), &r)));
}

// Independent reference: unrestricted A* between exactly resolved edge points,
// with no re-snapping (which could accidentally move them onto another edge).
fn audit_local_windows(g: &Graph, path: &[Traversal], r: &RouteRequest, best: f64) {
    let samples = g.local_samples(path, r.mode, best * LOCAL_FRACTION);
    let penalties = vec![1.0; g.edges.len()];
    let resolved = |s: &Sample| Resolved {
        true_position: None,
        nodes: vec![],
        snap: Some(Snap {
            position: g.edges[s.edge].point(s.fraction),
            edge: s.edge,
            fraction: s.fraction,
            offset_m: s.fraction * g.edges[s.edge].metres,
            distance_m: 0.0,
        }),
    };
    for (i, start) in samples.iter().enumerate() {
        for end in samples[i + 1..]
            .iter()
            .take_while(|end| end.cost - start.cost <= best * LOCAL_FRACTION + EPS)
        {
            let shortest: f64 = g
                .search(&resolved(start), &resolved(end), r, &penalties)
                .unwrap()
                .iter()
                .map(|t| t.cost(g, r.mode, &penalties))
                .sum();
            assert!(
                end.cost - start.cost <= shortest * LOCAL_TOLERANCE + EPS,
                "external alternative has a local shortcut"
            );
        }
    }
}

#[test]
fn optional_external_graph_three_hundred_alternative_pairs() {
    let Some(file) = std::env::var_os("WAYFIND_GRAPH_FILE") else {
        eprintln!("skipped: WAYFIND_GRAPH_FILE is unset");
        return;
    };
    if !std::path::Path::new(&file).is_file() {
        eprintln!("skipped: external graph is absent");
        return;
    }
    let g = Graph::from_json(&std::fs::read_to_string(file).expect("read external graph"))
        .expect("load external graph");
    let ids: Vec<_> = g.targets.keys().collect();
    assert!(ids.len() >= 2);
    let mut state = 0x5eed_u64;
    let mut random = || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        (state >> 32) as usize
    };
    let mut rejected = BTreeMap::<Rejection, usize>::new();
    let mut returned = 0;
    let mut counts = [0; 3];
    let start = std::time::Instant::now();
    for _ in 0..300 {
        let a = random() % ids.len();
        let b = (a + 1 + random() % (ids.len() - 1)) % ids.len();
        let mut r = request(target(ids[a]), target(ids[b]));
        r.alternatives = 3;
        let mut seen = HashSet::new();
        let mut accepted = vec![];
        let response = g
            .route_observed(&r, |path, reason| {
                // Count unique candidates per request, not repeated penalty attempts.
                let key: Vec<_> = path
                    .iter()
                    .map(|t| (t.edge, t.from.to_bits(), t.to.to_bits()))
                    .collect();
                if seen.insert(key) {
                    if let Some(reason) = reason {
                        *rejected.entry(reason).or_default() += 1;
                    }
                }
                if reason.is_none() {
                    accepted.push(path.to_vec());
                }
            })
            .expect("route external pair");
        assert_eq!(response.routes.len(), accepted.len() + 1);
        counts[response.routes.len() - 1] += 1;
        let best = response.routes[0].duration_s;
        for (path, route) in accepted.iter().zip(&response.routes[1..]) {
            assert_eq!(
                g.no_uturn(path, route),
                None,
                "external alternative has a U-turn or return"
            );
            assert!(g.locally_optimal(path, &r, best));
            audit_local_windows(&g, path, &r, best);
            assert!(route.duration_s <= best * 1.4 + EPS);
            assert!(crate::Navigator::new(route.clone()).is_ok());
            returned += 1;
        }
    }
    eprintln!("300 pairs; {returned} alternatives returned; requests with 1/2/3 routes: {counts:?}; unique rejected candidates by first failing rule: {rejected:?}; audit elapsed: {:?}", start.elapsed());
}

#[test]
fn an_alternative_may_not_go_out_on_a_public_road_when_the_best_route_stays_inside() {
    // Two parallel ways of similar length between the same ends; one is a public road.
    let data = serde_json::json!({
        "format": "wayfind-graph/1",
        "flags": {"walk": 1, "bike": 2, "steps": 4, "connector": 8, "public": 16},
        "nodes": [[118.9000, 32.1000], [118.9010, 32.1000], [118.9005, 32.1004], [118.9005, 32.0996]],
        "edges": [[0, 2, 60.0, 3, -1], [2, 1, 60.0, 3, -1], [0, 3, 62.0, 19, -1], [3, 1, 62.0, 19, -1]],
        "shapes": [
            [[118.9000, 32.1000], [118.9005, 32.1004]], [[118.9005, 32.1004], [118.9010, 32.1000]],
            [[118.9000, 32.1000], [118.9005, 32.0996]], [[118.9005, 32.0996], [118.9010, 32.1000]]
        ],
        "names": [], "entrances": {}, "targets": {"a": [0], "b": [1]}, "landmarks": [], "stats": {}
    });
    let graph = Graph::from_json(&data.to_string()).unwrap();
    let request: RouteRequest = serde_json::from_value(serde_json::json!({
        "origin": {"id": "a", "type": "target"}, "destination": {"id": "b", "type": "target"}, "mode": "walk", "alternatives": 3
    })).unwrap();
    let routes = graph.route(&request).unwrap().routes;
    assert_eq!(routes.len(), 1);
    assert!((routes[0].distance_m - 120.0).abs() < 1.0);
}
