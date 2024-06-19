use error_stack::Context;
use std::fmt;

#[derive(Debug)]
pub struct GogoInitError;

impl fmt::Display for GogoInitError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.write_str("Failed to initialize Gogo")
    }
}

impl Context for GogoInitError {}

#[derive(Debug)]
pub struct GogoSearchFailedError;

impl fmt::Display for GogoSearchFailedError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.write_str("Failed to search anime on Gogo")
    }
}

impl Context for GogoSearchFailedError {}
