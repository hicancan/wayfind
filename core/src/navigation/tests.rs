use super::*;
use crate::{testutil::*, Graph, ManeuverType};
use std::collections::HashSet;

fn update(nav: &mut Navigator, x: f64, y: f64, ms: f64) -> NavigationUpdate {
    let pos = p(x, y);
    nav.update(pos[0], pos[1], 4.0, Some(90.0), ms).unwrap()
}

#[test]
fn simulated_walk_with_jitter_progress_prompts_eta_and_arrival() {
    let g = line();
    let route = one(&g, &request(target("start"), target("end")));
    let mut nav = Navigator::new(route).unwrap();
    let mut spoken = HashSet::new();
    let mut progress = 0.0;
    let mut stages = Vec::new();
    let mut turn_current = false;
    for tick in 0..=302 {
        let distance = (tick as f64).min(300.0);
        let jitter = if tick % 2 == 0 { 1.8 } else { -1.8 };
        let (x, y) = if distance <= 150.0 {
            (distance, jitter)
        } else {
            (150.0 + jitter, distance - 150.0)
        };
        let out = update(&mut nav, x, y, tick as f64 * 800.0);
        assert!(out.distance_travelled_m + 1e-6 >= progress);
        progress = out.distance_travelled_m;
        assert!(!out.off_route);
        close(out.eta_s, out.distance_remaining_m / 1.25, 0.01);
        if out.current.maneuver.kind == ManeuverType::Turn {
            turn_current = true;
        }
        for prompt in out.voice_prompts {
            assert!(spoken.insert((prompt.maneuver_index, format!("{:?}", prompt.stage))));
            if prompt.maneuver_index == 1 {
                stages.push(prompt.stage);
            }
        }
    }
    assert!(turn_current);
    assert_eq!(
        stages,
        vec![PromptStage::Early, PromptStage::Approach, PromptStage::Now]
    );
    let out = update(&mut nav, 150.0, 150.0, 250_000.0);
    assert!(out.arrived);
    assert!(out.voice_prompts.is_empty());
    assert!(out.next.is_none());
    assert_eq!(out.distance_remaining_m, 0.0);
    assert_eq!(out.current.maneuver.kind, ManeuverType::Arrive);
}

#[test]
fn off_route_consecutive_fixes_timeout_accuracy_and_recovery() {
    let g = graph(&[(0.0, 0.0), (500.0, 0.0)], &[(0, 1, 500.0, 3, -1)]);
    let route = one(&g, &request(target("start"), target("end")));
    let mut nav = Navigator::new(route.clone()).unwrap();
    update(&mut nav, 40.0, 0.0, 0.0);
    assert!(!update(&mut nav, 45.0, 40.0, 1000.0).off_route);
    assert!(!update(&mut nav, 48.0, 40.0, 2000.0).off_route);
    let off = update(&mut nav, 50.0, 40.0, 3000.0);
    assert!(off.off_route);
    assert!(off.voice_prompts.is_empty());
    close(off.distance_travelled_m, 40.0, 0.01);
    assert!(!update(&mut nav, 52.0, 0.0, 4000.0).off_route);
    assert!(!update(&mut nav, 55.0, 40.0, 5000.0).off_route);
    assert!(!update(&mut nav, 55.0, 0.0, 5500.0).off_route);
    assert!(!update(&mut nav, 55.0, 40.0, 6000.0).off_route);
    assert!(update(&mut nav, 55.0, 40.0, 14_000.0).off_route);
    let mut nav = Navigator::new(route).unwrap();
    let pos = p(40.0, 35.0);
    for t in 0..5 {
        assert!(
            !nav.update(pos[0], pos[1], 20.0, None, t as f64 * 5000.0)
                .unwrap()
                .off_route
        );
    }
}

