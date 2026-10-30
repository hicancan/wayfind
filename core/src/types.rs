use serde::{Deserialize, Serialize};

/// `[longitude, latitude]` in WGS84 degrees.
pub type Position = [f64; 2];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Walk,
    Bike,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Endpoint {
    Position { position: Position },
    Target { id: String },
}

fn snap_default() -> f64 {
    100.0
}
fn alternatives_default() -> u8 {
    1
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RouteRequest {
    pub origin: Endpoint,
    pub destination: Endpoint,
    #[serde(default)]
    pub mode: Mode,
    #[serde(default)]
    pub avoid_steps: bool,
    #[serde(default = "snap_default")]
    pub max_snap_distance_m: f64,
    /// Total number of routes requested, including the best (1..=3).
    #[serde(default = "alternatives_default")]
    pub alternatives: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snap {
    pub position: Position,
    pub edge: usize,
    /// Along the stored edge orientation a -> b, in declared edge metres.
    pub offset_m: f64,
    pub fraction: f64,
    pub distance_m: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RouteResponse {
    pub routes: Vec<Route>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Route {
    pub mode: Mode,
    /// Dominant accumulated way, or the landmark best describing an unnamed stretch.
    pub via: String,
    pub geometry: Vec<Position>,
    /// Route distance at each geometry vertex; uses supplied edge lengths.
    pub geometry_offsets_m: Vec<f64>,
    pub distance_m: f64,
    pub duration_s: f64,
    pub segments: Vec<RouteSegment>,
    pub maneuvers: Vec<Maneuver>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RouteSegment {
    pub geometry_start: usize,
    pub geometry_end: usize,
    pub distance_m: f64,
    pub duration_s: f64,
    /// None for the off-network connector to/from a true position.
    pub edge: Option<usize>,
    pub name: Option<String>,
    pub steps: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManeuverType {
    Depart,
    Turn,
    Continue,
    Stairs,
    Arrive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modifier {
    Ahead,
    SlightLeft,
    Left,
    SharpLeft,
    SlightRight,
    Right,
    SharpRight,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ManeuverLandmark {
    pub name: String,
    pub at: Position,
    pub relation: LandmarkRelation,
}

/// For a beside landmark, the cardinal side is the path's side of the landmark.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LandmarkRelation {
    Ahead,
    Passed,
    North,
    East,
    South,
    West,
}

/// A decision folded into an earlier instruction to keep announcements spaced.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ManeuverAction {
    pub kind: ManeuverType,
    pub modifier: Modifier,
    pub offset_m: f64,
    #[serde(default)]
    pub way_name: Option<String>,
    #[serde(default)]
    pub landmark: Option<ManeuverLandmark>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Maneuver {
    #[serde(rename = "type")]
    pub kind: ManeuverType,
    pub modifier: Modifier,
    pub position: Position,
    pub bearing_before: f64,
    pub bearing_after: f64,
    pub offset_m: f64,
    pub distance_to_next_m: f64,
    pub way_name: Option<String>,
    #[serde(default)]
    pub target_id: Option<String>,
    #[serde(default)]
    pub landmark: Option<ManeuverLandmark>,
    /// True on the last instruction before arrival.
    #[serde(default)]
    pub leads_to_arrival: bool,
    #[serde(default)]
    pub following_actions: Vec<ManeuverAction>,
    pub instruction: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptStage {
    Depart,
    Early,
    Approach,
    Now,
    Arrive,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VoicePrompt {
    pub maneuver_index: usize,
    pub stage: PromptStage,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ManeuverDistance {
    pub maneuver: Maneuver,
    pub distance_m: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NavigationUpdate {
    pub distance_travelled_m: f64,
    pub distance_remaining_m: f64,
    pub eta_s: f64,
    pub matched_position: Position,
    pub matched_distance_m: f64,
    pub current: ManeuverDistance,
    pub next: Option<ManeuverDistance>,
    pub voice_prompts: Vec<VoicePrompt>,
    pub off_route: bool,
    pub arrived: bool,
}
