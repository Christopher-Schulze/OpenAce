#[cfg(test)]
mod tests {
    use openace_rust::c_api::*;
    use std::ffi::{CStr, CString};
    use std::ptr;

    #[test]
    fn test_c_api_initialization() {
        unsafe {
            let engine = openace_init();
            assert!(!engine.is_null());
            openace_shutdown(engine);
        }
    }

    #[test]
    fn test_c_api_submit_job() {
        unsafe {
            let engine = openace_init();
            let input_path = CString::new("tests/fixtures/test_media/sample.mp4").unwrap();
            let output_dir = CString::new("tests/fixtures/output_segments").unwrap();
            let job_id = openace_submit_segmentation_job(engine, input_path.as_ptr(), output_dir.as_ptr(), 10, b"mp4".as_ptr(), b"high".as_ptr());
            assert!(job_id > 0);
            openace_shutdown(engine);
        }
    }

    #[test]
    fn test_c_api_get_status() {
        unsafe {
            let engine = openace_init();
            let status_ptr = openace_get_engine_status(engine);
            let status = CStr::from_ptr(status_ptr).to_str().unwrap();
            assert_eq!(status, "Running");
            openace_free_string(status_ptr);
            openace_shutdown(engine);
        }
    }
}

#[test]
fn test_c_api_live_functions() {
    unsafe {
        let engine = openace_init();
        assert!(!openace_live_is_streaming(engine));
        let url = CString::new("test_url").unwrap();
        let result = openace_live_start_stream(engine, url.as_ptr(), 1080);
        assert_eq!(result, 0);
        assert!(openace_live_is_streaming(engine));
        openace_live_stop_stream(engine);
        openace_shutdown(engine);
    }
}

#[test]
fn test_c_api_transport_functions() {
    unsafe {
        let engine = openace_init();
        let tracker = CString::new("test_tracker").unwrap();
        openace_transport_set_tracker(engine, tracker.as_ptr());
        openace_transport_connect_peers(engine);
        assert!(openace_transport_get_peer_count(engine) >= 0);
        openace_shutdown(engine);
    }
}

#[test]
fn test_c_api_streamer_functions() {
    unsafe {
        let engine = openace_init();
        let file = CString::new("test_file.mp4").unwrap();
        openace_streamer_set_media_file(engine, file.as_ptr());
        openace_streamer_set_video_params(engine, 1920, 1080, 60, 5000);
        openace_streamer_enable_hardware_acceleration(engine, true);
        let result = openace_streamer_start_streaming(engine);
        assert_eq!(result, 0);
        assert!(openace_streamer_is_streaming(engine));
        openace_streamer_stop_streaming(engine);
        openace_shutdown(engine);
    }
}

#[test]
fn test_c_api_invalid_pointer() {
    unsafe {
        // Test with null engine pointer
        let status_ptr = openace_get_engine_status(ptr::null_mut());
        assert!(status_ptr.is_null());

        // Test submit job with invalid paths
        let result = openace_submit_segmentation_job(ptr::null_mut(), ptr::null(), ptr::null(), 0, ptr::null(), ptr::null());
        assert!(result < 0);
    }
}

#[test]
fn test_c_api_multithreading() {
    use std::thread;
    unsafe {
        let engine = openace_init();
        let handle = thread::spawn(move || {
            let status_ptr = openace_get_engine_status(engine);
            assert!(!status_ptr.is_null());
            openace_free_string(status_ptr);
        });
        handle.join().unwrap();
        openace_shutdown(engine);
    }
}

#[test]
fn test_c_api_acestream_versions() {
    unsafe {
        let engine = openace_init();
        // Simulate different AceStream protocol versions
        let old_protocol = CString::new("acestream_v1").unwrap();
        let result_v1 = openace_transport_send_message(engine, old_protocol.as_ptr());
        assert_eq!(result_v1, 0);

        let new_protocol = CString::new("acestream_v2").unwrap();
        let result_v2 = openace_transport_send_message(engine, new_protocol.as_ptr());
        assert_eq!(result_v2, 0);
        openace_shutdown(engine);
    }
}

#[test]
fn test_c_api_memory_leak() {
    unsafe {
        let engine = openace_init();
        for _ in 0..100 {
            let status_ptr = openace_get_engine_status(engine);
            openace_free_string(status_ptr);
        }
        openace_shutdown(engine);
        // Note: Actual memory leak detection would require tools like valgrind
    }
}
#[test]
fn test_c_api_compatibility_parity() {
    unsafe {
        let engine = openace_init();
        // Simulate AceStream protocol compatibility
        let protocol_data = CString::new("acestream_protocol").unwrap();
        let result = openace_transport_send_message(engine, protocol_data.as_ptr());
        assert_eq!(result, 0);
        openace_shutdown(engine);
    }
}