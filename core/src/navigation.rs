use serde::Serialize;

use crate::{
    geo, instructions, Error, ManeuverDistance, Mode, NavigationUpdate, PromptStage, Result, Route,
    VoicePrompt,
};

/// One mutable session per trip. Timestamps are monotonic milliseconds since
/// any epoch, represented as safe integers across JavaScript and Kotlin.
#[derive(Serialize)]
pub struct Navigator {
    route: Route,
    progress: f64,
    last_timestamp: Option<f64>,
    off_since: Option<f64>,
    off_count: u8,
    arrived: bool,
    announced: Vec<u8>,
}

impl Navigator {
    pub fn from_json(route_json: &str) -> Result<Self> {
        Self::new(serde_json::from_str(route_json)?)
    }

    pub fn new(route: Route) -> Result<Self> {
        validate_route(&route)?;
        Ok(Self {
            announced: vec![0; route.maneuvers.len()],
            route,
            progress: 0.0,
            last_timestamp: None,
            off_since: None,
            off_count: 0,
            arrived: false,
        })
    }

    pub fn update(
        &mut self,
        lon: f64,
        lat: f64,
        accuracy_m: f64,
        heading_deg: Option<f64>,
        timestamp_ms: f64,
    ) -> Result<NavigationUpdate> {
        let position = [lon, lat];
        geo::validate_position(position)?;
        if !accuracy_m.is_finite()
            || accuracy_m < 0.0
            || heading_deg.is_some_and(|h| !h.is_finite())
            || !timestamp_ms.is_finite()
            || !(0.0..=9_007_199_254_740_991.0).contains(&timestamp_ms)
            || timestamp_ms.fract() != 0.0
            || self.last_timestamp.is_some_and(|last| timestamp_ms < last)
        {
            return Err(Error::Invalid(
                "invalid accuracy, heading or monotonic millisecond timestamp".into(),
            ));
        }
        let elapsed = self
            .last_timestamp
            .map_or(0.0, |last| (timestamp_ms - last) / 1000.0);
        let speed = if self.route.mode == Mode::Bike {
            4.2
        } else {
            1.25
        };
        let low = (self.progress - 5.0).max(0.0);
        let high = if self.last_timestamp.is_none() {
            self.route.distance_m
        } else {
            (self.progress + 60.0 + elapsed * speed * 2.0 + accuracy_m * 2.0)
                .min(self.route.distance_m)
        };
        let mut best = (
            f64::INFINITY,
            self.progress,
            self.route.geometry[0],
            f64::INFINITY,
        );
        for index in 0..self.route.geometry.len().saturating_sub(1) {
            let (a, b) = (
                self.route.geometry_offsets_m[index],
                self.route.geometry_offsets_m[index + 1],
            );
            if b < low || a > high {
                continue;
            }
            let from = a.max(low);
            let to = b.min(high);
            let pa = geo::point_at(&self.route.geometry, &self.route.geometry_offsets_m, from);
            let pb = geo::point_at(&self.route.geometry, &self.route.geometry_offsets_m, to);
            let (point, fraction, distance) = geo::project(position, pa, pb);
            let offset = from + (to - from) * fraction;
            let heading_cost = heading_deg.map_or(0.0, |heading| {
                if geo::haversine(pa, pb) < 1.0 {
                    0.0
                } else {
                    geo::angle(heading, geo::bearing(pa, pb)).abs() / 180.0 * 6.0
                }
            });
            // Prefer the earlier branch at crossings. Keep heading influence
            // small enough that a noisy compass cannot pull a match far away.
            let score = distance + heading_cost + (offset - self.progress).abs() * 0.015;
            if score < best.0 {
                best = (score, offset, point, distance);
            }
        }
        if !best.0.is_finite() {
            let point = geo::point_at(
                &self.route.geometry,
                &self.route.geometry_offsets_m,
                self.progress,
            );
            best = (0.0, self.progress, point, geo::haversine(position, point));
        }
        let threshold = 20.0_f64.max(2.0 * accuracy_m);
        let outside = best.3 > threshold;
        if outside {
            self.off_count = self.off_count.saturating_add(1);
            self.off_since.get_or_insert(timestamp_ms);
        } else {
            self.off_count = 0;
            self.off_since = None;
            self.progress = self.progress.max(best.1);
        }
        let mut off_route = outside
            && (self.off_count >= 3
                || self
                    .off_since
                    .is_some_and(|start| timestamp_ms - start >= 8000.0));
        self.last_timestamp = Some(timestamp_ms);
        let destination = self.route.geometry[self.route.geometry.len() - 1];
        if geo::haversine(position, destination) <= 15.0 {
            self.arrived = true;
            self.progress = self.route.distance_m;
        }
        if self.arrived {
            off_route = false;
        }
        let prompts = self.prompts(off_route);
        let current_index = if self.arrived {
            self.route.maneuvers.len() - 1
        } else {
            self.route
                .maneuvers
                .partition_point(|m| m.offset_m <= self.progress + 1e-6)
                .saturating_sub(1)
                .min(self.route.maneuvers.len() - 2)
        };
        let info = |index: usize| ManeuverDistance {
            maneuver: self.route.maneuvers[index].clone(),
            distance_m: (self.route.maneuvers[index].offset_m - self.progress).max(0.0),
        };
        let current = info(current_index);
        let next = if current_index + 1 < self.route.maneuvers.len() {
            Some(info(current_index + 1))
        } else {
            None
        };
        let eta_s = self
            .route
            .segments
            .iter()
            .map(|segment| {
                let start = self.route.geometry_offsets_m[segment.geometry_start];
                let remaining =
                    (start + segment.distance_m - self.progress).clamp(0.0, segment.distance_m);
                if segment.distance_m > 0.0 {
                    segment.duration_s * remaining / segment.distance_m
                } else {
                    0.0
                }
            })
            .sum();
        Ok(NavigationUpdate {
            distance_travelled_m: self.progress,
            distance_remaining_m: (self.route.distance_m - self.progress).max(0.0),
            eta_s,
            matched_position: if self.arrived {
                destination
            } else {
                geo::point_at(
                    &self.route.geometry,
                    &self.route.geometry_offsets_m,
                    self.progress,
                )
            },
            matched_distance_m: best.3,
            current,
            next,
            voice_prompts: prompts,
            off_route,
            arrived: self.arrived,
        })
    }

