#![cfg_attr(not(test), no_std)]

use embedded_io_async::{Read, Write};
use log::debug;

mod constants;
use constants::*;

mod error;
pub use error::*;

mod config;
pub use config::*;

// Represents the operational state of the sensor
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationalState {
    /// Sensor is in low-power sleep mode.
    Sleeping,
    /// Sensor is actively taking measurements.
    Working,
}

/// Represents an SDS011 air quality sensor.
///
/// This struct provides methods to interact with the sensor,
/// such as initializing it, reading data, and configuring its settings.
///
/// # Type Parameters
///
/// * `Serial`: The type of the serial interface used to communicate with the sensor.
///   It must implement `embedded_io_async::Read` and `embedded_io_async::Write`.
pub struct Sds011<Serial> {
    serial: Serial,
    config: Config,
}

/// Represents a single data sample read from the SDS011 sensor.
///
/// Contains PM2.5 and PM10 particulate matter concentration values.
#[derive(Debug, Clone, Copy)]
pub struct Sds011Data {
    /// PM2.5 concentration in µg/m³.
    pub pm2_5: f32,
    /// PM10 concentration in µg/m³.
    pub pm10: f32,
}

impl<S> Sds011<S>
where
    S: Read + Write,
{
    /// Creates a new `Sds011` sensor instance.
    ///
    /// # Arguments
    ///
    /// * `serial`: The serial interface for communication with the sensor.
    /// * `config`: The initial configuration for the sensor.
    ///
    /// # Returns
    ///
    /// A new `Sds011` instance.
    pub fn new(serial: S, config: Config) -> Self {
        Self { serial, config }
    }

    /// Initializes the SDS011 sensor according to the provided configuration.
    ///
    /// This involves:
    /// - Flushing the serial buffer.
    /// - Setting the reporting mode (Active or Passive).
    /// - If Passive mode, putting the sensor to sleep initially.
    /// - If Active mode, setting the working period to continuous.
    pub async fn init(&mut self) -> Result<(), Error> {
        self.serial.flush().await.map_err(|_| Error::WriteFailure)?;

        // Set the desired reporting mode (active or passive)
        self.set_reporting_mode_cmd(self.config.mode)
            .await
            .map_err(|e| {
                log::error!(
                    "Failed to set reporting mode to {:?} during init: {:?}",
                    self.config.mode,
                    e
                );
                e
            })?;

        if self.config.mode == DeviceMode::Passive {
            // In passive mode, put the sensor to sleep initially
            self.set_operational_state(OperationalState::Sleeping)
                .await
                .map_err(|e| {
                    log::error!(
                        "Failed to set state to sleep during init (Passive Mode): {:?}",
                        e
                    );
                    e
                })?;
        } else {
            // In active mode, set the working period to continuous (0)
            self.set_working_period_value(0x00).await.map_err(|e| {
                log::error!(
                    "Failed to set working period to continuous during init (Active Mode): {:?}",
                    e
                );
                e
            })?;
        }

        debug!("SDS011 init sequence complete.");
        Ok(())
    }

    /// Reads a single data sample from the SDS011 sensor.
    ///
    /// In Passive mode, this involves:
    /// 1. Waking the sensor up.
    /// 2. Waiting for a short period for the sensor to stabilize (implicitly handled by the sensor's response time).
    /// 3. Querying the data.
    /// 4. Putting the sensor back to sleep.
    ///
    /// In Active mode, this involves:
    /// 1. Querying the data (as the sensor is already working).
    ///
    /// Returns an `Sds011Data` struct containing PM2.5 and PM10 values, or an `Error` if the operation fails.
    pub async fn read_sample(&mut self) -> Result<Sds011Data, Error> {
        if self.config.mode == DeviceMode::Passive {
            debug!("Waking up sensor (Passive Mode)");
            self.set_operational_state(OperationalState::Working)
                .await
                .map_err(|e| {
                    log::error!("Failed to wake up sensor: {:?}", e);
                    e
                })?;

            debug!("Waiting for sensor to stabilize after wakeup...");
        }

        // For both active and passive (after wakeup), query data
        let buffer = self.query_data_cmd().await.map_err(|e| {
            log::error!("Failed to query sensor data: {:?}", e);
            e
        })?;

        let data = self.process_frame(&buffer).ok_or_else(|| {
            log::error!("Failed to process queried frame. Buffer: {:02X?}", buffer);
            Error::InvalidFrame
        })?;

        if self.config.mode == DeviceMode::Passive {
            debug!("Putting sensor back to sleep (Passive Mode)");
            self.set_operational_state(OperationalState::Sleeping)
                .await
                .map_err(|e| {
                    log::error!("Failed to put sensor to sleep: {:?}", e);
                    e
                })?;
        }
        Ok(data)
    }

    // Sends the query data command and returns the raw 10-byte reply.
    async fn query_data_cmd(&mut self) -> Result<[u8; 10], Error> {
        debug!("Querying sensor data (CMD 0x04)");
        let mut command = self.base_command();
        command[2] = 0x04; // Query data command
        self.write(&mut command).await?;
        self.read().await
    }

    /// Sets the sensor's reporting mode (Active or Passive).
    ///
    /// # Arguments
    ///
    /// * `mode`: The `DeviceMode` to set (Active or Passive).
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the command was successful and the sensor acknowledged the change.
    /// * `Err(Error)` if the command failed, the reply was unexpected, or a write/read error occurred.
    pub async fn set_reporting_mode_cmd(&mut self, mode: DeviceMode) -> Result<(), Error> {
        debug!("Setting reporting mode to: {:?}", mode);
        let mut command = self.base_command();
        command[2] = 0x02; // Reporting mode command
        command[3] = 0x01; // Set
        command[4] = if mode == DeviceMode::Active {
            0x00 // 0x00 for Active mode
        } else {
            0x01 // 0x01 for Passive mode
        };

        self.write(&mut command).await?;
        let buffer = self.read().await?; // Read the reply

        // Verify reply. The SDS011 sensor might reply with a standard command reply (0xC5) or, in
        // some cases, immediately send a data report (0xC0) if switching to active mode and data
        // is ready.
        if (buffer[1] == REPLY_ID
            && buffer[2] == 0x02
            && buffer[3] == 0x01
            && buffer[4] == command[4])
            || buffer[1] == DATA_REPORT_ID
        {
            self.config.mode = mode; // Update internal config on successful set
            debug!("Reporting mode set to {:?}, reply: {:02X?}", mode, buffer);
            Ok(())
        } else {
            log::error!(
                "Failed to set reporting mode, unexpected reply: {:02X?}",
                buffer
            );
            Err(Error::CommandFailed)
        }
    }

    /// Queries the sensor's current reporting mode.
    ///
    /// # Returns
    ///
    /// * `Ok(DeviceMode)` containing the current mode (Active or Passive).
    /// * `Err(Error)` if the command failed, the reply was unexpected, or a write/read error occurred.
    pub async fn get_reporting_mode(&mut self) -> Result<DeviceMode, Error> {
        debug!("Querying reporting mode (CMD 0x02, Query)");
        let mut command = self.base_command();
        command[2] = 0x02; // Reporting mode command
        command[3] = 0x00; // Query
        self.write(&mut command).await?;
        let buffer = self.read().await?;

        if buffer[1] == REPLY_ID && buffer[2] == 0x02 && buffer[3] == 0x00 {
            let mode = if buffer[4] == 0x00 {
                DeviceMode::Active
            } else {
                DeviceMode::Passive
            };
            debug!("Queried reporting mode: {:?}", mode);
            Ok(mode)
        } else {
            log::warn!(
                "get_reporting_mode: Unexpected reply structure: {:02X?}",
                buffer
            );
            Err(Error::UnexpectedReply)
        }
    }

    /// Sets the sensor's operational state (Sleeping or Working).
    ///
    /// # Arguments
    ///
    /// * `state`: The `OperationalState` to set (Working or Sleeping).
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the command was successful and the sensor acknowledged the change.
    /// * `Err(Error)` if the command failed, the reply was unexpected, or a write/read error occurred.
    pub async fn set_operational_state(&mut self, state: OperationalState) -> Result<(), Error> {
        debug!("Setting operational state to: {:?}", state);
        let mut command = self.base_command();
        command[2] = 0x06; // Set state command
        command[3] = 0x01; // Set
        command[4] = if state == OperationalState::Working {
            0x01
        } else {
            0x00
        }; // 1=Work, 0=Sleep

        self.write(&mut command).await?;
        let buffer = self.read().await?;

        if (buffer[1] == REPLY_ID
            && buffer[2] == 0x06
            && buffer[3] == 0x01
            && buffer[4] == command[4])
            || buffer[1] == DATA_REPORT_ID
        {
            debug!(
                "Operational state set to {:?}, reply: {:02X?}",
                state, buffer
            );
            Ok(())
        } else {
            log::error!(
                "Failed to set operational state, unexpected reply: {:02X?}",
                buffer
            );
            Err(Error::CommandFailed)
        }
    }

    /// Queries the sensor's current operational state.
    ///
    /// # Returns
    ///
    /// * `Ok(OperationalState)` containing the current state (Working or Sleeping).
    /// * `Err(Error)` if the command failed, the reply was unexpected, or a write/read error occurred.
    pub async fn get_operational_state(&mut self) -> Result<OperationalState, Error> {
        debug!("Querying operational state (CMD 0x06, Query)");
        let mut command = self.base_command();
        command[2] = 0x06; // Set state command
        command[3] = 0x00; // Query
        self.write(&mut command).await?;
        let buffer = self.read().await?;

        if buffer[1] == REPLY_ID && buffer[2] == 0x06 && buffer[3] == 0x00 {
            let state = if buffer[4] == 0x01 {
                OperationalState::Working
            } else {
                OperationalState::Sleeping
            };
            debug!("Queried operational state: {:?}", state);
            Ok(state)
        } else {
            log::warn!(
                "get_operational_state: Unexpected reply structure: {:02X?}",
                buffer
            );
            Err(Error::UnexpectedReply)
        }
    }

    /// Sets the sensor's working period.
    ///
    /// The working period determines how often the sensor takes measurements when in Active mode.
    /// - A value of `0` sets the sensor to continuous working mode (reports data as soon as it's available).
    /// - Values from `1` to `30` set the sensor to work for 30 seconds, then sleep for `(period - 1) * 60 + 30` seconds,
    ///   reporting data once per `period` minutes.
    ///
    /// # Arguments
    ///
    /// * `period`: The working period in minutes. Must be between 0 and 30 (inclusive).
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the command was successful and the sensor acknowledged the change.
    /// * `Err(Error::InvalidArg)` if `period` is greater than 30.
    /// * `Err(Error::CommandFailed)` if the command failed or the reply was unexpected.
    /// * `Err(Error::WriteFailure)` or `Err(Error::ReadFailure)` for serial communication issues.
    pub async fn set_working_period_value(&mut self, period: u8) -> Result<(), Error> {
        if period > 30 {
            log::error!("Working period {} out of range (0-30)", period);
            return Err(Error::InvalidArg); // Or a specific error for invalid period
        }
        debug!("Setting working period to: {} minutes", period);
        let mut command = self.base_command();
        command[2] = 0x08; // Set working period command
        command[3] = 0x01; // Set
        command[4] = period;

        self.write(&mut command).await?;
        let buffer = self.read().await?;

        if (buffer[1] == REPLY_ID && buffer[2] == 0x08 && buffer[3] == 0x01 && buffer[4] == period)
            || buffer[1] == DATA_REPORT_ID
        {
            debug!("Working period set to {}, reply: {:02X?}", period, buffer);
            Ok(())
        } else {
            log::error!(
                "Failed to set working period, unexpected reply: {:02X?}",
                buffer
            );
            Err(Error::CommandFailed)
        }
    }

    /// Queries the sensor's current working period.
    ///
    /// The working period defines how often the sensor takes measurements and reports data
    /// when in Active mode. A value of `0` means continuous mode. Values `1-30` correspond
    /// to reporting data once every `N` minutes.
    ///
    /// # Returns
    ///
    /// * `Ok(u8)` containing the current working period in minutes (0-30).
    /// * `Err(Error)` if the command failed, the reply was unexpected, or a write/read error occurred.
    pub async fn get_working_period(&mut self) -> Result<u8, Error> {
        debug!("Querying working period (CMD 0x08, Query)");
        let mut command = self.base_command();
        command[2] = 0x08; // Set working period command
        command[3] = 0x00; // Query
        self.write(&mut command).await?;
        let buffer = self.read().await?;

        if buffer[1] == REPLY_ID && buffer[2] == 0x08 && buffer[3] == 0x00 {
            let period = buffer[4];
            debug!("Queried working period: {} minutes", period);
            Ok(period)
        } else {
            log::warn!(
                "get_working_period: Unexpected reply structure: {:02X?}",
                buffer
            );
            Err(Error::UnexpectedReply)
        }
    }

    /// Sets the device ID of the sensor.
    ///
    /// The device ID is used to address the sensor when multiple sensors might be on the same bus,
    /// though typically only one SDS011 is used per serial interface.
    /// The default device ID is `0xFFFF`.
    ///
    /// # Arguments
    ///
    /// * `new_id1`: The new LSB (Least Significant Byte) of the device ID.
    /// * `new_id2`: The new MSB (Most Significant Byte) of the device ID.
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the command was successful, the sensor acknowledged the change,
    ///   and the internal configuration was updated.
    /// * `Err(Error::CommandFailed)` if the command failed or the reply was unexpected.
    /// * `Err(Error::WriteFailure)` or `Err(Error::ReadFailure)` for serial communication issues.
    pub async fn set_device_id(&mut self, new_id1: u8, new_id2: u8) -> Result<(), Error> {
        debug!("Setting device ID to: {:02X}{:02X}", new_id1, new_id2);
        let mut command = self.base_command(); // Uses current self.config.id for addressing
        command[2] = 0x05; // Set Device ID command
        command[13] = new_id1;
        command[14] = new_id2;
        // Bytes 15 & 16 (current device ID for addressing) are already set by base_command()

        self.write(&mut command).await?;
        let buffer = self.read().await?;

        // Expected reply: AA C5 05 00 00 00 NEW_ID1 NEW_ID2 CS AB
        if buffer[1] == REPLY_ID
            && buffer[2] == 0x05
            && buffer[6] == new_id1
            && buffer[7] == new_id2
        {
            self.config.id.id1 = new_id1;
            self.config.id.id2 = new_id2;
            debug!(
                "Device ID updated locally to {:02X}{:02X}. Reply: {:02X?}",
                new_id1, new_id2, buffer
            );
            Ok(())
        } else {
            log::error!("Failed to set device ID, unexpected reply: {:02X?}", buffer);
            Err(Error::CommandFailed)
        }
    }

    /// Retrieves the firmware version of the sensor.
    ///
    /// The firmware version is returned as a tuple `(year, month, day)`.
    /// For example, a firmware version of "15-10-21" (YY-MM-DD) would be returned as `(15, 10, 21)`.
    ///
    /// # Returns
    ///
    /// * `Ok((u8, u8, u8))` containing the year, month, and day of the firmware version.
    /// * `Err(Error)` if the command failed, the reply was unexpected, or a write/read error occurred.
    pub async fn get_firmware(&mut self) -> Result<(u8, u8, u8), Error> {
        debug!("Getting firmware version (CMD 0x07)");
        let mut command = self.base_command();
        command[2] = 0x07;
        self.write(&mut command).await?;
        let buffer = self.read().await?;

        // Expected reply: AA C5 07 YEAR MONTH DAY ID1 ID2 CS AB
        if buffer[1] == REPLY_ID && buffer[2] == 0x07 {
            let year = buffer[3];
            let month = buffer[4];
            let day = buffer[5];
            debug!("Firmware version: 20{}-{}-{}", year, month, day);
            Ok((year, month, day))
        } else {
            log::warn!("get_firmware: Unexpected reply structure: {:02X?}", buffer);
            Err(Error::UnexpectedReply)
        }
    }

    // Constructs a base 19-byte command frame.
    fn base_command(&self) -> [u8; 19] {
        [
            HEAD,
            COMMAND_ID,
            0x00, // Placeholder for specific command type
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,               // Data bytes
            self.config.id.id1, // Device ID LSB for addressing
            self.config.id.id2, // Device ID MSB for addressing
            0x00,               // Placeholder for checksum
            TAIL,
        ]
    }

    // Writes a 19-byte command to the serial port, calculating and inserting the checksum.
    async fn write(&mut self, command: &mut [u8; 19]) -> Result<(), Error> {
        let checksum: u8 = command[2..=16]
            .iter()
            .fold(0u8, |sum, &b| sum.wrapping_add(b));
        command[17] = checksum;

        debug!("Executing command: {:02X?}", command);
        self.serial.flush().await.map_err(|_| Error::WriteFailure)?;
        self.serial
            .write_all(command)
            .await
            .map_err(|_| Error::WriteFailure)?;
        self.serial.flush().await.map_err(|_| Error::WriteFailure)?; // Ensure data is sent
        Ok(())
    }

    // Reads a 10-byte frame from the serial port, attempting to synchronize and validate.
    async fn read(&mut self) -> Result<[u8; 10], Error> {
        let mut attempts = 0;
        const MAX_ATTEMPTS: usize = 5;

        loop {
            attempts += 1;
            if attempts > MAX_ATTEMPTS {
                log::error!(
                    "Failed to read a valid frame after {} attempts",
                    MAX_ATTEMPTS
                );
                return Err(Error::ReadFailure);
            }

            let mut read_buffer = [0u8; 20]; // Read more to increase chance of catching a frame
            let bytes_read = self.serial.read(&mut read_buffer).await.map_err(|e| {
                log::debug!("Serial read error during attempt {}: {:?}", attempts, e); // Assuming e is Debug
                Error::ReadFailure
            })?;

            if bytes_read < 10 {
                log::debug!("Read less than 10 bytes ({}), retrying.", bytes_read);
                continue;
            }

            // Search for HEAD...TAIL pattern
            if let Some(head_idx) = read_buffer[..bytes_read]
                .windows(10)
                .position(|window| window[0] == HEAD && window[9] == TAIL)
            {
                let mut frame = [0u8; 10];
                frame.copy_from_slice(&read_buffer[head_idx..head_idx + 10]);

                debug!("Potential frame found: {:02X?}", frame);

                // Validate Checksum (critical)
                let checksum_calc: u8 = frame[2..8].iter().copied().sum::<u8>();
                if checksum_calc != frame[8] {
                    log::error!("Bad checksum: Calculated {:02X}, Received {:02X}. Frame: {:02X?}. Retrying.", checksum_calc, frame[8], &frame);
                    continue; // Checksum failed, try to read again
                }

                // Optional: Log if command ID is unexpected, but don't fail read() for it here.
                // Specific command handlers will interpret frame[1] and frame[2].
                if frame[1] != REPLY_ID && frame[1] != DATA_REPORT_ID {
                    log::warn!(
                        "Frame has unexpected command ID: {:02X} (Expected {:02X} or {:02X})",
                        frame[1],
                        REPLY_ID,
                        DATA_REPORT_ID
                    );
                }

                log::debug!("Successfully read and validated frame: {:02X?}", frame);
                return Ok(frame);
            } else {
                log::debug!(
                    "No HEAD...TAIL pattern in {:02X?}. Retrying.",
                    &read_buffer[..bytes_read]
                );
            }
        }
    }

    // Processes a validated 10-byte frame to extract PM2.5 and PM10 data.
    fn process_frame(&self, data: &[u8; 10]) -> Option<Sds011Data> {
        let pm2_5_lsb_idx;
        let pm2_5_msb_idx;
        let pm10_lsb_idx;
        let pm10_msb_idx;

        match data[1] {
            DATA_REPORT_ID => {
                // Active mode report (0xC0) or data reply for some sensors
                // Frame: AA C0 PM25_L PM25_H PM10_L PM10_H ID0 ID1 CS AB
                pm2_5_lsb_idx = 2;
                pm2_5_msb_idx = 3;
                pm10_lsb_idx = 4;
                pm10_msb_idx = 5;
            }
            REPLY_ID if data[2] == 0x04 => {
                // Reply to Query Data command (0xC5, sub-cmd 0x04)
                // Frame: AA C5 04 PM25_L PM25_H PM10_L PM10_H ID0 ID1 CS AB
                pm2_5_lsb_idx = 3;
                pm2_5_msb_idx = 4;
                pm10_lsb_idx = 5;
                pm10_msb_idx = 6;
            }
            REPLY_ID => {
                log::debug!("process_frame: Received REPLY_ID frame, but not for a data query (cmd_id: {:02X}). Frame: {:02X?}", data[2], data);
                return None; // Not a data frame we can parse for PM values
            }
            _ => {
                log::error!(
                    "process_frame: Unexpected frame command ID {:02X} for PM data. Frame: {:02X?}",
                    data[1],
                    data
                );
                return None;
            }
        }

        let pm2_5 =
            (u16::from(data[pm2_5_lsb_idx]) | (u16::from(data[pm2_5_msb_idx]) << 8)) as f32 / 10.0;
        let pm10 =
            (u16::from(data[pm10_lsb_idx]) | (u16::from(data[pm10_msb_idx]) << 8)) as f32 / 10.0;

        debug!("Processed frame - PM2.5: {}, PM10: {}", pm2_5, pm10);
        Some(Sds011Data { pm2_5, pm10 })
    }
}

