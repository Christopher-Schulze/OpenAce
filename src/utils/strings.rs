//! String utilities for OpenAce Rust
//!
//! Provides safe string operations, encoding/decoding, and text processing
//! as modern alternatives to C string functions.

use crate::error::{OpenAceError, Result};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use tracing::{debug, warn, trace};
use encoding_rs::{Encoding, UTF_8, WINDOWS_1252, ISO_8859_10};

/// String encoding types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringEncoding {
    /// UTF-8 encoding
    Utf8,
    /// Windows-1252 encoding
    Windows1252,
    /// ISO-8859-1 (Latin-1) encoding
    Iso88591,
    /// ASCII encoding
    Ascii,
    /// Auto-detect encoding
    Auto,
}

impl StringEncoding {
    /// Get the encoding_rs Encoding for this type
    fn to_encoding_rs(self) -> &'static Encoding {
        match self {
            StringEncoding::Utf8 => UTF_8,
            StringEncoding::Windows1252 => WINDOWS_1252,
            // Per WHATWG, ISO-8859-1 is treated as Windows-1252
            StringEncoding::Iso88591 => WINDOWS_1252,
            StringEncoding::Ascii => UTF_8, // ASCII is a subset of UTF-8
            StringEncoding::Auto => UTF_8, // Default to UTF-8 for auto-detection
        }
    }
}

/// String decoding result
#[derive(Debug, Clone)]
pub struct DecodedString {
    /// The decoded string
    pub content: String,
    /// The detected or used encoding
    pub encoding: StringEncoding,
    /// Whether the decoding was lossy
    pub was_lossy: bool,
    /// Number of replacement characters used
    pub replacement_count: usize,
}

/// Decode a byte slice to a string with specified encoding
pub fn decode_bytes(bytes: &[u8], encoding: StringEncoding) -> Result<DecodedString> {
    trace!("decode_bytes: Starting decode operation with {} bytes, encoding: {:?}", bytes.len(), encoding);
    
    if bytes.is_empty() {
        trace!("decode_bytes: Empty byte slice, returning empty result");
        return Ok(DecodedString {
            content: String::new(),
            encoding,
            was_lossy: false,
            replacement_count: 0,
        });
    }
    
    trace!("decode_bytes: Determining actual encoding for decoding");
    let actual_encoding = if encoding == StringEncoding::Auto {
        trace!("decode_bytes: Auto-detection requested, detecting encoding");
        let detected = detect_encoding(bytes);
        trace!("decode_bytes: Auto-detected encoding: {:?}", detected);
        detected
    } else {
        trace!("decode_bytes: Using specified encoding: {:?}", encoding);
        encoding
    };
    
    trace!("decode_bytes: Converting to encoding_rs representation");
    let encoding_rs = actual_encoding.to_encoding_rs();
    trace!("decode_bytes: Performing decode operation with encoding: {}", encoding_rs.name());
    let (decoded, _used_encoding, had_errors) = encoding_rs.decode(bytes);
    
    // Count replacement characters
    trace!("decode_bytes: Counting replacement characters in decoded string");
    let replacement_count = decoded.matches('\u{FFFD}').count();
    
    debug!(
        "Decoded {} bytes using {:?}, lossy: {}, replacements: {}",
        bytes.len(),
        actual_encoding,
        had_errors,
        replacement_count
    );
    
    if had_errors {
        warn!("decode_bytes: Lossy decoding detected with {} replacement characters", replacement_count);
    }
    
    trace!("decode_bytes: Decode operation completed successfully");
    Ok(DecodedString {
        content: decoded.into_owned(),
        encoding: actual_encoding,
        was_lossy: had_errors,
        replacement_count,
    })
}

