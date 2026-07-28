//! Output density shared by all `nude` commands.
//!
//! Typing contract every resource must honour, so the output stays structured,
//! predictable, and queryable:
//!
//! - **compact** — flat rows of *primitive* cells only (string, int, bool,
//!   `date`, `filesize`, `nothing`). No lists or records: a compact table is
//!   meant to `where`/`sort-by`/`select` cleanly. Prefer a typed cell over a
//!   human blob — e.g. split Docker's `"Exited (0) 2h ago"` status into a
//!   `state` enum, an `exit_code` int, and a health string.
//! - **wide** / **full** — may nest lists and records for the full picture.
//!   `full` is the raw daemon payload converted verbatim.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    /// Minimal columns from the list endpoint (the default for lists).
    #[default]
    Compact,
    /// Rich columns from the inspect endpoint (the default for a named lookup).
    Wide,
    /// The raw inspect payload, converted verbatim.
    Full,
}

impl OutputFormat {
    /// Every format as `(value, description)`, for `--output` completion.
    /// Kept in step with [`FromStr`](std::str::FromStr) below.
    pub const ALL: &'static [(&'static str, &'static str)] = &[
        ("compact", "Minimal columns from the list endpoint"),
        ("wide", "Rich columns from the inspect endpoint"),
        ("full", "The raw inspect payload, converted verbatim"),
    ];
}

impl std::str::FromStr for OutputFormat {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "compact" => Ok(Self::Compact),
            "wide" => Ok(Self::Wide),
            "full" => Ok(Self::Full),
            _ => Err(()),
        }
    }
}