#[cfg(test)]
mod tests {
    use super::*; // Make Sds011, Config, etc. available
    use embedded_io_async::{Error, ErrorKind, ErrorType};
    use embedded_io_async::{Read, Write};
    use futures_executor::block_on; // For running async tests

    // A mock serial port for testing
    #[derive(Debug, Default)]
    struct MockSerial {
        write_buffer: Vec<u8>, // Stores bytes written to the mock serial
        read_buffer: Vec<u8>,  // Bytes to be returned by read operations
        read_pos: usize,       // Current position in the read_buffer
        flush_called: bool,    // Whether flush was called
        fail_write: bool,      // Simulate write failure
        fail_read: bool,       // Simulate read failure
        fail_flush: bool,      // Simulate flush failure
    }

    impl MockSerial {
        fn new(read_data: Vec<u8>) -> Self {
            MockSerial {
                write_buffer: Vec::new(),
                read_buffer: read_data,
                read_pos: 0,
                flush_called: false,
                fail_write: false,
                fail_read: false,
                fail_flush: false,
            }
        }

        // Helper to set expected data to be read by the SUT
        fn set_read_data(&mut self, data: Vec<u8>) {
            self.read_buffer = data;
            self.read_pos = 0;
        }

        // Helper to get bytes written by the SUT
        fn get_written_data(&self) -> &[u8] {
            &self.write_buffer
        }
    }

