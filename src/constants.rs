// COMMAND_ID is the byte that identifies a command frame sent to the sensor.
pub(crate) const COMMAND_ID: u8 = 0xB4;

// DATA_REPORT_ID is the byte that identifies a data report frame received from the sensor.
// This is used in active reporting mode or as a reply to some commands.
pub(crate) const DATA_REPORT_ID: u8 = 0xC0;

// REPLY_ID is the byte that identifies a reply frame received from the sensor
// in response to a command.
pub(crate) const REPLY_ID: u8 = 0xC5;

// HEAD is the byte that marks the beginning of any frame (command or data).
pub(crate) const HEAD: u8 = 0xAA;

// TAIL is the byte that marks the end of any frame (command or data).
pub(crate) const TAIL: u8 = 0xAB;
