use std::{fmt::{Display, Formatter, Result}, error::Error};

#[derive(Debug)]
pub struct NullError;

impl Display for NullError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "NoError")
    }
}

impl Error for NullError {}