    impl Read for MockSerial {
        async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
            if self.fail_read {
                return Err(DummyError); // Simulate read error
            }
            // Simulate reading one frame (10 bytes) or less at a time,
            // or whatever is smaller: buf.len(), 10 bytes, or remaining data.
            let max_bytes_for_this_call = core::cmp::min(buf.len(), 10);
            let bytes_available_in_mock = self.read_buffer.len() - self.read_pos;
            let bytes_to_read = core::cmp::min(max_bytes_for_this_call, bytes_available_in_mock);

            if bytes_to_read > 0 {
                buf[..bytes_to_read].copy_from_slice(
                    &self.read_buffer[self.read_pos..self.read_pos + bytes_to_read],
                );
                self.read_pos += bytes_to_read;
                Ok(bytes_to_read)
            } else {
                // If read_pos is at the end of read_buffer, it means no more data.
                // If buf.len() was 0, this is also Ok(0).
                Ok(0)
            }
        }
    }

    impl Write for MockSerial {
        async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
            if self.fail_write {
                return Err(DummyError); // Simulate write error
            }
            self.write_buffer.extend_from_slice(buf);
            Ok(buf.len())
        }

        async fn flush(&mut self) -> Result<(), Self::Error> {
            if self.fail_flush {
                return Err(DummyError); // Simulate flush error
            }
            self.flush_called = true;
            Ok(())
        }
    }

    #[derive(Debug, Clone, Copy)]
    pub struct DummyError;

    impl Error for DummyError {
        fn kind(&self) -> ErrorKind {
            ErrorKind::Other
        }
    }

    // implement ErrorType
    impl ErrorType for MockSerial {
        type Error = DummyError;
    }

    // Helper to create a default config for tests
    fn default_config() -> Config {
        Config::new(DeviceID::default(), DeviceMode::Passive)
    }

    #[test]
    fn test_sds011_new() {
        let mock_serial = MockSerial::new(vec![]);
        let config = default_config();
        let sensor = Sds011::new(mock_serial, config);
        assert_eq!(sensor.config, config);
    }

    // Helper to calculate checksum for a command slice (excluding head, tail, and checksum byte itself)
    fn calculate_checksum(command_payload: &[u8]) -> u8 {
        command_payload
            .iter()
            .fold(0u8, |sum, &b| sum.wrapping_add(b))
    }

    #[test]
    fn test_init_passive_mode() {
        let mut mock_serial = MockSerial::new(vec![]);
        // Expected reply for set_reporting_mode_cmd (passive)
        // HEAD, REPLY_ID, CMD_SET_REPORTING_MODE, 0x01 (Set), 0x01 (Passive), 0,0,0, CHECKSUM, TAIL
        let reply_set_mode = vec![
            HEAD,
            REPLY_ID,
            0x02,
            0x01,
            0x01,
            0,
            0,
            0,
            (0x02 + 0x01 + 0x01),
            TAIL,
        ];
        // Expected reply for set_operational_state (sleep)
        // HEAD, REPLY_ID, CMD_SET_STATE, 0x01 (Set), 0x00 (Sleep), 0,0,0, CHECKSUM, TAIL
        let reply_set_sleep = vec![
            HEAD,
            REPLY_ID,
            0x06,
            0x01,
            0x00,
            0,
            0,
            0,
            #[allow(clippy::identity_op)]
            (0x06 + 0x01 + 0x00),
            TAIL,
        ];

        mock_serial.set_read_data([reply_set_mode, reply_set_sleep].concat());

        let config = Config::new(DeviceID::default(), DeviceMode::Passive);
        let mut sensor = Sds011::new(mock_serial, config);

        let result = block_on(sensor.init());
        assert!(result.is_ok(), "init failed: {:?}", result.err());

        let written_data = sensor.serial.get_written_data();
        assert!(
            written_data.len() >= 38,
            "Expected at least two 19-byte commands"
        );

        // Command 1: Set Reporting Mode to Passive
        // HEAD, CMD_ID, 0x02, 0x01, 0x01 (Passive), ..., ID1, ID2, CHECKSUM, TAIL
        let cmd1 = &written_data[0..19];
        assert_eq!(cmd1[0], HEAD);
        assert_eq!(cmd1[1], COMMAND_ID);
        assert_eq!(cmd1[2], 0x02); // Reporting mode command
        assert_eq!(cmd1[3], 0x01); // Set
        assert_eq!(cmd1[4], 0x01); // Passive
        assert_eq!(cmd1[15], DeviceID::default().id1);
        assert_eq!(cmd1[16], DeviceID::default().id2);
        let checksum1 = calculate_checksum(&cmd1[2..=16]);
        assert_eq!(cmd1[17], checksum1);
        assert_eq!(cmd1[18], TAIL);

        // Command 2: Set Operational State to Sleeping
        // HEAD, CMD_ID, 0x06, 0x01, 0x00 (Sleep), ..., ID1, ID2, CHECKSUM, TAIL
        let cmd2 = &written_data[19..38];
        assert_eq!(cmd2[0], HEAD);
        assert_eq!(cmd2[1], COMMAND_ID);
        assert_eq!(cmd2[2], 0x06); // Set state command
        assert_eq!(cmd2[3], 0x01); // Set
        assert_eq!(cmd2[4], 0x00); // Sleep
        assert_eq!(cmd2[15], DeviceID::default().id1);
        assert_eq!(cmd2[16], DeviceID::default().id2);
        let checksum2 = calculate_checksum(&cmd2[2..=16]);
        assert_eq!(cmd2[17], checksum2);
        assert_eq!(cmd2[18], TAIL);

        assert!(sensor.serial.flush_called);
    }

    #[test]
    fn test_init_active_mode() {
        let mut mock_serial = MockSerial::new(vec![]);
        // Expected reply for set_reporting_mode_cmd (active)
        let reply_set_mode = vec![
            HEAD,
            REPLY_ID,
            0x02,
            0x01,
            0x00,
            0,
            0,
            0,
            #[allow(clippy::identity_op)]
            (0x02 + 0x01 + 0x00),
            TAIL,
        ];
        // Expected reply for set_working_period (continuous)
        let reply_set_period = vec![
            HEAD,
            REPLY_ID,
            0x08,
            0x01,
            0x00,
            0,
            0,
            0,
            #[allow(clippy::identity_op)]
            (0x08 + 0x01 + 0x00),
            TAIL,
        ];
        mock_serial.set_read_data([reply_set_mode, reply_set_period].concat());

        let config = Config::new(DeviceID::default(), DeviceMode::Active);
        let mut sensor = Sds011::new(mock_serial, config);

        let result = block_on(sensor.init());
        assert!(result.is_ok(), "init failed: {:?}", result.err());

        let written_data = sensor.serial.get_written_data();
        assert!(
            written_data.len() >= 38,
            "Expected at least two 19-byte commands"
        );

        // Command 1: Set Reporting Mode to Active
        let cmd1 = &written_data[0..19];
        assert_eq!(cmd1[0], HEAD);
        assert_eq!(cmd1[1], COMMAND_ID);
        assert_eq!(cmd1[2], 0x02); // Reporting mode command
        assert_eq!(cmd1[3], 0x01); // Set
        assert_eq!(cmd1[4], 0x00); // Active
        let checksum1 = calculate_checksum(&cmd1[2..=16]);
        assert_eq!(cmd1[17], checksum1);
        assert_eq!(cmd1[18], TAIL);

        // Command 2: Set Working Period to Continuous (0)
        let cmd2 = &written_data[19..38];
        assert_eq!(cmd2[0], HEAD);
        assert_eq!(cmd2[1], COMMAND_ID);
        assert_eq!(cmd2[2], 0x08); // Set working period command
        assert_eq!(cmd2[3], 0x01); // Set
        assert_eq!(cmd2[4], 0x00); // Continuous
        let checksum2 = calculate_checksum(&cmd2[2..=16]);
        assert_eq!(cmd2[17], checksum2);
        assert_eq!(cmd2[18], TAIL);

        assert!(sensor.serial.flush_called);
    }
}