/// Encode a string to bytes with specified encoding
pub fn encode_string(text: &str, encoding: StringEncoding) -> Result<Vec<u8>> {
    trace!("encode_string: Starting encode operation with {} chars, encoding: {:?}", text.len(), encoding);
    
    if text.is_empty() {
        trace!("encode_string: Empty string, returning empty byte vector");
        return Ok(Vec::new());
    }
    
    trace!("encode_string: Converting to encoding_rs representation");
    let encoding_rs = encoding.to_encoding_rs();
    trace!("encode_string: Performing encode operation with encoding: {}", encoding_rs.name());
    let (encoded, _used_encoding, had_errors) = encoding_rs.encode(text);
    
    if had_errors {
        warn!("Lossy encoding when converting to {:?}", encoding);
        trace!("encode_string: Lossy encoding detected, some characters may have been replaced");
    }
    
    debug!(
        "Encoded {} chars to {} bytes using {:?}, lossy: {}",
        text.len(),
        encoded.len(),
        encoding,
        had_errors
    );
    
    trace!("encode_string: Encode operation completed successfully");
    Ok(encoded.into_owned())
}

/// Detect encoding of byte slice (simple heuristic)
fn detect_encoding(bytes: &[u8]) -> StringEncoding {
    trace!("detect_encoding: Starting encoding detection for {} bytes", bytes.len());
    
    // Check if it's valid UTF-8
    trace!("detect_encoding: Testing for valid UTF-8");
    if std::str::from_utf8(bytes).is_ok() {
        trace!("detect_encoding: Valid UTF-8 detected");
        return StringEncoding::Utf8;
    }
    
    // Check for common Windows-1252 byte patterns
    trace!("detect_encoding: Testing for Windows-1252 specific byte patterns");
    let has_windows_chars = bytes.iter().any(|&b| {
        matches!(b, 0x80..=0x9F) // Windows-1252 specific range
    });
    
    if has_windows_chars {
        trace!("detect_encoding: Windows-1252 specific characters detected");
        return StringEncoding::Windows1252;
    }
    
    // Check if it's valid ASCII
    trace!("detect_encoding: Testing for ASCII compatibility");
    if bytes.iter().all(|&b| b < 128) {
        trace!("detect_encoding: ASCII encoding detected");
        return StringEncoding::Ascii;
    }
    
    // Default to ISO-8859-1 for other cases
    trace!("detect_encoding: Defaulting to ISO-8859-1 encoding");
    StringEncoding::Iso88591
}

/// Safe C string conversion functions
pub mod c_strings {
    use super::*;
    
    /// Convert a C string pointer to a Rust String
    /// 
    /// # Safety
    /// The pointer must be valid and null-terminated
    pub unsafe fn c_str_to_string(ptr: *const c_char) -> Result<String> {
        trace!("c_str_to_string: Starting C string to Rust string conversion");
        
        if ptr.is_null() {
            trace!("c_str_to_string: Null pointer provided, returning empty string");
            return Ok(String::new());
        }
        
        trace!("c_str_to_string: Converting C string pointer to CStr");
        let c_str = CStr::from_ptr(ptr);
        let bytes = c_str.to_bytes();
        trace!("c_str_to_string: Extracted {} bytes from C string", bytes.len());
        
        // Try UTF-8 first, fall back to auto-detection
        trace!("c_str_to_string: Attempting UTF-8 conversion first");
        match std::str::from_utf8(bytes) {
            Ok(s) => {
                trace!("c_str_to_string: Successfully converted to UTF-8 string with {} chars", s.len());
                Ok(s.to_string())
            },
            Err(_) => {
                trace!("c_str_to_string: UTF-8 conversion failed, falling back to auto-detection");
                let decoded = decode_bytes(bytes, StringEncoding::Auto)?;
                if decoded.was_lossy {
                    warn!("c_str_to_string: Lossy conversion from C string with {} replacement characters", decoded.replacement_count);
                }
                trace!("c_str_to_string: Auto-detection completed, encoding: {:?}", decoded.encoding);
                Ok(decoded.content)
            }
        }
    }
    
