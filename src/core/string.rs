//! String utilities for OpenAce Rust
//!
//! Provides string manipulation and conversion utilities for the OpenAce system.

use crate::error::{OpenAceError, Result};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use tracing::{debug, warn, trace};

/// String utilities for C interoperability and general string operations
pub struct StringUtils;

impl StringUtils {
    /// Convert a Rust string to a C-compatible null-terminated string
    pub fn to_c_string(s: &str) -> Result<CString> {
        trace!("StringUtils::to_c_string: Converting Rust string of {} chars to C string", s.len());
        
        let result = CString::new(s).map_err(|e| {
            warn!("StringUtils::to_c_string: String contains null byte: {}", e);
            OpenAceError::internal(format!("Failed to convert string to C string: {}", e))
        });
        
        if result.is_ok() {
            trace!("StringUtils::to_c_string: Successfully created C string");
        }
        
        result
    }
    
    /// Convert a C string pointer to a Rust string
    /// 
    /// # Safety
    /// The caller must ensure that the pointer is valid and points to a null-terminated string
    pub unsafe fn from_c_string(ptr: *const c_char) -> Result<String> {
        trace!("StringUtils::from_c_string: Starting C string to Rust string conversion");
        
        if ptr.is_null() {
            warn!("StringUtils::from_c_string: Null pointer passed to from_c_string");
            return Err(OpenAceError::internal("Null pointer passed to from_c_string"));
        }
        
        trace!("StringUtils::from_c_string: Converting C string pointer to CStr");
        let c_str = CStr::from_ptr(ptr);
        trace!("StringUtils::from_c_string: Converting CStr to UTF-8 string");
        let result = c_str.to_str()
            .map(|s| {
                trace!("StringUtils::from_c_string: Successfully converted {} chars to Rust string", s.len());
                s.to_string()
            })
            .map_err(|e| {
                warn!("StringUtils::from_c_string: Invalid UTF-8 in C string: {}", e);
                OpenAceError::internal(format!("Invalid UTF-8 in C string: {}", e))
            });
        
        trace!("StringUtils::from_c_string: C string conversion completed");
        result
    }
    
    /// Sanitize a string for use in file paths
    pub fn sanitize_filename(s: &str) -> String {
        trace!("StringUtils::sanitize_filename: Starting filename sanitization for: '{}'", s);
        
        let result = s.chars()
            .map(|c| match c {
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => {
                    trace!("StringUtils::sanitize_filename: Replacing invalid character '{}' with '_'", c);
                    '_'
                },
                c if c.is_control() => {
                    trace!("StringUtils::sanitize_filename: Replacing control character with '_'");
                    '_'
                },
                c => c,
            })
            .collect();
        
        debug!("StringUtils::sanitize_filename: Sanitized '{}' to '{}'", s, result);
        result
    }
    
    /// Truncate a string to a maximum length, adding ellipsis if needed
    pub fn truncate_with_ellipsis(s: &str, max_len: usize) -> String {
        trace!("StringUtils::truncate_with_ellipsis: Truncating string of {} chars to max {} chars", s.len(), max_len);
        
        if s.len() <= max_len {
            trace!("StringUtils::truncate_with_ellipsis: No truncation needed");
            s.to_string()
        } else if max_len <= 3 {
            trace!("StringUtils::truncate_with_ellipsis: Max length too small, returning ellipsis only");
            "...".to_string()
        } else {
            trace!("StringUtils::truncate_with_ellipsis: Truncating and adding ellipsis");
            let result = format!("{}...", &s[..max_len - 3]);
            trace!("StringUtils::truncate_with_ellipsis: Truncated to '{}'", result);
            result
        }
    }
    