#[test]
fn bike_early_threshold_is_200_metres_and_no_repeat_when_starting_close() {
    let g = graph(
        &[(0.0, 0.0), (300.0, 0.0), (300.0, 300.0)],
        &[(0, 1, 300.0, 3, -1), (1, 2, 300.0, 3, -1)],
    );
    let mut r = request(target("start"), target("end"));
    r.mode = Mode::Bike;
    let route = one(&g, &r);
    let mut nav = Navigator::new(route.clone()).unwrap();
    let out = update(&mut nav, 110.0, 0.0, 0.0);
    assert_eq!(out.voice_prompts.len(), 1);
    assert_eq!(out.voice_prompts[0].stage, PromptStage::Early);
    assert!(update(&mut nav, 110.0, 1.0, 1000.0)
        .voice_prompts
        .is_empty());
    let mut nav = Navigator::new(route).unwrap();
    let out = update(&mut nav, 295.0, 0.0, 0.0);
    assert_eq!(out.voice_prompts.len(), 1);
    assert_eq!(out.voice_prompts[0].stage, PromptStage::Now);
    assert!(update(&mut nav, 294.0, 0.0, 1000.0)
        .voice_prompts
        .iter()
        .all(|p| p.maneuver_index != 1));
}

#[test]
fn crossing_route_does_not_teleport_and_progress_cannot_reverse() {
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
    let mut nav = Navigator::new(one(&g, &request(target("start"), target("end")))).unwrap();
    close(
        update(&mut nav, -10.0, 0.0, 0.0).distance_travelled_m,
        90.0,
        0.01,
    );
    assert!(update(&mut nav, 1.0, 1.0, 1000.0).distance_travelled_m < 110.0);
    let progress = update(&mut nav, 5.0, 0.0, 2000.0).distance_travelled_m;
    assert!(update(&mut nav, -5.0, 0.0, 3000.0).distance_travelled_m >= progress);
}

#[test]
fn eta_uses_remaining_segment_times_and_arrival_uses_the_true_endpoint() {
    let g = graph(
        &[(0.0, 0.0), (100.0, 0.0), (100.0, 60.0)],
        &[(0, 1, 100.0, 3, -1), (1, 2, 60.0, 5, -1)],
    );
    let route = one(&g, &request(target("start"), target("end")));
    let mut nav = Navigator::new(route).unwrap();
    close(update(&mut nav, 80.0, 0.0, 0.0).eta_s, 116.0, 0.01);
    close(update(&mut nav, 100.0, 30.0, 40_000.0).eta_s, 50.0, 0.01);
    let g = graph(&[(0.0, 0.0), (200.0, 0.0)], &[(0, 1, 200.0, 3, -1)]);
    let route = one(&g, &request(position(0.0, 0.0), position(180.0, 40.0)));
    let mut nav = Navigator::new(route).unwrap();
    assert!(!update(&mut nav, 180.0, 0.0, 0.0).arrived);
    assert!(update(&mut nav, 180.0, 28.0, 30_000.0).arrived);
}

#[test]
fn invalid_updates_and_serialized_routes_return_errors_without_changing_session() {
    let g = line();
    let route = one(&g, &request(target("start"), target("end")));
    let json = serde_json::to_string(&route).unwrap();
    let mut nav = Navigator::from_json(&json).unwrap();
    assert!(nav.update(f64::NAN, 0.0, 5.0, None, 0.0).is_err());
    assert!(nav.update(0.0, 0.0, -1.0, None, 0.0).is_err());
    assert!(nav.update(0.0, 0.0, 5.0, Some(f64::NAN), 0.0).is_err());
    assert!(nav.update(0.0, 0.0, 5.0, None, 0.5).is_err());
    update(&mut nav, 10.0, 0.0, 1000.0);
    assert!(nav.update(0.0, 0.0, 5.0, None, 999.0).is_err());
    assert!(serde_json::to_string(&nav).is_ok());
    for field in ["geometry", "geometry_offsets_m", "segments", "maneuvers"] {
        let mut value = serde_json::to_value(&route).unwrap();
        value[field] = serde_json::json!([]);
        assert!(Navigator::from_json(&value.to_string()).is_err());
    }
    let mut bad = route.clone();
    bad.segments[0].geometry_end = usize::MAX;
    assert!(Navigator::new(bad).is_err());
    let mut bad = route.clone();
    bad.geometry_offsets_m[1] = -1.0;
    assert!(Navigator::new(bad).is_err());
    let mut bad = route;
    bad.maneuvers[0].bearing_before = f64::INFINITY;
    assert!(Navigator::new(bad).is_err());
}