    /// Convert a Rust string to a C string
    pub fn string_to_c_str(s: &str) -> Result<CString> {
        trace!("string_to_c_str: Converting Rust string of {} chars to C string", s.len());
        
        CString::new(s)
            .inspect(|_c_str| {
                trace!("string_to_c_str: Successfully created C string");
            })
            .map_err(|e| {
                warn!("string_to_c_str: String contains null byte: {}", e);
                OpenAceError::internal(format!("String contains null byte: {}", e))
            })
    }
    
    /// Convert a Rust string to a C string pointer (caller must free)
    pub fn string_to_c_ptr(s: &str) -> Result<*mut c_char> {
        trace!("string_to_c_ptr: Converting Rust string to C pointer");
        
        let c_string = string_to_c_str(s)?;
        let ptr = c_string.into_raw();
        
        trace!("string_to_c_ptr: Created C string pointer: {:p}", ptr);
        Ok(ptr)
    }
    
    /// Free a C string pointer created by string_to_c_ptr
    /// 
    /// # Safety
    /// The pointer must have been created by string_to_c_ptr
    pub unsafe fn free_c_ptr(ptr: *mut c_char) {
        trace!("free_c_ptr: Freeing C string pointer: {:p}", ptr);
        
        if !ptr.is_null() {
            let _ = CString::from_raw(ptr);
            trace!("free_c_ptr: Successfully freed C string pointer");
        } else {
            trace!("free_c_ptr: Null pointer provided, no action needed");
        }
    }
    
    /// Get the length of a C string
    /// 
    /// # Safety
    /// The pointer must be valid and null-terminated
    pub unsafe fn c_str_len(ptr: *const c_char) -> usize {
        trace!("c_str_len: Getting length of C string at pointer: {:p}", ptr);
        
        if ptr.is_null() {
            trace!("c_str_len: Null pointer provided, returning 0");
            return 0;
        }
        
        let c_str = CStr::from_ptr(ptr);
        let len = c_str.to_bytes().len();
        
        trace!("c_str_len: C string length: {} bytes", len);
        len
    }
}

/// String validation and sanitization
pub mod validation {
    use super::*;
    
    /// Check if a string is valid UTF-8
    pub fn is_valid_utf8(bytes: &[u8]) -> bool {
        trace!("is_valid_utf8: Validating {} bytes for UTF-8 compliance", bytes.len());
        let is_valid = std::str::from_utf8(bytes).is_ok();
        if is_valid {
            trace!("is_valid_utf8: Bytes are valid UTF-8");
        } else {
            trace!("is_valid_utf8: Bytes are not valid UTF-8");
        }
        is_valid
    }
    
    /// Check if a string contains only ASCII characters
    pub fn is_ascii(s: &str) -> bool {
        trace!("is_ascii: Checking if string with {} chars is ASCII", s.len());
        let is_ascii = s.is_ascii();
        if is_ascii {
            trace!("is_ascii: String contains only ASCII characters");
        } else {
            trace!("is_ascii: String contains non-ASCII characters");
        }
        is_ascii
    }
    
