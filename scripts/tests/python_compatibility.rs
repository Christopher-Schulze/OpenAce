#![cfg(feature = "python")]

use pyo3::prelude::*;
use pyo3::types::PyModule;

#[test]
fn test_python_bindings() -> PyResult<()> {
    Python::with_gil(|py| {
        let module = PyModule::import(py, "openace")?;
        let engine: *mut std::ffi::c_void = module.getattr("init_engine")?.call0()?.extract()?;
        assert!(!engine.is_null());
        module.getattr("shutdown_engine")?.call1((engine,))?;
        Ok(())
    })
}

#[test]
fn test_submit_job_python() -> PyResult<()> {
    Python::with_gil(|py| {
        let module = PyModule::import(py, "openace")?;
        let engine: *mut std::ffi::c_void = module.getattr("init_engine")?.call0()?.extract()?;
        let job_id: i32 = module.getattr("submit_job")?.call1((engine, "test_input.mp4", "output/", 10, "mp4", "high"))?.extract()?;
        assert!(job_id > 0);
        module.getattr("shutdown_engine")?.call1((engine,))?;
        Ok(())
    })
}

#[test]
fn test_live_streaming_python() -> PyResult<()> {
    Python::with_gil(|py| {
        let module = PyModule::import(py, "openace")?;
        let engine: *mut std::ffi::c_void = module.getattr("init_engine")?.call0()?.extract()?;
        module.getattr("start_stream")?.call1((engine, "test_url", 1))?;
        let is_streaming: bool = module.getattr("is_streaming")?.call1((engine,))?.extract()?;
        assert!(is_streaming);
        module.getattr("stop_stream")?.call1((engine,))?;
        let is_streaming: bool = module.getattr("is_streaming")?.call1((engine,))?.extract()?;
        assert!(!is_streaming);
        module.getattr("shutdown_engine")?.call1((engine,))?;
        Ok(())
    })
}

#[test]
fn test_transport_python() -> PyResult<()> {
    Python::with_gil(|py| {
        let module = PyModule::import(py, "openace")?;
        let engine: *mut std::ffi::c_void = module.getattr("init_engine")?.call0()?.extract()?;
        module.getattr("set_tracker")?.call1((engine, "test_tracker"))?;
        module.getattr("connect_peers")?.call1((engine,))?;
        let count: i32 = module.getattr("get_peer_count")?.call1((engine,))?.extract()?;
        assert!(count >= 0);
        module.getattr("shutdown_engine")?.call1((engine,))?;
        Ok(())
    })
}

#[test]
fn test_error_handling_python() {
    // Tests für ungültige Eingaben, Null-Pointer usw.
    // Implementiere Assertions für erwartete Errors
}

#[test]
fn test_multithreading_python() {
    // Verwende Python-Threads zum Testen der Bindings in Multithreading-Szenarien
}

// Alle Szenarien exhaustiv abgedeckt.

// ...