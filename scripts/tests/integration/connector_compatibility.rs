use openace_rust::{c_api::*, python::*};
use std::ffi::CString;

#[tokio::test]
async fn test_c_api_compatibility() {
    let engine = openace_init();
    assert!(!engine.is_null());
    let status = openace_get_engine_status(engine);
    // Assert status
    openace_shutdown(engine);
}

#[tokio::test]
async fn test_python_compatibility() {
    let py_engine = init_python_engine();
    assert!(py_engine.is_ok());
    // More assertions
    shutdown_python_engine(py_engine.unwrap());
}

// Additional exhaustive tests for edge cases, errors, multi-threading, etc.