    /// Check if a string is safe for file names
    pub fn is_safe_filename(s: &str) -> bool {
        trace!("is_safe_filename: Validating filename safety for: '{}'", s);
        
        if s.is_empty() || s.len() > 255 {
            trace!("is_safe_filename: Filename failed length check - empty: {}, length: {}", s.is_empty(), s.len());
            return false;
        }
        
        // Check for invalid characters
        let invalid_chars = ['/', '\\', ':', '*', '?', '"', '<', '>', '|', '\0'];
        if s.chars().any(|c| invalid_chars.contains(&c) || c.is_control()) {
            trace!("is_safe_filename: Filename contains invalid characters");
            return false;
        }
        
        // Check for reserved names on Windows
        let reserved_names = [
            "CON", "PRN", "AUX", "NUL",
            "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
            "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ];
        
        let name_upper = s.to_uppercase();
        if reserved_names.contains(&name_upper.as_str()) {
            trace!("is_safe_filename: Filename matches reserved Windows name: {}", name_upper);
            return false;
        }
        
        // Check if it starts or ends with space or dot
        if s.starts_with(' ') || s.ends_with(' ') || s.ends_with('.') {
            trace!("is_safe_filename: Filename has invalid leading/trailing characters");
            return false;
        }
        
        trace!("is_safe_filename: Filename validation passed");
        true
    }
    
    /// Sanitize a string for use as a filename
    pub fn sanitize_filename(s: &str) -> String {
        if s.is_empty() {
            return "unnamed".to_string();
        }
        
        let mut result = s
            .chars()
            .map(|c| {
                if c.is_control() || ['/', '\\', ':', '*', '?', '"', '<', '>', '|'].contains(&c) {
                    '_'
                } else {
                    c
                }
            })
            .collect::<String>();
        
        // Trim spaces and dots
        result = result.trim_matches(' ').trim_end_matches('.').to_string();
        
        // Ensure it's not empty after sanitization
        if result.is_empty() {
            result = "unnamed".to_string();
        }
        
        // Truncate if too long
        if result.len() > 255 {
            result.truncate(255);
        }
        
        result
    }
    
    /// Check if a string is a valid URL
    pub fn is_valid_url(s: &str) -> bool {
        trace!("is_valid_url: Validating URL: '{}'", s);
        let is_valid = url::Url::parse(s).is_ok();
        if is_valid {
            trace!("is_valid_url: URL validation passed");
        } else {
            trace!("is_valid_url: URL validation failed");
        }
        is_valid
    }
    
    /// Check if a string is a valid email address (basic check)
    pub fn is_valid_email(s: &str) -> bool {
        trace!("is_valid_email: Validating email: '{}'", s);
        
        let parts: Vec<&str> = s.split('@').collect();
        if parts.len() != 2 {
            trace!("is_valid_email: Email validation failed - incorrect number of @ symbols: {}", parts.len());
            return false;
        }
        
        let local = parts[0];
        let domain = parts[1];
        
        if local.is_empty() || domain.is_empty() {
            trace!("is_valid_email: Email validation failed - empty local or domain part");
            return false;
        }
        
        if local.len() > 64 || domain.len() > 253 {
            trace!("is_valid_email: Email validation failed - length limits exceeded (local: {}, domain: {})", local.len(), domain.len());
            return false;
        }
        
        // Basic character validation
        let valid_local_chars = |c: char| c.is_alphanumeric() || "!#$%&'*+-/=?^_`{|}~.".contains(c);
        let valid_domain_chars = |c: char| c.is_alphanumeric() || ".-".contains(c);
        
        let local_valid = local.chars().all(valid_local_chars);
        let domain_valid = domain.chars().all(valid_domain_chars);
        
        if !local_valid {
            trace!("is_valid_email: Email validation failed - invalid characters in local part");
        }
        if !domain_valid {
            trace!("is_valid_email: Email validation failed - invalid characters in domain part");
        }
        
        let is_valid = local_valid && domain_valid;
        if is_valid {
            trace!("is_valid_email: Email validation passed");
        }
        
        is_valid
    }
}

/// String formatting and manipulation
pub mod formatting {
    use super::*;
    
    /// Truncate a string to a maximum length with ellipsis
    pub fn truncate_with_ellipsis(s: &str, max_len: usize) -> String {
        trace!("truncate_with_ellipsis: Truncating string of {} chars to max {} chars", s.len(), max_len);
        
        if s.len() <= max_len {
            trace!("truncate_with_ellipsis: No truncation needed");
            s.to_string()
        } else if max_len <= 3 {
            trace!("truncate_with_ellipsis: Max length too small, returning ellipsis only");
            "...".to_string()
        } else {
            trace!("truncate_with_ellipsis: Truncating and adding ellipsis");
            format!("{}...", &s[..max_len - 3])
        }
    }
    
    /// Convert a string to title case
    pub fn to_title_case(s: &str) -> String {
        trace!("to_title_case: Converting '{}' to title case", s);
        
        let result = s.split_whitespace()
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        
        trace!("to_title_case: Converted to '{}'", result);
        result
    }
    
