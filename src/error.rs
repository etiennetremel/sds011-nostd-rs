#[derive(Debug)]
pub enum Error {
    EmptyDataFrame,
    BadChecksum,
    InvalidFrame,
    ReadFailure,
}
