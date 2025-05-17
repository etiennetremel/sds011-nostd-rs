#[derive(Debug)]
pub enum Error {
    /// The checksum received does not match the calculated checksum.
    BadChecksum,
    /// The command sent to the sensor failed.
    CommandFailed,
    /// The data frame received from the sensor is empty.
    EmptyDataFrame,
    /// An invalid argument was provided to a function.
    InvalidArg,
    /// The frame received from the sensor is invalid.
    InvalidFrame,
    /// Failed to read from the serial port.
    ReadFailure,
    /// The reply received from the sensor was not expected.
    UnexpectedReply,
    /// Failed to write to the serial port.
    WriteFailure,
}