    /// Parse a content ID from various formats (acestream://, magnet:, etc.)
    pub fn parse_content_id(input: &str) -> Result<String> {
        trace!("StringUtils::parse_content_id: Parsing content ID from input: '{}'", input);
        
        let input = input.trim();
        trace!("StringUtils::parse_content_id: Trimmed input: '{}'", input);
        
        // Handle acestream:// URLs
        if input.starts_with("acestream://") {
            trace!("StringUtils::parse_content_id: Detected acestream:// URL format");
            let id = input.strip_prefix("acestream://").unwrap();
            trace!("StringUtils::parse_content_id: Extracted ID from acestream URL: '{}' (length: {})", id, id.len());
            
            if id.len() == 40 && id.chars().all(|c| c.is_ascii_hexdigit()) {
                let result = id.to_lowercase();
                debug!("StringUtils::parse_content_id: Successfully parsed acestream URL to hash: '{}'", result);
                return Ok(result);
            } else {
                trace!("StringUtils::parse_content_id: Invalid acestream ID - length: {}, is_hex: {}", 
                      id.len(), id.chars().all(|c| c.is_ascii_hexdigit()));
            }
        }
        
        // Handle magnet links
        if input.starts_with("magnet:") {
            trace!("StringUtils::parse_content_id: Detected magnet link format");
            // Extract info hash from magnet link
            if let Some(start) = input.find("xt=urn:btih:") {
                let hash_start = start + 12;
                if let Some(end) = input[hash_start..].find('&') {
                    let hash = &input[hash_start..hash_start + end];
                    trace!("StringUtils::parse_content_id: Extracted hash from magnet (with &): '{}' (length: {})", hash, hash.len());
                    
                    if hash.len() == 40 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
                        let result = hash.to_lowercase();
                        debug!("StringUtils::parse_content_id: Successfully parsed magnet link to hash: '{}'", result);
                        return Ok(result);
                    }
                } else {
                    let hash = &input[hash_start..];
                    trace!("StringUtils::parse_content_id: Extracted hash from magnet (no &): '{}' (length: {})", hash, hash.len());
                    
                    if hash.len() == 40 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
                        let result = hash.to_lowercase();
                        debug!("StringUtils::parse_content_id: Successfully parsed magnet link to hash: '{}'", result);
                        return Ok(result);
                    }
                }
            } else {
                trace!("StringUtils::parse_content_id: No 'xt=urn:btih:' found in magnet link");
            }
        }
        
        // Handle raw content IDs
        trace!("StringUtils::parse_content_id: Testing for raw content ID format");
        if input.len() == 40 && input.chars().all(|c| c.is_ascii_hexdigit()) {
            let result = input.to_lowercase();
            debug!("StringUtils::parse_content_id: Successfully parsed raw content ID: '{}'", result);
            return Ok(result);
        }
        
        warn!("StringUtils::parse_content_id: Failed to parse content ID from input: '{}'", input);
        Err(OpenAceError::configuration(format!(
            "Invalid content ID format: {}", input
        )))
    }
    
    /// Generate a random content ID for testing purposes
    pub fn generate_test_content_id() -> String {
        trace!("StringUtils::generate_test_content_id: Generating test content ID");
        
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let content_id = (0..40)
            .map(|_| format!("{:x}", rng.gen_range(0..16)))
            .collect();
        
        debug!("StringUtils::generate_test_content_id: Generated test content ID: '{}'", content_id);
        content_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_sanitize_filename() {
        assert_eq!(StringUtils::sanitize_filename("test/file.txt"), "test_file.txt");
        assert_eq!(StringUtils::sanitize_filename("normal_file.txt"), "normal_file.txt");
        assert_eq!(StringUtils::sanitize_filename("file<>with|bad*chars?.txt"), "file__with_bad_chars_.txt");
    }
    
    #[test]
    fn test_truncate_with_ellipsis() {
        assert_eq!(StringUtils::truncate_with_ellipsis("short", 10), "short");
        assert_eq!(StringUtils::truncate_with_ellipsis("this is a long string", 10), "this is...");
        assert_eq!(StringUtils::truncate_with_ellipsis("abc", 2), "...");
    }
    
    #[test]
    fn test_parse_content_id() {
        // Test acestream:// URL
        let result = StringUtils::parse_content_id("acestream://1234567890abcdef1234567890abcdef12345678");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "1234567890abcdef1234567890abcdef12345678");
        
        // Test raw content ID
        let result = StringUtils::parse_content_id("1234567890ABCDEF1234567890ABCDEF12345678");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "1234567890abcdef1234567890abcdef12345678");
        
        // Test invalid format
        let result = StringUtils::parse_content_id("invalid");
        assert!(result.is_err());
    }
    
    #[test]
    fn test_generate_test_content_id() {
        let id = StringUtils::generate_test_content_id();
        assert_eq!(id.len(), 40);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }
}