    pub fn update_json(
        &mut self,
        lon: f64,
        lat: f64,
        accuracy_m: f64,
        heading_deg: Option<f64>,
        timestamp_ms: f64,
    ) -> Result<String> {
        Ok(serde_json::to_string(&self.update(
            lon,
            lat,
            accuracy_m,
            heading_deg,
            timestamp_ms,
        )?)?)
    }

    fn prompts(&mut self, off_route: bool) -> Vec<VoicePrompt> {
        let mut prompts = Vec::new();
        let last = self.route.maneuvers.len() - 1;
        if self.arrived {
            if self.announced[last] & 4 == 0 {
                prompts.push(VoicePrompt {
                    maneuver_index: last,
                    stage: PromptStage::Arrive,
                    text: self.route.maneuvers[last].instruction.clone(),
                });
                self.announced.fill(7);
            }
            return prompts;
        }
        if off_route {
            return prompts;
        }
        if self.announced[0] == 0 {
            if self.progress < 20.0 {
                prompts.push(VoicePrompt {
                    maneuver_index: 0,
                    stage: PromptStage::Depart,
                    text: self.route.maneuvers[0].instruction.clone(),
                });
            }
            self.announced[0] = 7;
        }
        let early = if self.route.mode == Mode::Bike {
            200.0
        } else {
            100.0
        };
        let now = if self.route.mode == Mode::Bike {
            12.0
        } else {
            8.0
        };
        for index in 1..=last {
            let maneuver = &self.route.maneuvers[index];
            let distance = maneuver.offset_m - self.progress;
            if distance < -now {
                self.announced[index] = 7;
                continue;
            }
            // Only the next decision should speak, even if several are nearby.
            if self.announced[index] & 4 != 0 {
                continue;
            }
            let (mask, stage) = if distance <= now {
                (7, PromptStage::Now)
            } else if distance <= 30.0 {
                (3, PromptStage::Approach)
            } else if distance <= early {
                (1, PromptStage::Early)
            } else {
                break;
            };
            if self.announced[index] & mask != mask {
                self.announced[index] |= mask;
                prompts.push(VoicePrompt {
                    maneuver_index: index,
                    stage,
                    text: if stage == PromptStage::Now {
                        maneuver.instruction.clone()
                    } else {
                        instructions::chinese_advance(maneuver, distance)
                    },
                });
            }
            break;
        }
        prompts
    }
}

fn validate_route(route: &Route) -> Result<()> {
    let invalid = || Error::Invalid("invalid route geometry, metrics or maneuvers".into());
    let metric = |x: f64| x.is_finite() && x >= 0.0;
    if route.geometry.is_empty()
        || route.geometry.len() != route.geometry_offsets_m.len()
        || !metric(route.distance_m)
        || !metric(route.duration_s)
        || route.maneuvers.len() < 2
    {
        return Err(invalid());
    }
    for p in &route.geometry {
        geo::validate_position(*p)?;
    }
    let offsets = &route.geometry_offsets_m;
    if offsets.iter().any(|x| !metric(*x))
        || offsets[0] != 0.0
        || offsets.windows(2).any(|p| p[1] < p[0])
        || (offsets[offsets.len() - 1] - route.distance_m).abs() > 1e-5
    {
        return Err(invalid());
    }
    let mut end = 0;
    let mut duration = 0.0;
    for s in &route.segments {
        if s.geometry_start != end
            || s.geometry_end <= end
            || s.geometry_end >= route.geometry.len()
            || !metric(s.distance_m)
            || !metric(s.duration_s)
            || (offsets[s.geometry_end] - offsets[s.geometry_start] - s.distance_m).abs() > 1e-5
        {
            return Err(invalid());
        }
        duration += s.duration_s;
        end = s.geometry_end;
    }
    if end != route.geometry.len() - 1 || (duration - route.duration_s).abs() > 1e-5 {
        return Err(invalid());
    }
    let mut previous = 0.0;
    for m in &route.maneuvers {
        geo::validate_position(m.position)?;
        if !metric(m.offset_m)
            || m.offset_m < previous
            || m.offset_m > route.distance_m + 1e-5
            || !metric(m.distance_to_next_m)
            || !m.bearing_before.is_finite()
            || !m.bearing_after.is_finite()
        {
            return Err(invalid());
        }
        previous = m.offset_m;
    }
    use crate::ManeuverType::{Arrive, Depart};
    if route.maneuvers[0].kind != Depart
        || route.maneuvers[0].offset_m != 0.0
        || route.maneuvers[route.maneuvers.len() - 1].kind != Arrive
        || (previous - route.distance_m).abs() > 1e-5
    {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
