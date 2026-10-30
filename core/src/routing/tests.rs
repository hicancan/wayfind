use super::*;
use crate::{testutil::*, ManeuverType, Modifier, Navigator};

#[test]
fn same_edge_partial_route_with_true_position_connectors() {
    let g = graph(&[(0.0, 0.0), (200.0, 0.0)], &[(0, 1, 200.0, 3, 0)]);
    let route = one(&g, &request(position(50.0, 10.0), position(150.0, -10.0)));
    close(route.distance_m, 120.0, 0.01);
    close(route.duration_s, 96.0, 0.01);
    assert_eq!(route.geometry.len(), 4);
    close(geo::haversine(route.geometry[0], p(50.0, 10.0)), 0.0, 0.001);
    assert_eq!(route.maneuvers.last().unwrap().modifier, Modifier::Right);
    let reverse = one(&g, &request(position(150.0, 10.0), position(50.0, 10.0)));
    assert_eq!(reverse.maneuvers.last().unwrap().modifier, Modifier::Right);
    assert!(Navigator::new(reverse).is_ok());
}

#[test]
fn shapes_are_retained_and_oriented_and_targets_choose_best_entrances() {
    let mut d = data(
        &[(0.0, 0.0), (100.0, 100.0), (300.0, 100.0)],
        &[(0, 1, 200.0, 3, 0), (1, 2, 200.0, 3, 1)],
    );
    d.shapes[0] = vec![p(100.0, 0.0)];
    d.targets.insert("many".into(), vec![0, 1]);
    let g = Graph::from_data(d).unwrap();
    let route = one(&g, &request(target("end"), target("start")));
    assert_eq!(
        route.geometry,
        vec![p(300.0, 100.0), p(100.0, 100.0), p(100.0, 0.0), p(0.0, 0.0)]
    );
    close(
        one(&g, &request(target("end"), target("many"))).distance_m,
        200.0,
        0.001,
    );
    close(
        one(&g, &request(target("many"), target("end"))).distance_m,
        200.0,
        0.001,
    );
    let part = one(&g, &request(position(100.0, 50.0), position(50.0, 0.0)));
    close(part.distance_m, 100.0, 0.01);
    assert!(part
        .geometry
        .iter()
        .any(|pnt| geo::haversine(*pnt, p(100.0, 0.0)) < 0.01));
}

#[test]
fn time_costs_steps_modes_connectors_and_avoid_steps() {
    let g = graph(
        &[(0.0, 0.0), (100.0, 0.0), (200.0, 0.0)],
        &[
            (0, 2, 100.0, 7, -1),
            (0, 1, 110.0, 3, -1),
            (1, 2, 110.0, 3, -1),
        ],
    );
    let mut r = request(target("start"), target("end"));
    let route = one(&g, &r);
    close(route.distance_m, 100.0, 0.001);
    close(route.duration_s, 100.0 / 0.6, 0.001);
    assert!(route
        .maneuvers
        .iter()
        .any(|m| m.kind == ManeuverType::Stairs));
    r.avoid_steps = true;
    close(one(&g, &r).distance_m, 220.0, 0.001);
    r.mode = Mode::Bike;
    r.avoid_steps = false;
    close(one(&g, &r).duration_s, 220.0 / 4.2, 0.001);
    let g = graph(&[(0.0, 0.0), (100.0, 0.0)], &[(0, 1, 100.0, 11, -1)]);
    close(one(&g, &r).duration_s, 80.0, 0.001);
    let g = graph(&[(0.0, 0.0), (100.0, 0.0)], &[(0, 1, 100.0, 1, -1)]);
    assert!(matches!(g.route(&r), Err(Error::NoRoute)));
}

#[test]
fn zero_length_trip_disconnected_and_invalid_requests() {
    let g = graph(
        &[(0.0, 0.0), (100.0, 0.0), (300.0, 0.0)],
        &[(0, 1, 100.0, 3, -1)],
    );
    assert!(matches!(
        g.route(&request(target("start"), target("end"))),
        Err(Error::NoRoute)
    ));
    let zero = one(&g, &request(target("start"), target("start")));
    assert_eq!(zero.geometry.len(), 1);
    assert_eq!(zero.distance_m, 0.0);
    assert!(
        Navigator::new(zero)
            .unwrap()
            .update(0.0, 0.0, 5.0, None, 0.0)
            .unwrap()
            .arrived
    );
    assert!(matches!(
        g.route(&request(target("missing"), target("end"))),
        Err(Error::UnknownTarget(_))
    ));
    let mut r = request(target("start"), target("end"));
    r.alternatives = 4;
    assert!(g.route(&r).is_err());
    r.alternatives = 1;
    r.max_snap_distance_m = -1.0;
    assert!(g.route(&r).is_err());
}

