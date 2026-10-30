//! Optional local integration check. Supply WAYFIND_GRAPH_FILE; external data
//! never becomes a fixture or appears in diagnostics.
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
use wayfind_core::{Endpoint, Mode, Navigator, RouteRequest, Router};

#[test]
fn local_graph_connectivity_and_thousand_routes() {
    let Some(path) = std::env::var_os("WAYFIND_GRAPH_FILE") else {
        eprintln!("skipped: WAYFIND_GRAPH_FILE is unset");
        return;
    };
    if !std::path::Path::new(&path).is_file() {
        eprintln!("skipped: external graph is absent");
        return;
    }
    let json = std::fs::read_to_string(path).expect("read external graph");
    let router = Router::new(&json).expect("load external graph");
    let labels = router.graph.components(Mode::Walk, false);
    let mut counts = BTreeMap::new();
    for &label in &labels {
        *counts.entry(label).or_insert(0) += 1;
    }
    let main = *counts
        .iter()
        .max_by_key(|(_, size)| *size)
        .expect("nonempty graph")
        .0;
    let ids: Vec<_> = router.graph.targets().keys().cloned().collect();
    assert!(ids.len() >= 2);
    for nodes in router.graph.targets().values() {
        assert!(
            nodes.iter().any(|n| labels[*n] == main),
            "every target must have access to the main component"
        );
    }
    let request = |a: usize, b: usize| RouteRequest {
        origin: Endpoint::Target { id: ids[a].clone() },
        destination: Endpoint::Target { id: ids[b].clone() },
        mode: Mode::Walk,
        avoid_steps: false,
        max_snap_distance_m: 100.0,
        alternatives: 1,
    };
    for a in 0..ids.len() {
        for b in 0..ids.len() {
            let mut response = router
                .route(&request(a, b))
                .expect("all target pairs must be reachable");
            assert!(
                Navigator::new(response.routes.remove(0)).is_ok(),
                "all routes must be usable by a navigator"
            );
        }
    }
    // Reproducible pseudo-random distinct pairs; includes full geometry and
    // maneuvers, but excludes graph loading and JSON serialization.
    let mut state = 0x5eed_u64;
    let mut random = || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        (state >> 32) as usize
    };
    let pairs: Vec<_> = (0..1000)
        .map(|_| {
            let a = random() % ids.len();
            let b = (a + 1 + random() % (ids.len() - 1)) % ids.len();
            request(a, b)
        })
        .collect();
    let start = Instant::now();
    for pair in &pairs {
        std::hint::black_box(router.route(pair).expect("route random pair"));
    }
    let elapsed = start.elapsed();
    eprintln!(
        "{} nodes, {} edges, {} targets; all {} target pairs reachable; 1000 routes: {elapsed:?}",
        router.graph.node_count(),
        router.graph.edge_count(),
        ids.len(),
        ids.len() * ids.len()
    );
    if !cfg!(debug_assertions) {
        assert!(
            elapsed < Duration::from_millis(800),
            "1000 routes must take well under one second in release"
        );
    }
}
