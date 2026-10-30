//! Language-specific rendering. Routing and navigation never depend on rendered text.
use crate::{LandmarkRelation, Maneuver, ManeuverLandmark, ManeuverType, Mode, Modifier};

pub fn rounded_distance(metres: f64) -> u64 {
    if !metres.is_finite() || metres <= 0.0 {
        return 0;
    }
    let step = if metres < 100.0 { 10.0 } else { 50.0 };
    ((metres / step).round() * step).max(step) as u64
}
fn spoken_distance(metres: f64) -> u64 {
    rounded_distance(metres).max(10)
}
fn direction(bearing: f64) -> &'static str {
    ["北", "东北", "东", "东南", "南", "西南", "西", "西北"]
        [((bearing.rem_euclid(360.0) + 22.5) / 45.0) as usize % 8]
}
fn turn(modifier: Modifier) -> &'static str {
    match modifier {
        Modifier::Ahead => "直行",
        Modifier::SlightLeft => "稍向左转",
        Modifier::Left => "左转",
        Modifier::SharpLeft => "向左急转",
        Modifier::SlightRight => "稍向右转",
        Modifier::Right => "右转",
        Modifier::SharpRight => "向右急转",
    }
}
fn action(kind: ManeuverType, modifier: Modifier) -> &'static str {
    match kind {
        ManeuverType::Turn => turn(modifier),
        ManeuverType::Stairs => "走楼梯",
        ManeuverType::Continue => "沿路直行",
        ManeuverType::Depart => "出发",
        ManeuverType::Arrive => "到达",
    }
}
fn turn_action(
    modifier: Modifier,
    way_name: Option<&str>,
    landmark: Option<&ManeuverLandmark>,
) -> String {
    let prefix = landmark.map_or(String::new(), |landmark| {
        let name = &landmark.name;
        match landmark.relation {
            LandmarkRelation::Ahead => format!("在{name}前"),
            LandmarkRelation::Passed => format!("过{name}后"),
            relation => {
                let side = match relation {
                    LandmarkRelation::North => "北",
                    LandmarkRelation::East => "东",
                    LandmarkRelation::South => "南",
                    LandmarkRelation::West => "西",
                    _ => "",
                };
                format!("在{name}{side}侧")
            }
        }
    });
    let road = way_name
        .filter(|s| !s.is_empty())
        .map_or(String::new(), |n| format!("进入{n}"));
    format!("{prefix}{}{road}", turn(modifier))
}
fn arrival(m: &Maneuver, nearby: bool) -> String {
    let destination = m.target_id.as_deref().unwrap_or("目的地");
    let side = match m.modifier {
        Modifier::Left => "左侧",
        Modifier::Right => "右侧",
        _ => "前方",
    };
    let start = if nearby { "已到达" } else { "到达" };
    let suffix = if nearby { "附近" } else { "" };
    format!("{start}{destination}{suffix}，入口在您的{side}")
}
pub fn chinese(m: &Maneuver, mode: Mode) -> String {
    if m.kind == ManeuverType::Arrive {
        return arrival(m, false);
    }
    let name = m.way_name.as_deref().filter(|s| !s.is_empty());
    let walk = if mode == Mode::Walk { "走" } else { "前行" };
    let mut text = match m.kind {
        ManeuverType::Depart => {
            let from = m
                .target_id
                .as_ref()
                .map_or("出发".into(), |id| format!("从{id}出发"));
            let along = name.map_or(String::new(), |n| format!("沿{n}"));
            format!("{from}，{along}向{}", direction(m.bearing_after))
        }
        ManeuverType::Turn => turn_action(m.modifier, m.way_name.as_deref(), m.landmark.as_ref()),
        ManeuverType::Continue => name.map_or("沿路直行".into(), |n| {
            format!("沿{n}向{}", direction(m.bearing_after))
        }),
        ManeuverType::Stairs => "走楼梯".into(),
        ManeuverType::Arrive => return arrival(m, false),
    };
    let mut last_offset = m.offset_m;
    for following in &m.following_actions {
        let next_action = if following.kind == ManeuverType::Turn {
            turn_action(
                following.modifier,
                following.way_name.as_deref(),
                following.landmark.as_ref(),
            )
        } else {
            action(following.kind, following.modifier).into()
        };
        let gap = following.offset_m - last_offset;
        if gap <= 1e-6 {
            text.push_str(&format!("，{next_action}"));
        } else {
            let distance = spoken_distance(gap);
            text.push_str(&format!("，前行 {distance} 米后{next_action}"));
        }
        last_offset = following.offset_m;
    }
    let remaining = m.distance_to_next_m - (last_offset - m.offset_m);
    if remaining <= 1e-6 {
        if m.leads_to_arrival {
            text.push_str("，即可到达");
        }
        return text;
    }
    let distance = spoken_distance(remaining);
    if m.leads_to_arrival {
        text.push_str(&format!("，前行 {distance} 米到达"));
    } else if m.kind == ManeuverType::Depart && m.following_actions.is_empty() {
        text.push_str(&format!("{walk} {distance} 米"));
    } else {
        let join = if m.kind == ManeuverType::Continue && name.is_some() {
            ""
        } else {
            "，"
        };
        text.push_str(&format!("{join}{walk} {distance} 米"));
    }
    text
}
pub fn chinese_advance(m: &Maneuver, distance_m: f64) -> String {
    let text = if m.kind == ManeuverType::Turn {
        turn_action(m.modifier, m.way_name.as_deref(), m.landmark.as_ref())
    } else if m.kind == ManeuverType::Arrive {
        format!("到达{}", m.target_id.as_deref().unwrap_or("目的地"))
    } else {
        action(m.kind, m.modifier).into()
    };
    let mut prompt = if distance_m <= 8.0 {
        format!("前方{text}")
    } else {
        format!("{} 米后{text}", spoken_distance(distance_m))
    };
    for following in &m.following_actions {
        let next_action = if following.kind == ManeuverType::Turn {
            turn_action(
                following.modifier,
                following.way_name.as_deref(),
                following.landmark.as_ref(),
            )
        } else {
            action(following.kind, following.modifier).into()
        };
        prompt.push_str(&format!("，随后{next_action}"));
    }
    prompt
}
pub fn chinese_arrived(m: &Maneuver) -> String {
    arrival(m, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn maneuver(kind: ManeuverType, modifier: Modifier) -> Maneuver {
        Maneuver {
            kind,
            modifier,
            position: [0.0, 0.0],
            bearing_before: 270.0,
            bearing_after: 270.0,
            offset_m: 0.0,
            distance_to_next_m: 120.0,
            way_name: Some("甲路".into()),
            target_id: None,
            landmark: None,
            leads_to_arrival: false,
            following_actions: vec![],
            instruction: String::new(),
        }
    }
    #[test]
    fn text_and_rounding() {
        let mut m = maneuver(ManeuverType::Depart, Modifier::Ahead);
        m.target_id = Some("start".into());
        assert_eq!(chinese(&m, Mode::Walk), "从start出发，沿甲路向西走 100 米");
        m.target_id = None;
        assert_eq!(chinese(&m, Mode::Walk), "出发，沿甲路向西走 100 米");
        m.kind = ManeuverType::Turn;
        m.modifier = Modifier::Right;
        m.way_name = None;
        assert_eq!(chinese(&m, Mode::Walk), "右转，走 100 米");
        assert_eq!(chinese_advance(&m, 52.0), "50 米后右转");
        assert_eq!(chinese_advance(&m, 4.0), "前方右转");
        m.leads_to_arrival = true;
        m.distance_to_next_m = 39.0;
        assert_eq!(chinese(&m, Mode::Walk), "右转，前行 40 米到达");
        m.leads_to_arrival = false;
        m.kind = ManeuverType::Continue;
        m.way_name = Some("甲路".into());
        m.distance_to_next_m = 302.0;
        m.bearing_after = 0.0;
        assert_eq!(chinese(&m, Mode::Walk), "沿甲路向北走 300 米");
        m.kind = ManeuverType::Stairs;
        m.distance_to_next_m = 20.0;
        assert_eq!(chinese(&m, Mode::Walk), "走楼梯，走 20 米");
        m.kind = ManeuverType::Arrive;
        m.modifier = Modifier::Left;
        m.target_id = Some("end".into());
        assert_eq!(chinese(&m, Mode::Walk), "到达end，入口在您的左侧");
        assert_eq!(chinese_arrived(&m), "已到达end附近，入口在您的左侧");
        for (input, expected) in [
            (0.0, 0),
            (1.0, 10),
            (24.0, 20),
            (96.0, 100),
            (100.0, 100),
            (124.0, 100),
            (126.0, 150),
            (176.0, 200),
        ] {
            assert_eq!(rounded_distance(input), expected);
        }
        for kind in [
            ManeuverType::Depart,
            ManeuverType::Turn,
            ManeuverType::Continue,
            ManeuverType::Stairs,
        ] {
            m.kind = kind;
            for distance in [0.0, 0.1, 4.0, 10.0] {
                m.distance_to_next_m = distance;
                assert!(!chinese(&m, Mode::Walk).contains(" 0 米"));
            }
        }
    }
}