#[test]
fn zero_cost_edges_preserve_connectivity_geometry_and_terminate_on_cycles() {
    let mut d = data(
        &[(0.0, 0.0), (0.0, 0.0), (10.0, 0.0), (100.0, 0.0)],
        &[
            (0, 1, 0.0, 3, -1),
            (1, 2, 0.0, 3, -1),
            (2, 0, 0.0, 3, -1),
            (2, 3, 90.0, 3, -1),
        ],
    );
    d.shapes[1] = vec![p(5.0, 1.0)];
    let g = Graph::from_data(d).unwrap();
    let route = one(&g, &request(target("start"), target("end")));
    close(route.distance_m, 90.0, 1e-6);
    assert_eq!(route.geometry[0], p(0.0, 0.0));
    assert_eq!(route.geometry.last(), Some(&p(100.0, 0.0)));
    assert!(Navigator::new(route).is_ok());
}

#[test]
fn astar_matches_independent_all_pairs_oracle_with_short_declared_edges() {
    let nodes = [
        (0.0, 0.0),
        (100.0, 0.0),
        (200.0, 0.0),
        (0.0, 100.0),
        (100.0, 100.0),
        (200.0, 100.0),
    ];
    let edges = [
        (0, 1, 20.0, 3, -1),
        (1, 2, 60.0, 3, -1),
        (0, 3, 10.0, 3, -1),
        (3, 4, 10.0, 3, -1),
        (4, 5, 10.0, 3, -1),
        (5, 2, 10.0, 3, -1),
        (1, 4, 100.0, 3, -1),
    ];
    let mut d = data(&nodes, &edges);
    for i in 0..nodes.len() {
        d.targets.insert(i.to_string(), vec![i]);
    }
    let g = Graph::from_data(d).unwrap();
    let mut oracle = vec![vec![f64::INFINITY; nodes.len()]; nodes.len()];
    for (i, row) in oracle.iter_mut().enumerate() {
        row[i] = 0.0;
    }
    for (a, b, m, _, _) in edges {
        oracle[a][b] = m / 1.25;
        oracle[b][a] = m / 1.25;
    }
    for k in 0..nodes.len() {
        for i in 0..nodes.len() {
            for j in 0..nodes.len() {
                oracle[i][j] = oracle[i][j].min(oracle[i][k] + oracle[k][j]);
            }
        }
    }
    for (i, row) in oracle.iter().enumerate() {
        for (j, expected) in row.iter().enumerate() {
            close(
                one(&g, &request(target(&i.to_string()), target(&j.to_string()))).duration_s,
                *expected,
                1e-6,
            );
        }
    }
}

#[test]
fn partial_edge_route_can_take_a_faster_detour_instead_of_the_direct_slice() {
    let mut d = data(
        &[(0.0, 0.0), (100.0, 0.0), (50.0, 50.0)],
        &[
            (0, 1, 1000.0, 3, -1),
            (0, 2, 50.0, 3, -1),
            (2, 1, 50.0, 3, -1),
        ],
    );
    d.targets.insert("far".into(), vec![1]);
    let g = Graph::from_data(d).unwrap();
    let route = one(&g, &request(position(20.0, 0.0), position(80.0, 0.0)));
    close(route.distance_m, 500.0, 0.1);
    assert!(route
        .geometry
        .iter()
        .any(|point| geo::haversine(*point, p(50.0, 50.0)) < 0.01));
    assert!(Navigator::new(route).is_ok());
}