    /// Convert a string to snake_case
    pub fn to_snake_case(s: &str) -> String {
        trace!("to_snake_case: Converting '{}' to snake_case", s);
        
        let mut result = String::new();
        let mut prev_was_upper = false;
        
        for (i, c) in s.chars().enumerate() {
            if c.is_uppercase() {
                if i > 0 && !prev_was_upper {
                    result.push('_');
                }
                result.push(c.to_lowercase().next().unwrap());
                prev_was_upper = true;
            } else if c.is_whitespace() || c == '-' {
                if !result.ends_with('_') {
                    result.push('_');
                }
                prev_was_upper = false;
            } else {
                result.push(c);
                prev_was_upper = false;
            }
        }
        
        trace!("to_snake_case: Converted to '{}'", result);
        result
    }
    
    /// Convert a string to kebab-case
    pub fn to_kebab_case(s: &str) -> String {
        trace!("to_kebab_case: Converting '{}' to kebab-case", s);
        let result = to_snake_case(s).replace('_', "-");
        trace!("to_kebab_case: Converted to '{}'", result);
        result
    }
    
    /// Convert a string to PascalCase
    pub fn to_pascal_case(s: &str) -> String {
        trace!("to_pascal_case: Converting '{}' to PascalCase", s);
        
        let result = s.split(|c: char| c.is_whitespace() || c == '_' || c == '-')
            .filter(|word| !word.is_empty())
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
                }
            })
            .collect();
        
        trace!("to_pascal_case: Converted to '{}'", result);
        result
    }
    
    /// Convert a string to camelCase
    pub fn to_camel_case(s: &str) -> String {
        trace!("to_camel_case: Converting '{}' to camelCase", s);
        
        let pascal = to_pascal_case(s);
        if pascal.is_empty() {
            trace!("to_camel_case: Empty pascal case result, returning empty string");
            return pascal;
        }
        
        let result = {
            let mut chars = pascal.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
            }
        };
        
        trace!("to_camel_case: Converted to '{}'", result);
        result
    }
    
    /// Pad a string to a specific width
    pub fn pad_string(s: &str, width: usize, pad_char: char, align: Alignment) -> String {
        if s.len() >= width {
            return s.to_string();
        }
        
        let padding = width - s.len();
        
        match align {
            Alignment::Left => format!("{}{}", s, pad_char.to_string().repeat(padding)),
            Alignment::Right => format!("{}{}", pad_char.to_string().repeat(padding), s),
            Alignment::Center => {
                let left_padding = padding / 2;
                let right_padding = padding - left_padding;
                format!(
                    "{}{}{}",
                    pad_char.to_string().repeat(left_padding),
                    s,
                    pad_char.to_string().repeat(right_padding)
                )
            }
        }
    }
    
    /// String alignment options
    #[derive(Debug, Clone, Copy)]
    pub enum Alignment {
        Left,
        Right,
        Center,
    }
}

/// String hashing and comparison
pub mod hashing {
    use super::*;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    /// Calculate a hash of a string
    pub fn hash_string(s: &str) -> u64 {
        trace!("hash_string: Calculating hash for string of {} chars", s.len());
        let mut hasher = DefaultHasher::new();
        s.hash(&mut hasher);
        let hash = hasher.finish();
        trace!("hash_string: Generated hash: {}", hash);
        hash
    }
    
    /// Calculate Levenshtein distance between two strings
    pub fn levenshtein_distance(s1: &str, s2: &str) -> usize {
        trace!("levenshtein_distance: Calculating distance between strings of {} and {} chars", s1.len(), s2.len());
        
        let len1 = s1.chars().count();
        let len2 = s2.chars().count();
        
        if len1 == 0 {
            trace!("levenshtein_distance: First string empty, returning {}", len2);
            return len2;
        }
        if len2 == 0 {
            trace!("levenshtein_distance: Second string empty, returning {}", len1);
            return len1;
        }
        
        let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];
        
