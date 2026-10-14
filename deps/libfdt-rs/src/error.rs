use crate::myctypes::c_int;
use core::fmt;
use core::num::TryFromIntError;

#[derive(Debug, Clone, Copy)]
pub enum Error {
    Raw(c_int),
    TryFromInt(TryFromIntError),
    Invalid,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl core::error::Error for Error {}

pub type Result<T> = core::result::Result<T, Error>;

impl From<TryFromIntError> for Error {
    fn from(e: TryFromIntError) -> Self {
        Self::TryFromInt(e)
    }
}

/// Converts a libfdt style return code into a `Result`.
pub fn wrap_unit(code: c_int) -> Result<()> {
    if code < 0 {
        Err(Error::Raw(code))
    } else {
        Ok(())
    }
}
