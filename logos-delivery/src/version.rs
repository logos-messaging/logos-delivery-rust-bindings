//! Startup check that the linked `liblogosdelivery` is not older than the
//! revision the bindings were generated from.
//!
//! The library exposes no ABI constant: the only runtime signal is the
//! context-free `logosdelivery_version()` export, which answers
//! `version / git commit hash: v<nimble version>-g<short hash>`. Libraries from
//! before the CBOR FFI do not export it at all. The check therefore requires the
//! export and a nimble version at least `MIN_LIBRARY_VERSION` (recorded by
//! `scripts/gen-bindings.sh`); the hash is never compared, so newer compatible
//! libraries pass.

#[cfg(not(logosdelivery_static))]
use std::ffi::c_void;
use std::ffi::{c_char, CStr};
use std::sync::OnceLock;

use crate::error::{DeliveryError, Result};

const MIN_LIBRARY_VERSION: &str = include_str!("../MIN_LIBRARY_VERSION");

const NOT_EXPORTED: &str = "no logosdelivery_version export";

#[cfg(not(logosdelivery_static))]
extern "C" {
    fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
}

// A statically linked archive drops symbols nothing references, so dlsym cannot
// find the export there: reference it directly (the library is linked anyway).
#[cfg(logosdelivery_static)]
extern "C" {
    fn logosdelivery_version() -> *const c_char;
}

#[cfg(all(target_os = "macos", not(logosdelivery_static)))]
const RTLD_DEFAULT: *mut c_void = -2isize as *mut c_void;
#[cfg(all(not(target_os = "macos"), not(logosdelivery_static)))]
const RTLD_DEFAULT: *mut c_void = std::ptr::null_mut();

/// What the library reports about itself, read once per process.
fn reported_version() -> &'static Option<String> {
    static VERSION: OnceLock<Option<String>> = OnceLock::new();
    VERSION.get_or_init(|| {
        #[cfg(logosdelivery_static)]
        let version: unsafe extern "C" fn() -> *const c_char = logosdelivery_version;
        // Looked up at run time: a direct extern reference would turn an old
        // library into a link error instead of a clear one.
        #[cfg(not(logosdelivery_static))]
        let version: unsafe extern "C" fn() -> *const c_char = {
            let sym = unsafe { dlsym(RTLD_DEFAULT, c"logosdelivery_version".as_ptr()) };
            if sym.is_null() {
                return None;
            }
            unsafe { std::mem::transmute(sym) }
        };
        let ptr = unsafe { version() };
        if ptr.is_null() {
            return None;
        }
        Some(
            unsafe { CStr::from_ptr(ptr) }
                .to_string_lossy()
                .into_owned(),
        )
    })
}

/// `min` overrides the recorded minimum (tests only).
pub(crate) fn check_library(min: Option<&str>) -> Result<()> {
    let min = min.unwrap_or(MIN_LIBRARY_VERSION).trim();
    match reported_version() {
        None => Err(DeliveryError::VersionMismatch {
            expected: format!(">= {min}"),
            found: NOT_EXPORTED.into(),
        }),
        Some(reported) => compare(min, reported),
    }
}

fn compare(min: &str, reported: &str) -> Result<()> {
    let Some(min_parts) = parse_triple(min) else {
        tracing::warn!("cannot parse the minimum library version: {min}");
        return Ok(());
    };
    match parse_reported(reported) {
        None => {
            tracing::warn!("cannot parse the library version, skipping the check: {reported}");
            Ok(())
        }
        Some(found) if found >= min_parts => Ok(()),
        Some(_) => Err(DeliveryError::VersionMismatch {
            expected: format!(">= {min}"),
            found: reported.to_string(),
        }),
    }
}

/// Extracts `x.y.z` from `... : v0.39.0-g7e8375` (or `"v0.39.0-47-g7e8375"`).
fn parse_reported(reported: &str) -> Option<(u32, u32, u32)> {
    let tail = reported.rsplit(':').next()?;
    parse_triple(tail.trim().trim_matches('"'))
}

fn parse_triple(s: &str) -> Option<(u32, u32, u32)> {
    let s = s.strip_prefix('v').unwrap_or(s);
    let mut parts = s.splitn(3, '.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch_field = parts.next()?;
    let digits = patch_field
        .find(|c: char| !c.is_ascii_digit())
        .map_or(patch_field, |end| &patch_field[..end]);
    Some((major, minor, digits.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPORTED: &str = "version / git commit hash: v0.39.0-g7e8375";

    #[test]
    fn parses_reported_formats() {
        assert_eq!(parse_reported(REPORTED), Some((0, 39, 0)));
        assert_eq!(
            parse_reported("version / git commit hash: \"v0.39.0-47-g7e8375\""),
            Some((0, 39, 0))
        );
        assert_eq!(parse_reported("version / git commit hash: n/a"), None);
        assert_eq!(parse_reported("garbage"), None);
    }

    #[test]
    fn accepts_equal_and_newer() {
        assert!(compare("0.39.0", REPORTED).is_ok());
        assert!(compare("0.38.1", REPORTED).is_ok());
        assert!(compare("0.9.0", "x: v0.10.0-gabcdef").is_ok());
    }

    #[test]
    fn rejects_older() {
        let err = compare("0.40.0", REPORTED).unwrap_err();
        assert!(matches!(
            err,
            DeliveryError::VersionMismatch { ref expected, ref found }
                if expected == ">= 0.40.0" && found == REPORTED
        ));
        assert!(compare("0.39.1", REPORTED).is_err());
    }

    #[test]
    fn unparsable_version_is_not_fatal() {
        assert!(compare("0.39.0", "version / git commit hash: n/a").is_ok());
    }

    #[test]
    fn recorded_minimum_parses() {
        assert!(parse_triple(MIN_LIBRARY_VERSION.trim()).is_some());
    }
}
