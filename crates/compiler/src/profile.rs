//! Compatibility metadata for compiling a user-owned Complete Collection installation.

/// Executable identity, callback roots, DLL bases and startup defaults; no game bytes.
pub fn complete_profile_json() -> &'static str {
    include_str!("../profiles/complete-collection.json")
}

#[cfg(test)]
#[path = "../tests/unit/profile.rs"]
mod tests;
