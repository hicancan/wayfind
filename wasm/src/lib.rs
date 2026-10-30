use wasm_bindgen::prelude::*;

fn error(error: impl ToString) -> JsValue {
    JsValue::from_str(&error.to_string())
}

#[wasm_bindgen]
pub struct Router {
    inner: wayfind_core::Router,
}

#[wasm_bindgen]
impl Router {
    #[wasm_bindgen(constructor)]
    pub fn new(graph_json: &str) -> Result<Router, JsValue> {
        Ok(Self {
            inner: wayfind_core::Router::new(graph_json).map_err(error)?,
        })
    }

    pub fn route(&self, request_json: &str) -> Result<String, JsValue> {
        self.inner.route_json(request_json).map_err(error)
    }
}

#[wasm_bindgen]
pub struct Navigator {
    inner: wayfind_core::Navigator,
}

#[wasm_bindgen]
impl Navigator {
    #[wasm_bindgen(constructor)]
    pub fn new(route_json: &str) -> Result<Navigator, JsValue> {
        Ok(Self {
            inner: wayfind_core::Navigator::from_json(route_json).map_err(error)?,
        })
    }

    pub fn update(
        &mut self,
        lon: f64,
        lat: f64,
        accuracy_m: f64,
        heading_deg: Option<f64>,
        timestamp_ms: f64,
    ) -> Result<String, JsValue> {
        self.inner
            .update_json(lon, lat, accuracy_m, heading_deg, timestamp_ms)
            .map_err(error)
    }
}
