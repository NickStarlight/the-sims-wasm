//! Recoverable failures for decoded instructions the emitter cannot translate.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmitError {
    pub guest_va: u32,
    pub message: String,
}

impl EmitError {
    pub fn new(guest_va: u32, message: impl Into<String>) -> Self {
        Self {
            guest_va,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for EmitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "instruction at {:#x}: {}", self.guest_va, self.message)
    }
}

impl std::error::Error for EmitError {}

#[cfg(test)]
#[path = "../../tests/unit/translate/error.rs"]
mod tests;
