//! Shim for yanked core2 0.4.0. Re-exports std::io traits.
pub use std::io::{self, Read, Write, Error, ErrorKind, Result, Cursor, BufRead, Seek, SeekFrom};
