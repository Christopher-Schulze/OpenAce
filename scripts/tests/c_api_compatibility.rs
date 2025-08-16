#![cfg(feature = "c_api")]

use std::ffi::{c_char, c_int, c_void, CString};
use openace_rust::c_api::*;

#[test]
fn test_init_and_shutdown() {
    let engine = openace_init();
    assert!(!engine.is_null());
    openace_shutdown(engine);
}

#[test]
fn test_submit_segmentation_job() {
    let engine = openace_init();
    let input_path = CString::new("test_input.mp4").unwrap();
    let output_dir = CString::new("output/").unwrap();
    let format = CString::new("mp4").unwrap();
    let quality = CString::new("high").unwrap();
    let job_id = openace_submit_segmentation_job(engine, input_path.as_ptr(), output_dir.as_ptr(), 10, format.as_ptr(), quality.as_ptr());
    assert!(job_id > 0);
    openace_shutdown(engine);
}

#[test]
fn test_get_engine_status() {
    let engine = openace_init();
    let status_ptr = openace_get_engine_status(engine);
    let status = unsafe { CString::from_raw(status_ptr as *mut c_char) };
    assert_eq!(status.to_str().unwrap(), "Running");
    openace_shutdown(engine);
}

#[test]
fn test_live_streaming() {
    let engine = openace_init();
    let url = CString::new("test_url").unwrap();
    assert_eq!(openace_live_start_stream(engine, url.as_ptr(), 1), 0);
    assert!(openace_live_is_streaming(engine));
    openace_live_stop_stream(engine);
    assert!(!openace_live_is_streaming(engine));
    openace_shutdown(engine);
}

#[test]
fn test_transport_functions() {
    let engine = openace_init();
    let tracker = CString::new("test_tracker").unwrap();
    openace_transport_set_tracker(engine, tracker.as_ptr());
    openace_transport_connect_peers(engine);
    assert!(openace_transport_get_peer_count(engine) >= 0);
    openace_shutdown(engine);
}

#[test]
fn test_invalid_pointer() {
    let status = openace_get_engine_status(std::ptr::null_mut());
    assert!(status.is_null());
}

#[test]
fn test_multithreading() {
    use std::thread;
    let engine = openace_init();
    let handle = thread::spawn(move || {
        // Teste Funktionen in anderem Thread
        openace_live_is_streaming(engine);
    });
    handle.join().unwrap();
    openace_shutdown(engine);
}

// Füge Tests für Python-Integration hinzu, falls nötig, oder erstelle separate Datei.

// ...