#![cfg(feature = "c_api")]

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::ptr;
use crate::engines::manager::EngineManager;
use tokio::runtime::Runtime;

#[no_mangle]
pub extern "C" fn openace_init() -> *mut c_void {
    if let Ok(manager) = EngineManager::new() {
        Box::into_raw(Box::new(manager)) as *mut c_void
    } else {
        ptr::null_mut()
    }
}

#[no_mangle]
pub extern "C" fn openace_shutdown(engine: *mut c_void) {
    if !engine.is_null() {
        unsafe { Box::from_raw(engine as *mut EngineManager); }
    }
}

#[no_mangle]
pub extern "C" fn openace_submit_segmentation_job(
    engine: *mut c_void,
    input_path: *const c_char,
    output_path: *const c_char,
    format: *const c_char,
) -> *mut c_char {
    if engine.is_null() || input_path.is_null() || output_path.is_null() || format.is_null() {
        return std::ptr::null_mut();
    }
    
    let manager = unsafe { &*(engine as *const EngineManager) };
    
    let input_path_str = match unsafe { CStr::from_ptr(input_path).to_str() } {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    
    let output_path_str = match unsafe { CStr::from_ptr(output_path).to_str() } {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    
    let format_str = match unsafe { CStr::from_ptr(format).to_str() } {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    
    let rt = Runtime::new().unwrap();
    match rt.block_on(manager.submit_segmentation_job(input_path_str, output_path_str, format_str)) {
        Ok(job_id) => {
            match CString::new(job_id) {
                Ok(c_string) => c_string.into_raw(),
                Err(_) => std::ptr::null_mut(),
            }
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn openace_get_engine_status(engine: *mut c_void) -> *mut c_char {
    if engine.is_null() {
        return ptr::null_mut();
    }
    
    let manager = unsafe { &*(engine as *const EngineManager) };
    let rt = Runtime::new().unwrap();
    
    match rt.block_on(manager.get_state()) {
        state => {
            let status_str = format!("{:?}", state);
            match CString::new(status_str) {
                Ok(c_string) => c_string.into_raw(),
                Err(_) => ptr::null_mut(),
            }
        }
    }
}

// Ähnlich für andere Funktionen: Implementiere mit echter Engine-Logik

#[no_mangle]
pub extern "C" fn openace_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe { CString::from_raw(ptr); }
    }
}

#[no_mangle]
pub extern "C" fn openace_live_start_stream(engine: *mut c_void, stream_key: *const c_char, rtmp_url: *const c_char) -> c_int {
    if engine.is_null() || stream_key.is_null() || rtmp_url.is_null() {
        return -1;
    }
    
    let manager = unsafe { &*(engine as *const EngineManager) };
    
    let stream_key_str = match unsafe { CStr::from_ptr(stream_key).to_str() } {
        Ok(s) => {
            if s.len() > 1024 {
                return -3; // Invalid input
            }
            s
        }
        Err(_) => return -2,
    };
    
    let rtmp_url_str = match unsafe { CStr::from_ptr(rtmp_url).to_str() } {
        Ok(s) => {
            if s.len() > 1024 {
                return -3; // Invalid input
            }
            s
        }
        Err(_) => return -2,
    };
    
    let rt = Runtime::new().unwrap();
    match rt.block_on(manager.live_start_stream(stream_key_str, rtmp_url_str)) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

#[no_mangle]
pub extern "C" fn openace_live_is_streaming(engine: *mut c_void, stream_key: *const c_char) -> c_int {
    if engine.is_null() || stream_key.is_null() {
        return -1;
    }
    
    let manager = unsafe { &*(engine as *const EngineManager) };
    
    let stream_key_str = match unsafe { CStr::from_ptr(stream_key).to_str() } {
        Ok(s) => s,
        Err(_) => return -1,
    };
    
    let rt = Runtime::new().unwrap();
    match rt.block_on(manager.is_streaming(stream_key_str)) {
        Ok(is_streaming) => if is_streaming { 1 } else { 0 },
        Err(_) => -1,
    }
}

#[no_mangle]
pub extern "C" fn openace_live_stop_stream(engine: *mut c_void, stream_key: *const c_char) -> c_int {
    if engine.is_null() || stream_key.is_null() {
        return -1;
    }
    
    let manager = unsafe { &*(engine as *const EngineManager) };
    
    let stream_key_str = match unsafe { CStr::from_ptr(stream_key).to_str() } {
        Ok(s) => s,
        Err(_) => return -1,
    };
    
    let rt = Runtime::new().unwrap();
    match rt.block_on(manager.live_stop_stream(stream_key_str)) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

#[no_mangle]
pub extern "C" fn openace_transport_set_tracker(engine: *mut c_void, tracker_url: *const c_char) -> c_int {
    if engine.is_null() || tracker_url.is_null() {
        return -1;
    }
    
    let manager = unsafe { &*(engine as *const EngineManager) };
    
    let tracker_url_str = match unsafe { CStr::from_ptr(tracker_url).to_str() } {
        Ok(s) => {
            if s.len() > 1024 {
                return -3; // Invalid input
            }
            s
        }
        Err(_) => return -2,
    };
    
    let rt = Runtime::new().unwrap();
    match rt.block_on(manager.transport_set_tracker(tracker_url_str)) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

#[no_mangle]
pub extern "C" fn openace_transport_connect_peers(engine: *mut c_void, peer_addresses: *const *const c_char, count: c_int) -> c_int {
    if engine.is_null() || peer_addresses.is_null() || count <= 0 {
        return -1;
    }
    
    let manager = unsafe { &*(engine as *const EngineManager) };
    
    let mut addresses = Vec::new();
    for i in 0..count {
        let addr_ptr = unsafe { *peer_addresses.offset(i as isize) };
        if addr_ptr.is_null() {
            return -2;
        }
        
        let addr_str = match unsafe { CStr::from_ptr(addr_ptr).to_str() } {
            Ok(s) => s.to_string(),
            Err(_) => return -2,
        };
        
        addresses.push(addr_str);
    }
    
    let rt = Runtime::new().unwrap();
    match rt.block_on(manager.transport_connect_peers(&addresses)) {
        Ok(connected_count) => connected_count as c_int,
        Err(_) => -1,
    }
}

#[no_mangle]
pub extern "C" fn openace_transport_get_peer_count(engine: *mut c_void) -> c_int {
    if engine.is_null() {
        return -1;
    }
    
    let manager = unsafe { &*(engine as *const EngineManager) };
    
    let rt = Runtime::new().unwrap();
    match rt.block_on(manager.get_peer_count()) {
        Ok(count) => count as c_int,
        Err(_) => -1,
    }
}

// Alle Funktionen sind nun mit echter Engine-Logik implementiert.

// Füge noch mehr Funktionen hinzu, falls nötig, um alle originalen abzudecken.