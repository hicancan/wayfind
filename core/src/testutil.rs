use crate::*;

pub fn p(x: f64, y: f64) -> Position {
    [x / 111_195.080_233_532_9, y / 111_195.080_233_532_9]
}
pub fn target(id: &str) -> Endpoint {
    Endpoint::Target { id: id.into() }
}
pub fn position(x: f64, y: f64) -> Endpoint {
    Endpoint::Position { position: p(x, y) }
}
pub fn request(origin: Endpoint, destination: Endpoint) -> RouteRequest {
    RouteRequest {
        origin,
        destination,
        mode: Mode::Walk,
        avoid_steps: false,
        max_snap_distance_m: 100.0,
        alternatives: 1,
    }
}
pub fn data(nodes: &[(f64, f64)], edges: &[(usize, usize, f64, u32, i64)]) -> GraphData {
    serde_json::from_value(serde_json::json!({
        "format":"wayfind-graph/1", "nodes":nodes.iter().map(|(x,y)|p(*x,*y)).collect::<Vec<_>>(),
        "edges":edges, "shapes":vec![Vec::<Position>::new();edges.len()],"names":["甲路","乙路"],
        "flags":{"walk":1,"bike":2,"steps":4,"connector":8},
        "targets":{"start":[0],"end":[nodes.len()-1]}
    }))
    .unwrap()
}
pub fn graph(nodes: &[(f64, f64)], edges: &[(usize, usize, f64, u32, i64)]) -> Graph {
    Graph::from_data(data(nodes, edges)).unwrap()
}
pub fn one(graph: &Graph, request: &RouteRequest) -> Route {
    graph.route(request).unwrap().routes.remove(0)
}
pub fn line() -> Graph {
    graph(
        &[(0.0, 0.0), (150.0, 0.0), (150.0, 150.0)],
        &[(0, 1, 150.0, 3, 0), (1, 2, 150.0, 3, 1)],
    )
}
pub fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() < tolerance,
        "{actual} != {expected}"
    );
}
