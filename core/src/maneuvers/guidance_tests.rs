use super::*;
use crate::{testutil::*, Landmark};

fn with_landmark(x: f64, y: f64) -> Route {
    let mut d = data(
        &[(0.0, 0.0), (100.0, 0.0), (100.0, -100.0)],
        &[(0, 1, 100.0, 3, -1), (1, 2, 100.0, 3, -1)],
    );
    d.landmarks.push(Landmark {
        name: "甲广场".into(),
        at: p(x, y),
    });
    one(
        &Graph::from_data(d).unwrap(),
        &request(target("start"), target("end")),
    )
}

#[test]
fn landmark_ahead_passed_and_beside_use_the_incoming_direction() {
    for (x, y, relation, text) in [
        (120.0, 0.0, LandmarkRelation::Ahead, "在甲广场前右转"),
        (80.0, 0.0, LandmarkRelation::Passed, "过甲广场后右转"),
        (100.0, 20.0, LandmarkRelation::South, "在甲广场南侧右转"),
        (100.0, -20.0, LandmarkRelation::North, "在甲广场北侧右转"),
    ] {
        let r = with_landmark(x, y);
        let m = &r.maneuvers[1];
        assert_eq!(m.landmark.as_ref().unwrap().relation, relation);
        assert!(m.instruction.starts_with(text), "{}", m.instruction);
        assert!(m.instruction.contains(" 米"));
        assert_eq!(m.landmark.as_ref().unwrap().at, p(x, y));
        assert!(r.maneuvers[0].landmark.is_none());
        assert!(r.maneuvers.last().unwrap().landmark.is_none());
    }
    // Rotating the approach north changes ahead/passed but not map sides.
    let mut d = data(
        &[(0.0, 0.0), (0.0, 100.0), (100.0, 100.0)],
        &[(0, 1, 100.0, 3, -1), (1, 2, 100.0, 3, -1)],
    );
    d.landmarks.push(Landmark {
        name: "甲广场".into(),
        at: p(-20.0, 100.0),
    });
    let g = Graph::from_data(d).unwrap();
    let r = one(&g, &request(target("start"), target("end")));
    assert!(r.maneuvers[1].instruction.starts_with("在甲广场东侧右转"));
    assert_eq!(
        instructions::chinese_advance(&r.maneuvers[1], 100.0),
        "100 米后在甲广场东侧右转"
    );
    let r = one(&g, &request(target("end"), target("start")));
    assert!(r.maneuvers[1].instruction.starts_with("在甲广场前左转"));
}

#[test]
fn landmarks_respect_radius_nearest_and_fuller_name_ties() {
    assert!(with_landmark(135.1, 0.0).maneuvers[1].landmark.is_none());
    assert!(with_landmark(134.9, 0.0).maneuvers[1].landmark.is_some());
    let mut d = data(
        &[(0.0, 0.0), (100.0, 0.0), (100.0, -100.0)],
        &[(0, 1, 100.0, 3, -1), (1, 2, 100.0, 3, -1)],
    );
    d.landmarks = vec![
        Landmark {
            name: "甲".into(),
            at: p(110.0, 0.0),
        },
        Landmark {
            name: "甲广场".into(),
            at: p(110.0, 0.0),
        },
        Landmark {
            name: "乙广场的很长名字".into(),
            at: p(120.0, 0.0),
        },
    ];
    let r = one(
        &Graph::from_data(d.clone()).unwrap(),
        &request(target("start"), target("end")),
    );
    assert_eq!(r.maneuvers[1].landmark.as_ref().unwrap().name, "甲广场");
    d.landmarks.reverse();
    let r2 = one(
        &Graph::from_data(d).unwrap(),
        &request(target("start"), target("end")),
    );
    assert_eq!(r.maneuvers[1].instruction, r2.maneuvers[1].instruction);
}

