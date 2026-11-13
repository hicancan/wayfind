use std::sync::Mutex;

uniffi::setup_scaffolding!();

#[derive(Debug, uniffi::Error)]
#[uniffi(flat_error)]
pub enum WayfindError {
    Failed { reason: String },
}

impl std::fmt::Display for WayfindError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Failed { reason } => f.write_str(reason),
        }
    }
}
impl std::error::Error for WayfindError {}
fn error(error: impl ToString) -> WayfindError {
    WayfindError::Failed {
        reason: error.to_string(),
    }
}

#[derive(uniffi::Object)]
pub struct Router {
    inner: wayfind_core::Router,
}

#[uniffi::export]
impl Router {
    #[uniffi::constructor]
    pub fn new(graph_json: String) -> Result<Self, WayfindError> {
        Ok(Self {
            inner: wayfind_core::Router::new(&graph_json).map_err(error)?,
        })
    }

    pub fn route(&self, request_json: String) -> Result<String, WayfindError> {
        self.inner.route_json(&request_json).map_err(error)
    }
}

#[derive(uniffi::Object)]
pub struct Navigator {
    inner: Mutex<wayfind_core::Navigator>,
}

#[uniffi::export]
impl Navigator {
    #[uniffi::constructor]
    pub fn new(route_json: String) -> Result<Self, WayfindError> {
        Ok(Self {
            inner: Mutex::new(wayfind_core::Navigator::from_json(&route_json).map_err(error)?),
        })
    }

    pub fn update(
        &self,
        lon: f64,
        lat: f64,
        accuracy_m: f64,
        heading_deg: Option<f64>,
        timestamp_ms: f64,
    ) -> Result<String, WayfindError> {
        self.inner
            .lock()
            .map_err(|_| error("navigation session lock poisoned"))?
            .update_json(lon, lat, accuracy_m, heading_deg, timestamp_ms)
            .map_err(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRAPH: &str = r#"{"format":"wayfind-graph/1","nodes":[[0,0],[0.002,0]],
        "edges":[[0,1,222,3,-1]],"shapes":[[]],"names":[],
        "flags":{"walk":1,"bike":2,"steps":4,"connector":8},"targets":{"start":[0],"end":[1]}}"#;
    const REQUEST: &str =
        r#"{"origin":{"type":"target","id":"start"},"destination":{"type":"target","id":"end"}}"#;

    #[test]
    fn json_protocol_and_navigation_round_trip() {
        let router = Router::new(GRAPH.into()).unwrap();
        let response: wayfind_core::RouteResponse =
            serde_json::from_str(&router.route(REQUEST.into()).unwrap()).unwrap();
        assert_eq!(response.routes[0].distance_m, 222.0);
        let navigator =
            Navigator::new(serde_json::to_string(&response.routes[0]).unwrap()).unwrap();
        let update: wayfind_core::NavigationUpdate =
            serde_json::from_str(&navigator.update(0.0, 0.0, 5.0, None, 0.0).unwrap()).unwrap();
        assert!(!update.arrived);
        assert!(!update.voice_prompts.is_empty());
        let update: wayfind_core::NavigationUpdate =
            serde_json::from_str(&navigator.update(0.002, 0.0, 5.0, None, 180_000.0).unwrap())
                .unwrap();
        assert!(update.arrived);
    }

    #[test]
    fn errors_cross_the_adapter_and_leave_the_session_usable() {
        assert!(Router::new("{}".into()).is_err());
        assert!(Navigator::new("{}".into()).is_err());
        let router = Router::new(GRAPH.into()).unwrap();
        assert!(router.route("{}".into()).is_err());
        let value: serde_json::Value =
            serde_json::from_str(&router.route(REQUEST.into()).unwrap()).unwrap();
        let navigator = Navigator::new(value["routes"][0].to_string()).unwrap();
        assert!(navigator.update(0.0, 0.0, -1.0, None, 0.0).is_err());
        assert!(navigator.update(0.0, 0.0, 5.0, None, 0.0).is_ok());
    }
}
