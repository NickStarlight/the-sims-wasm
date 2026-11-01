use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidRange,
    UnknownCpuHelper(u32),
    InvalidClock,
    Overlap,
    Unmapped,
    Protected,
    InvalidAllocation,
    OutOfMemory,
    Host(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::UnknownCpuHelper(id) => return write!(f, "unknown CPU helper {id}"),
            Self::InvalidClock => "invalid monotonic clock for CPU helper",
            Self::InvalidRange => "invalid guest address range",
            Self::Overlap => "guest range overlaps an existing mapping",
            Self::Unmapped => "guest range is not mapped",
            Self::Protected => "guest range is protected",
            Self::InvalidAllocation => "address is not a live allocation owned by this process",
            Self::OutOfMemory => "guest allocation exceeds the process memory limit",
            Self::Host(message) => return write!(f, "guest memory operation failed: {message}"),
        })
    }
}

impl std::error::Error for Error {}
