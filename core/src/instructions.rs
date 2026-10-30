//! Language-specific rendering of semantic maneuvers. Add renderers here for
//! other languages; routing and navigation never depend on rendered text.
use crate::{Maneuver, ManeuverType, Mode, Modifier};

pub fn rounded_distance(metres: f64) -> u64 {
    if !metres.is_finite() || metres <= 0.0 {
        return 0;
    }
    let step = if metres < 100.0 { 10.0 } else { 50.0 };
    ((metres / step).round() * step).max(step) as u64
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

pub fn chinese(maneuver: &Maneuver, mode: Mode) -> String {
    let name = maneuver.way_name.as_deref().filter(|s| !s.is_empty());
    match maneuver.kind {
        ManeuverType::Depart => {
            let action = if mode == Mode::Walk { "走" } else { "前行" };
            let along = name.map_or(String::new(), |n| format!("沿{n}"));
            format!(
                "向{}出发，{along}{action} {} 米",
                direction(maneuver.bearing_after),
                rounded_distance(maneuver.distance_to_next_m)
            )
        }
        ManeuverType::Turn => format!(
            "{}{}",
            turn(maneuver.modifier),
            name.map_or(String::new(), |n| format!("进入{n}"))
        ),
        ManeuverType::Continue => {
            name.map_or("继续直行".into(), |n| format!("继续直行，沿{n}前行"))
        }
        ManeuverType::Stairs => "走楼梯".into(),
        ManeuverType::Arrive => match maneuver.modifier {
            Modifier::Left => "到达目的地，入口在您的左侧".into(),
            Modifier::Right => "到达目的地，入口在您的右侧".into(),
            _ => "到达目的地，入口在您的前方".into(),
        },
    }
}

pub fn chinese_advance(maneuver: &Maneuver, distance_m: f64) -> String {
    let action = match maneuver.kind {
        ManeuverType::Stairs => "走楼梯",
        ManeuverType::Arrive => "到达目的地",
        ManeuverType::Depart => "出发",
        ManeuverType::Continue => "继续直行",
        ManeuverType::Turn => turn(maneuver.modifier),
    };
    format!("前方 {} 米{action}", rounded_distance(distance_m))
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
            instruction: String::new(),
        }
    }
    #[test]
    fn text_and_rounding() {
        let mut m = maneuver(ManeuverType::Depart, Modifier::Ahead);
        assert_eq!(chinese(&m, Mode::Walk), "向西出发，沿甲路走 100 米");
        m.kind = ManeuverType::Turn;
        m.modifier = Modifier::Right;
        assert_eq!(chinese(&m, Mode::Walk), "右转进入甲路");
        assert_eq!(chinese_advance(&m, 52.0), "前方 50 米右转");
        m.kind = ManeuverType::Stairs;
        assert_eq!(chinese(&m, Mode::Walk), "走楼梯");
        m.kind = ManeuverType::Arrive;
        m.modifier = Modifier::Left;
        assert_eq!(chinese(&m, Mode::Walk), "到达目的地，入口在您的左侧");
        for (input, expected) in [
            (0.0, 0),
            (1.0, 10),
            (24.0, 20),
            (96.0, 100),
            (124.0, 100),
            (126.0, 150),
            (176.0, 200),
        ] {
            assert_eq!(rounded_distance(input), expected);
        }
    }
}