#[test]
fn prompts_share_landmark_text_and_keep_ten_metres_apart_except_arrival() {
    let mut d = data(
        &[(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)],
        &[(0, 1, 100.0, 3, -1), (1, 2, 100.0, 11, -1)],
    );
    d.landmarks.push(crate::Landmark {
        name: "甲广场".into(),
        at: p(100.0, -10.0),
    });
    let r = one(
        &Graph::from_data(d).unwrap(),
        &request(target("start"), target("end")),
    );
    let mut nav = Navigator::new(r.clone()).unwrap();
    let mut last_progress = None;
    let mut texts = vec![];
    for metre in 0..=200 {
        let (x, y) = if metre <= 100 {
            (metre as f64, 0.0)
        } else {
            (100.0, (metre - 100) as f64)
        };
        let out = update(&mut nav, x, y, metre as f64 * 1000.0);
        for prompt in out.voice_prompts {
            if prompt.stage != PromptStage::Arrive {
                if let Some(last) = last_progress {
                    assert!(out.distance_travelled_m - last >= 10.0 - 1e-6);
                }
                last_progress = Some(out.distance_travelled_m);
            }
            texts.push(prompt.text);
        }
    }
    assert!(texts.iter().any(|t| t == "90 米后在甲广场北侧左转"));
    assert!(texts.iter().any(|t| t == "前方在甲广场北侧左转"));
    assert!(texts.iter().any(|t| t == "已到达end附近，入口在您的左侧"));
    // Arrival may speak even when the departure/previous prompt was <10 m ago.
    let short = graph(&[(0.0, 0.0), (20.0, 0.0)], &[(0, 1, 20.0, 3, -1)]);
    let mut nav = Navigator::new(one(&short, &request(target("start"), target("end")))).unwrap();
    assert_eq!(
        update(&mut nav, 0.0, 0.0, 0.0).voice_prompts[0].stage,
        PromptStage::Depart
    );
    assert_eq!(
        update(&mut nav, 6.0, 0.0, 1000.0).voice_prompts[0].stage,
        PromptStage::Arrive
    );
}

#[test]
fn malformed_landmark_and_folded_actions_are_rejected_across_route_json() {
    let r = one(&line(), &request(target("start"), target("end")));
    for (field, value) in [
        (
            "landmark",
            serde_json::json!({"name":"甲广场","at":[0,91],"relation":"ahead"}),
        ),
        (
            "landmark",
            serde_json::json!({"name":" ","at":[0,0],"relation":"ahead"}),
        ),
        (
            "following_actions",
            serde_json::json!([{"kind":"turn","modifier":"right","offset_m":-1}]),
        ),
        (
            "following_actions",
            serde_json::json!([{"kind":"stairs","modifier":"ahead","offset_m":999}]),
        ),
        (
            "following_actions",
            serde_json::json!([{"kind":"arrive","modifier":"ahead","offset_m":10}]),
        ),
    ] {
        let mut value_route = serde_json::to_value(&r).unwrap();
        value_route["maneuvers"][0][field] = value;
        assert!(Navigator::from_json(&value_route.to_string()).is_err());
    }
    // Older serialized routes still deserialize when optional semantics are absent.
    let mut value = serde_json::to_value(&r).unwrap();
    for m in value["maneuvers"].as_array_mut().unwrap() {
        for field in [
            "target_id",
            "landmark",
            "leads_to_arrival",
            "following_actions",
        ] {
            m.as_object_mut().unwrap().remove(field);
        }
    }
    assert!(Navigator::from_json(&value.to_string()).is_ok());
}
