//! Small browser binding; all image parsing and translation runs inside Wasm.
use crate::{Artifact, Compiler};
use wasm_bindgen::prelude::*;

/// Metadata used to verify and compile the user's local installation.
#[wasm_bindgen(js_name = completeProfileJson)]
pub fn complete_profile_json() -> String {
    crate::complete_profile_json().to_owned()
}

#[wasm_bindgen(js_name = Compiler)]
pub struct BrowserCompiler(Compiler);

#[wasm_bindgen(js_class = Compiler)]
impl BrowserCompiler {
    #[wasm_bindgen(constructor)]
    pub fn new(executable: &[u8]) -> Result<BrowserCompiler, JsError> {
        Compiler::new(executable)
            .map(Self)
            .map_err(|error| JsError::new(&error.to_string()))
    }

    #[wasm_bindgen(js_name = addModule)]
    pub fn add_module(&mut self, bytes: &[u8], base: u32) -> Result<(), JsError> {
        self.0
            .add_module(bytes, base)
            .map_err(|error| JsError::new(&error.to_string()))
    }

    #[wasm_bindgen(js_name = loadPlanJson)]
    pub fn load_plan_json(&self) -> Result<String, JsError> {
        self.0
            .load_plan()
            .metadata_json()
            .map_err(|error| JsError::new(&error.to_string()))
    }

    #[wasm_bindgen(js_name = loadBytes)]
    pub fn load_bytes(&self) -> Vec<u8> {
        self.0.load_plan().bytes
    }

    pub fn compile(&self, roots: &[u32]) -> Result<BrowserArtifact, JsError> {
        self.0
            .compile(roots)
            .map(BrowserArtifact)
            .map_err(|error| JsError::new(&error.to_string()))
    }
}

#[wasm_bindgen(js_name = Artifact)]
pub struct BrowserArtifact(Artifact);

#[wasm_bindgen(js_class = Artifact)]
impl BrowserArtifact {
    /// Copies the emitted module bytes to the host for WebAssembly.compile.
    pub fn wasm(&self) -> Vec<u8> {
        self.0.wasm.clone()
    }

    #[wasm_bindgen(js_name = metadataJson)]
    pub fn metadata_json(&self) -> Result<String, JsError> {
        self.0
            .metadata_json()
            .map_err(|error| JsError::new(&error.to_string()))
    }

    /// Serialize only the fields needed to instantiate and link the guest module.
    #[wasm_bindgen(js_name = runtimeMetadataJson)]
    pub fn runtime_metadata_json(&self) -> Result<String, JsError> {
        self.0
            .runtime_metadata_json()
            .map_err(|error| JsError::new(&error.to_string()))
    }
}