#[test]
fn multi_source_and_destination_targets_choose_the_global_best_pair() {
    let mut d = data(
        &[(0.0, 0.0), (100.0, 0.0), (200.0, 0.0), (300.0, 0.0)],
        &[
            (0, 1, 10.0, 3, -1),
            (1, 2, 100.0, 3, -1),
            (2, 3, 20.0, 3, -1),
        ],
    );
    d.targets.insert("origins".into(), vec![0, 3]);
    d.targets.insert("destinations".into(), vec![2, 1]);
    let g = Graph::from_data(d).unwrap();
    let route = one(&g, &request(target("origins"), target("destinations")));
    close(route.distance_m, 10.0, 0.001);
    assert_eq!(route.geometry, vec![p(0.0, 0.0), p(100.0, 0.0)]);
    close(
        one(&g, &request(target("destinations"), target("origins"))).distance_m,
        10.0,
        0.001,
    );
}

#[test]
fn alternatives_diversity_limit_and_unpenalized_duration() {
    let mut d = data(
        &[(0.0, 0.0), (100.0, 50.0), (100.0, -50.0), (200.0, 0.0)],
        &[
            (0, 3, 200.0, 3, -1),
            (0, 1, 110.0, 3, -1),
            (1, 3, 110.0, 3, -1),
            (0, 2, 120.0, 3, -1),
            (2, 3, 120.0, 3, -1),
        ],
    );
    let g = Graph::from_data(d.clone()).unwrap();
    let mut r = request(target("start"), target("end"));
    r.alternatives = 3;
    let routes = g.route(&r).unwrap().routes;
    assert_eq!(routes.len(), 3);
    for (route, distance) in routes.iter().zip([200.0, 220.0, 240.0]) {
        close(route.distance_m, distance, 0.001);
        close(route.duration_s, distance / 1.25, 0.001);
    }
    d.edges[3].2 = 180.0;
    d.edges[4].2 = 180.0;
    assert_eq!(
        Graph::from_data(d).unwrap().route(&r).unwrap().routes.len(),
        2
    );
    let g = graph(
        &[(0.0, 0.0), (90.0, 0.0), (95.0, 5.0), (100.0, 0.0)],
        &[
            (0, 1, 90.0, 3, -1),
            (1, 3, 10.0, 3, -1),
            (1, 2, 7.0, 3, -1),
            (2, 3, 7.0, 3, -1),
        ],
    );
    assert_eq!(g.route(&r).unwrap().routes.len(), 1);
}

#[test]
fn actual_geometry_left_right_sharp_and_slight() {
    for (after, expected) in [
        (60.0_f64, Modifier::SlightLeft),
        (0.0, Modifier::Left),
        (300.0, Modifier::SharpLeft),
        (120.0, Modifier::SlightRight),
        (180.0, Modifier::Right),
        (240.0, Modifier::SharpRight),
    ] {
        let a = after.to_radians();
        let g = graph(
            &[
                (0.0, 0.0),
                (100.0, 0.0),
                (100.0 + 100.0 * a.sin(), 100.0 * a.cos()),
            ],
            &[(0, 1, 100.0, 3, 0), (1, 2, 100.0, 3, 0)],
        );
        let route = one(&g, &request(target("start"), target("end")));
        assert_eq!(route.maneuvers[1].kind, ManeuverType::Turn);
        assert_eq!(route.maneuvers[1].modifier, expected);
        close(route.maneuvers[1].distance_to_next_m, 100.0, 0.001);
    }
}

#[test]
fn straight_suppression_name_change_and_ambiguous_fork() {
    let nodes = [(0.0, 0.0), (100.0, 0.0), (200.0, 0.0)];
    let mut d = data(&nodes, &[(0, 1, 100.0, 3, 0), (1, 2, 100.0, 3, 0)]);
    let r = request(target("start"), target("end"));
    assert_eq!(
        one(&Graph::from_data(d.clone()).unwrap(), &r)
            .maneuvers
            .len(),
        2
    );
    d.edges[1].4 = 1;
    let route = one(&Graph::from_data(d.clone()).unwrap(), &r);
    assert_eq!(route.maneuvers[1].kind, ManeuverType::Continue);
    assert_eq!(route.maneuvers[1].way_name.as_deref(), Some("乙路"));
    d.edges[1].4 = 0;
    d.nodes.push(p(200.0, 30.0));
    d.edges.push((1, 3, 110.0, 3, 0));
    d.shapes.push(vec![]);
    assert_eq!(
        one(&Graph::from_data(d.clone()).unwrap(), &r).maneuvers[1].kind,
        ManeuverType::Continue
    );
    d.nodes[3] = p(100.0, 100.0);
    assert_eq!(one(&Graph::from_data(d).unwrap(), &r).maneuvers.len(), 2);
}
