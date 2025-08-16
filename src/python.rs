#![cfg(feature = "python")]

use pyo3::prelude::*;
use pyo3::wrap_pyfunction;
use std::ffi::CString;
use std::os::raw::c_void;
use crate::c_api::*; // Abhängig von c_api Feature

#[pyfunction]
fn init_engine() -> PyResult<*mut c_void> {
    Ok(openace_init())
}

#[pyfunction]
fn shutdown_engine(engine: *mut c_void) -> PyResult<()> {
    openace_shutdown(engine);
    Ok(())
}

#[pyfunction]
fn submit_job(engine: *mut c_void, input_path: &str, output_dir: &str, duration: i32, format: &str, quality: &str) -> PyResult<i32> {
    let input_c = CString::new(input_path)?;
    let output_c = CString::new(output_dir)?;
    let format_c = CString::new(format)?;
    let quality_c = CString::new(quality)?;
    Ok(openace_submit_segmentation_job(engine, input_c.as_ptr(), output_c.as_ptr(), duration, format_c.as_ptr(), quality_c.as_ptr()))
}

#[pyfunction]
fn get_job_status(engine: *mut c_void, job_id: i32) -> PyResult<i32> {
    Ok(openace_get_job_status(engine, job_id))
}

#[pyfunction]
fn cancel_job(engine: *mut c_void, job_id: i32) -> PyResult<i32> {
    Ok(openace_cancel_job(engine, job_id))
}

#[pyfunction]
fn get_engine_stats(engine: *mut c_void) -> PyResult<String> {
    let stats_ptr = openace_get_engine_stats(engine);
    if stats_ptr.is_null() {
        return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Failed to get engine stats"));
    }
    let stats_cstr = unsafe { std::ffi::CStr::from_ptr(stats_ptr) };
    let stats_str = stats_cstr.to_str().map_err(|_| {
        PyErr::new::<pyo3::exceptions::PyUnicodeDecodeError, _>("Invalid UTF-8 in stats")
    })?;
    Ok(stats_str.to_string())
}

#[pyfunction]
fn set_log_level(level: &str) -> PyResult<i32> {
    let level_c = CString::new(level)?;
    Ok(openace_set_log_level(level_c.as_ptr()))
}

#[pyfunction]
fn get_version() -> PyResult<String> {
    let version_ptr = openace_get_version();
    if version_ptr.is_null() {
        return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Failed to get version"));
    }
    let version_cstr = unsafe { std::ffi::CStr::from_ptr(version_ptr) };
    let version_str = version_cstr.to_str().map_err(|_| {
        PyErr::new::<pyo3::exceptions::PyUnicodeDecodeError, _>("Invalid UTF-8 in version")
    })?;
    Ok(version_str.to_string())
}

#[pymodule]
fn openace(py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(init_engine, m)?)?;
    m.add_function(wrap_pyfunction!(shutdown_engine, m)?)?;
    m.add_function(wrap_pyfunction!(submit_job, m)?)?;
    m.add_function(wrap_pyfunction!(get_job_status, m)?)?;
    m.add_function(wrap_pyfunction!(cancel_job, m)?)?;
    m.add_function(wrap_pyfunction!(get_engine_stats, m)?)?;
    m.add_function(wrap_pyfunction!(set_log_level, m)?)?;
    m.add_function(wrap_pyfunction!(get_version, m)?)?;
    Ok(())
}