#[test]
fn departure_names_origin_and_arrival_names_destination_and_connector_side() {
    for (y, side) in [(20.0, Modifier::Left), (-20.0, Modifier::Right)] {
        let mut d = data(
            &[(0.0, 0.0), (100.0, 0.0), (100.0, y)],
            &[(0, 1, 100.0, 3, 0), (1, 2, 20.0, 11, -1)],
        );
        d.landmarks = vec![
            Landmark {
                name: "start".into(),
                at: p(0.0, 0.0),
            },
            Landmark {
                name: "end".into(),
                at: p(100.0, y),
            },
        ];
        let g = Graph::from_data(d).unwrap();
        let r = one(&g, &request(target("start"), target("end")));
        assert_eq!(r.maneuvers[0].target_id.as_deref(), Some("start"));
        assert!(r.maneuvers[0]
            .instruction
            .starts_with("从start出发，沿甲路向东走"));
        let last = r.maneuvers.last().unwrap();
        assert_eq!(last.target_id.as_deref(), Some("end"));
        assert_eq!(last.modifier, side);
        assert!(last.instruction.starts_with("到达end，入口在您的"));
        assert!(r.maneuvers[0].landmark.is_none() && last.landmark.is_none());
        let position_route = one(&g, &request(position(0.0, 0.0), position(100.0, y)));
        assert!(position_route.maneuvers[0]
            .instruction
            .starts_with("出发，"));
    }
}

#[test]
fn opposite_tiny_jogs_cancel_without_losing_the_path_or_distances() {
    for gap in [4.0, 7.9, 20.0] {
        let g = graph(
            &[(0.0, 0.0), (100.0, 0.0), (100.0, gap), (200.0, gap)],
            &[
                (0, 1, 100.0, 3, -1),
                (1, 2, gap, 3, -1),
                (2, 3, 100.0, 3, -1),
            ],
        );
        let r = one(&g, &request(target("start"), target("end")));
        assert_eq!(r.maneuvers.len(), 2, "gap {gap}");
        assert_eq!(r.geometry.len(), 4);
        close(r.distance_m, 200.0 + gap, 1e-6);
        close(r.maneuvers[0].distance_to_next_m, r.distance_m, 1e-6);
    }
}

#[test]
fn cancelling_jog_with_residual_angle_becomes_one_slight_turn() {
    for residual in [25.0_f64, 30.0, 35.0] {
        let a = residual.to_radians();
        let g = graph(
            &[
                (0.0, 0.0),
                (100.0, 0.0),
                (100.0, 7.0),
                (100.0 + 100.0 * a.cos(), 7.0 + 100.0 * a.sin()),
            ],
            &[
                (0, 1, 100.0, 3, -1),
                (1, 2, 7.0, 3, -1),
                (2, 3, 100.0, 3, -1),
            ],
        );
        let r = one(&g, &request(target("start"), target("end")));
        assert_eq!(r.maneuvers.len(), 3);
        assert_eq!(r.maneuvers[1].modifier, Modifier::SlightLeft);
    }
}

#[test]
fn folded_turn_retains_its_landmark_and_excludes_the_departure_target() {
    let mut d = data(
        &[(0.0, 0.0), (5.0, 0.0), (5.0, -100.0)],
        &[(0, 1, 5.0, 11, -1), (1, 2, 100.0, 3, 0)],
    );
    d.landmarks = vec![
        Landmark {
            name: "start".into(),
            at: p(5.0, -1.0),
        },
        Landmark {
            name: "甲广场".into(),
            at: p(5.0, 10.0),
        },
    ];
    let r = one(
        &Graph::from_data(d).unwrap(),
        &request(target("start"), target("end")),
    );
    assert_eq!(r.maneuvers.len(), 2);
    let action = &r.maneuvers[0].following_actions[0];
    assert_eq!(action.landmark.as_ref().unwrap().name, "甲广场");
    assert_eq!(action.way_name.as_deref(), Some("甲路"));
    assert!(r.maneuvers[0]
        .instruction
        .contains("在甲广场南侧右转进入甲路"));
    assert!(r.maneuvers[0].landmark.is_none());
}