        // Initialize first row and column
        for (i, row) in matrix.iter_mut().enumerate().take(len1 + 1) {
            row[0] = i;
        }
        for j in 0..=len2 {
            matrix[0][j] = j;
        }
        
        let chars1: Vec<char> = s1.chars().collect();
        let chars2: Vec<char> = s2.chars().collect();
        
        for i in 1..=len1 {
            for j in 1..=len2 {
                let cost = if chars1[i - 1] == chars2[j - 1] { 0 } else { 1 };
                
                matrix[i][j] = std::cmp::min(
                    std::cmp::min(
                        matrix[i - 1][j] + 1,     // deletion
                        matrix[i][j - 1] + 1,     // insertion
                    ),
                    matrix[i - 1][j - 1] + cost, // substitution
                );
            }
        }
        
        let distance = matrix[len1][len2];
        trace!("levenshtein_distance: Calculated distance: {}", distance);
        distance
    }
    
    /// Calculate string similarity (0.0 to 1.0)
    pub fn string_similarity(s1: &str, s2: &str) -> f64 {
        trace!("string_similarity: Calculating similarity between strings of {} and {} chars", s1.len(), s2.len());
        
        let max_len = std::cmp::max(s1.chars().count(), s2.chars().count());
        if max_len == 0 {
            trace!("string_similarity: Both strings empty, returning 1.0");
            return 1.0;
        }
        
        let distance = levenshtein_distance(s1, s2);
        let similarity = 1.0 - (distance as f64 / max_len as f64);
        
        trace!("string_similarity: Calculated similarity: {} (distance: {}, max_len: {})", similarity, distance, max_len);
        similarity
    }
}

/// Initialize string utilities
pub fn initialize_strings() -> Result<()> {
    trace!("initialize_strings: Starting string utilities initialization");
    
    // Initialize any global string processing state if needed
    trace!("initialize_strings: Setting up encoding detection tables");
    
    // Verify encoding_rs is available
    trace!("initialize_strings: Verifying encoding_rs availability");
    let _utf8_test = UTF_8.name();
    let _windows1252_test = WINDOWS_1252.name();
    let _iso88591_test = ISO_8859_10.name();
    
    trace!("initialize_strings: All encoding systems verified");
    
    debug!("String utilities initialized successfully");
    Ok(())
}

/// Shutdown string utilities
pub fn shutdown_strings() -> Result<()> {
    trace!("shutdown_strings: Starting string utilities shutdown");
    
    // Clean up any global string processing state if needed
    trace!("shutdown_strings: Cleaning up encoding detection state");
    
    // No specific cleanup needed for encoding_rs
    trace!("shutdown_strings: All string processing resources cleaned up");
    
    debug!("String utilities shutdown completed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_decode_utf8() {
        let text = "Hello, 世界!";
        let bytes = text.as_bytes();
        let result = decode_bytes(bytes, StringEncoding::Utf8).unwrap();
        
        assert_eq!(result.content, text);
        assert_eq!(result.encoding, StringEncoding::Utf8);
        assert!(!result.was_lossy);
        assert_eq!(result.replacement_count, 0);
    }
    
    #[test]
    fn test_filename_validation() {
        assert!(validation::is_safe_filename("valid_file.txt"));
        assert!(!validation::is_safe_filename("invalid/file.txt"));
        assert!(!validation::is_safe_filename("CON"));
        assert!(!validation::is_safe_filename(""));
    }
    
    #[test]
    fn test_case_conversion() {
        assert_eq!(formatting::to_snake_case("HelloWorld"), "hello_world");
        assert_eq!(formatting::to_kebab_case("HelloWorld"), "hello-world");
        assert_eq!(formatting::to_pascal_case("hello_world"), "HelloWorld");
        assert_eq!(formatting::to_camel_case("hello_world"), "helloWorld");
    }
    
    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(hashing::levenshtein_distance("kitten", "sitting"), 3);
        assert_eq!(hashing::levenshtein_distance("hello", "hello"), 0);
        assert_eq!(hashing::levenshtein_distance("", "abc"), 3);
    }
}