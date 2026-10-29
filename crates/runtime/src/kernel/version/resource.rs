//! Version resource selection preserves its conventional name-1/first-entry fallback.
pub(super) fn resource(bytes: &[u8]) -> Result<&[u8], u32> {
    super::super::resources::resource(bytes, 16, &super::super::resources::Name::Version)
}