#[test]
fn named_or_long_crossings_and_non_cancelling_turns_are_retained() {
    for (gap, name) in [(30.0, -1), (20.0, 0)] {
        let g = graph(
            &[(0.0, 0.0), (100.0, 0.0), (100.0, gap), (200.0, gap)],
            &[
                (0, 1, 100.0, 3, -1),
                (1, 2, gap, 3, name),
                (2, 3, 100.0, 3, -1),
            ],
        );
        assert_eq!(
            one(&g, &request(target("start"), target("end")))
                .maneuvers
                .len(),
            4
        );
    }
    let g = graph(
        &[(0.0, 0.0), (100.0, 0.0), (100.0, -5.0), (0.0, -5.0)],
        &[
            (0, 1, 100.0, 3, -1),
            (1, 2, 5.0, 3, -1),
            (2, 3, 100.0, 3, -1),
        ],
    );
    let r = one(&g, &request(target("start"), target("end")));
    assert_eq!(r.maneuvers.len(), 3);
    assert_eq!(r.maneuvers[1].modifier, Modifier::Right);
    assert_eq!(
        r.maneuvers[1].following_actions[0].modifier,
        Modifier::Right
    );
    assert!(r.maneuvers[1].instruction.contains("米后右转"));
}

#[test]
fn entrance_spurs_and_short_joins_use_sixty_degree_threshold() {
    for (angle, retained) in [
        (30.0_f64, false),
        (59.0, false),
        (60.0, true),
        (61.0, true),
        (90.0, true),
    ] {
        for flags in [3, 11] {
            let a = (90.0 + angle).to_radians();
            let mut d = data(
                &[
                    (0.0, 0.0),
                    (100.0, 0.0),
                    (100.0 + 100.0 * a.sin(), 100.0 * a.cos()),
                ],
                &[(0, 1, 100.0, 3, 0), (1, 2, 100.0, 3, 0)],
            );
            d.nodes.push(p(100.0, 4.0));
            d.edges
                .push((1, 3, if flags == 11 { 100.0 } else { 4.0 }, flags, -1));
            d.shapes.push(vec![]);
            let r = one(
                &Graph::from_data(d).unwrap(),
                &request(target("start"), target("end")),
            );
            assert_eq!(
                r.maneuvers.iter().any(|m| m.kind == ManeuverType::Turn),
                retained
            );
        }
    }
}

#[test]
fn close_stairs_and_turns_are_spoken_together_and_never_lost() {
    for initial_steps in [false, true] {
        let g = graph(
            &[(0.0, 0.0), (100.0, 0.0), (100.0, -5.0), (100.0, -105.0)],
            &[
                (0, 1, 100.0, if initial_steps { 5 } else { 3 }, -1),
                (1, 2, 5.0, 3, -1),
                (2, 3, 100.0, 5, -1),
            ],
        );
        let r = one(&g, &request(target("start"), target("end")));
        for pair in r.maneuvers.windows(2) {
            assert!(
                pair[1].kind == ManeuverType::Arrive || pair[1].offset_m - pair[0].offset_m >= 10.0
            );
        }
        let m = r
            .maneuvers
            .iter()
            .find(|m| m.kind == ManeuverType::Turn)
            .unwrap();
        assert!(m
            .following_actions
            .iter()
            .any(|a| a.kind == ManeuverType::Stairs));
        assert!(m.instruction.contains("走楼梯"));
        assert!(instructions::chinese_advance(m, 5.0).contains("走楼梯"));
        if initial_steps {
            assert!(r.maneuvers[0].instruction.contains("走楼梯"));
            assert!(!r.maneuvers[0].instruction.contains("前行 10 米后走楼梯"));
        }
        assert!(crate::Navigator::new(r).is_ok());
    }
}

#[test]
fn zero_length_trip_has_no_zero_metre_instruction() {
    let g = line();
    let r = one(&g, &request(target("start"), target("start")));
    assert_eq!(r.distance_m, 0.0);
    for m in r.maneuvers {
        assert!(!m.instruction.contains("0 米"));
    }
}
