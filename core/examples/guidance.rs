//! Print caller-supplied target routes for human review; no external data is bundled.
use wayfind_core::{Endpoint, Mode, RouteRequest, Router};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1).peekable();
    let json = args.peek().is_some_and(|arg| arg == "--json");
    if json {
        args.next();
    }
    let file = args
        .next()
        .ok_or("usage: guidance [--json] GRAPH ORIGIN DESTINATION...")?;
    let origin = args.next().ok_or("missing origin target")?;
    let router = Router::new(&std::fs::read_to_string(file)?)?;
    for destination in args {
        let response = router.route(&RouteRequest {
            origin: Endpoint::Target { id: origin.clone() },
            destination: Endpoint::Target {
                id: destination.clone(),
            },
            mode: Mode::Walk,
            avoid_steps: false,
            max_snap_distance_m: 100.0,
            alternatives: 1,
        })?;
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "origin": origin, "destination": destination, "response": response
                })
            );
        } else {
            for route in response.routes {
                println!(
                    "{origin} → {destination}: {:.1} m, {} instructions",
                    route.distance_m,
                    route.maneuvers.len()
                );
                for (index, maneuver) in route.maneuvers.iter().enumerate() {
                    println!(
                        "{}. [{:.1} m] {}",
                        index + 1,
                        maneuver.offset_m,
                        maneuver.instruction
                    );
                }
            }
        }
    }
    Ok(())
}
