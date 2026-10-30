//! Place-independent routing and guidance. All distances are metres and times seconds.
mod geo;
mod graph;
pub mod instructions;
mod maneuvers;
mod navigation;
mod routing;
mod types;

pub use geo::{bearing, haversine};
pub use graph::{Flags, Graph, GraphData, Landmark};
pub use navigation::Navigator;
pub use types::*;

#[cfg(test)]
pub(crate) mod testutil;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("no usable edge within snap distance")]
    Snap,
    #[error("unknown target: {0}")]
    UnknownTarget(String),
    #[error("no route between endpoints")]
    NoRoute,
}

pub type Result<T> = std::result::Result<T, Error>;

/// Immutable graph; create one per network and reuse it for routing requests.
pub struct Router {
    pub graph: Graph,
}

impl Router {
    pub fn new(graph_json: &str) -> Result<Self> {
        Ok(Self {
            graph: Graph::from_json(graph_json)?,
        })
    }

    pub fn route(&self, request: &RouteRequest) -> Result<RouteResponse> {
        self.graph.route(request)
    }

    pub fn route_json(&self, request_json: &str) -> Result<String> {
        let request = serde_json::from_str(request_json)?;
        Ok(serde_json::to_string(&self.route(&request)?)?)
    }
}
