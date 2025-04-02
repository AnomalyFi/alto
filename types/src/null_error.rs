use std::{
    error::Error,
    fmt::{Display, Formatter, Result},
};

#[derive(Debug)]
pub struct NullError;

impl Display for NullError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "NoError")
    }
}

impl Error for NullError {}
