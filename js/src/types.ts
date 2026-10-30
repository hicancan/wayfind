export type Position = [longitude: number, latitude: number];
export type Mode = 'walk' | 'bike';
export type Endpoint = { type: 'position'; position: Position } | { type: 'target'; id: string };

export interface GraphData {
  format: 'wayfind-graph/1';
  nodes: Position[];
  edges: [a: number, b: number, metres: number, flags: number, name: number][];
  shapes: Position[][];
  names: string[];
  flags: { walk: 1; bike: 2; steps: 4; connector: 8 };
  targets: Record<string, number[]>;
  landmarks?: Landmark[];
}

export interface Landmark { name: string; at: Position }
export type LandmarkRelation = 'ahead' | 'passed' | 'north' | 'east' | 'south' | 'west';
export interface ManeuverLandmark extends Landmark { relation: LandmarkRelation }
export interface ManeuverAction {
  kind: ManeuverType;
  modifier: Modifier;
  offset_m: number;
  way_name: string | null;
  landmark: ManeuverLandmark | null;
}

export interface RouteRequest {
  origin: Endpoint;
  destination: Endpoint;
  mode?: Mode;
  avoid_steps?: boolean;
  max_snap_distance_m?: number;
  /** Total requested routes, including the best. Default 1. */
  alternatives?: 1 | 2 | 3;
}

export type ManeuverType = 'depart' | 'turn' | 'continue' | 'stairs' | 'arrive';
export type Modifier = 'ahead' | 'slight_left' | 'left' | 'sharp_left' | 'slight_right' | 'right' | 'sharp_right';
export interface Maneuver {
  type: ManeuverType;
  modifier: Modifier;
  position: Position;
  bearing_before: number;
  bearing_after: number;
  offset_m: number;
  distance_to_next_m: number;
  way_name: string | null;
  target_id: string | null;
  landmark: ManeuverLandmark | null;
  leads_to_arrival: boolean;
  following_actions: ManeuverAction[];
  instruction: string;
}

export interface RouteSegment {
  geometry_start: number;
  geometry_end: number;
  distance_m: number;
  duration_s: number;
  edge: number | null;
  name: string | null;
  steps: boolean;
}

export interface Route {
  mode: Mode;
  via: string;
  geometry: Position[];
  geometry_offsets_m: number[];
  distance_m: number;
  duration_s: number;
  segments: RouteSegment[];
  maneuvers: Maneuver[];
}

export interface RouteResponse { routes: Route[] }
export type PromptStage = 'depart' | 'early' | 'approach' | 'now' | 'arrive';
export interface VoicePrompt { maneuver_index: number; stage: PromptStage; text: string }
export interface ManeuverDistance { maneuver: Maneuver; distance_m: number }
export interface NavigationUpdate {
  distance_travelled_m: number;
  distance_remaining_m: number;
  eta_s: number;
  matched_position: Position;
  matched_distance_m: number;
  current: ManeuverDistance;
  next: ManeuverDistance | null;
  voice_prompts: VoicePrompt[];
  off_route: boolean;
  arrived: boolean;
}
