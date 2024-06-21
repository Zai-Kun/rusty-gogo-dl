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

#[derive(Debug)]
pub struct GogoFetchingDetailsFailed;

impl fmt::Display for GogoFetchingDetailsFailed {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.write_str("Failed to fetch anime details")
    }
}

impl Context for GogoFetchingDetailsFailed {}

#[derive(Debug)]
pub struct GogoFailedToFetchDownloadLinks;

impl fmt::Display for GogoFailedToFetchDownloadLinks {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.write_str("Failed to fetch download links for the episode")
    }
}

impl Context for GogoFailedToFetchDownloadLinks {}
