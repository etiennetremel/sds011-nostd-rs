#![cfg_attr(not(test), no_std)]

use embedded_io_async::{Read, Write};
use log::debug;

mod constants;
pub use constants::*;

mod error;
pub use error::*;

mod config;
pub use config::*;

pub struct Sds011<Serial> {
    serial: Serial,
    config: Config,
}

pub struct Sds011Data {
    pub pm2_5: f32,
    pub pm10: f32,
}

impl<S> Sds011<S>
where
    S: Read + Write,
{
    pub fn new(serial: S, config: Config) -> Self {
        Self { serial, config }
    }

    pub async fn init(&mut self) -> Result<(), Error> {
        self.serial.flush().await.ok();

        self.set_reporting_mode(false).await?;

        if self.config.mode == DeviceMode::Passive {
            self.set_state(false, true).await?;
        } else {
            self.set_working_period(false, 0x00).await?;
        }

        debug!("Waiting for first reply...");
        // not sure why, this seems to be required. The sensor reply the first
        // command on start
        let mut buf = [0u8; 19];
        self.serial.flush().await.ok();
        self.serial
            .read_exact(&mut buf)
            .await
            .map_err(|_| Error::ReadFailure)
            .ok();

        Ok(())
    }

    pub async fn read_sample(&mut self) -> Result<Sds011Data, Error> {
        self.serial.flush().await.ok();

        let buffer = self.query().await?;
        let data = self.process_frame(&buffer).ok_or(Error::InvalidFrame)?;

        self.set_state(false, true).await?;

        Ok(data)
    }

    pub async fn query(&mut self) -> Result<[u8; 10], Error> {
        debug!("Querying sensor");

        let mut command = self.base_command();

        command[2] = 0x04;

        self.write(&mut command).await?;
        let buffer = self.read().await?;
        Ok(buffer)
    }

    pub async fn set_working_period(&mut self, query: bool, period: u8) -> Result<[u8; 10], Error> {
        debug!("Sending set working period to {:?}", period);

        let mut command = self.base_command();

        command[2] = 0x08;
        command[3] = if query { 0x00 } else { 0x01 }; // query or set working period
        command[4] = period;

        self.write(&mut command).await?;

        if query {
            let buffer = self.read().await?;
            return Ok(buffer);
        }

        Ok([0u8; 10])
    }

    pub async fn set_state(&mut self, query: bool, sleep: bool) -> Result<[u8; 10], Error> {
        debug!("Setting state sleep={:?}", sleep);

        let mut command = self.base_command();

        command[2] = 0x06;
        command[3] = if query { 0x00 } else { 0x01 }; // query or set state
        command[4] = if sleep { 0x00 } else { 0x01 }; // set to sleep or work

        self.write(&mut command).await?;

        if query {
            let buffer = self.read().await?;
            return Ok(buffer);
        }

        Ok([0u8; 10])
    }

    pub async fn set_reporting_mode(&mut self, query: bool) -> Result<[u8; 10], Error> {
        debug!("Setting mode to {:?}", self.config.mode);

        let mut command = self.base_command();

        command[2] = 0x02;

        // query or set reporting mode
        command[3] = if query { 0x00 } else { 0x01 };

        // report active or passive mode
        command[4] = if self.config.mode == DeviceMode::Active {
            0x00
        } else {
            0x01
        };

        self.write(&mut command).await?;

        if query {
            let buffer = self.read().await?;
            return Ok(buffer);
        }

        Ok([0u8; 10])
    }

    pub async fn set_device_id(&mut self, id1: u8, id2: u8) -> Result<[u8; 10], Error> {
        let mut command = self.base_command();

        command[2] = 0x05;
        command[12] = id1;
        command[13] = id2;

        self.write(&mut command).await?;
        let buffer = self.read().await?;
        Ok(buffer)
    }

    pub async fn get_firmware(&mut self) -> Result<[u8; 10], Error> {
        let mut command = self.base_command();

        command[2] = 0x07;

        self.write(&mut command).await?;
        let buffer = self.read().await?;
        Ok(buffer)
    }

    fn base_command(&mut self) -> [u8; 19] {
        [
            HEAD, COMMAND_ID, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0xFF, 0xFF, TAIL,
        ]
    }

    async fn write(&mut self, command: &mut [u8; 19]) -> Result<(), Error> {
        command[15] = self.config.id.id1;
        command[16] = self.config.id.id2;

        let checksum: u8 = command[2..17]
            .iter()
            .fold(0u8, |sum, &b| sum.wrapping_add(b));
        command[17] = checksum;

        debug!("Executing command: {:?}", command);
        self.serial.write_all(command).await.ok();

        Ok(())
    }

    async fn read(&mut self) -> Result<[u8; 10], Error> {
        let mut buf = [0u8; 10];
        self.serial
            .read_exact(&mut buf)
            .await
            .map_err(|_| Error::ReadFailure)?;

        debug!("Read buffer: {:?}", buf);

        // Check for the correct start and end bytes
        if buf[0] != 0xAA || buf[9] != 0xAB {
            return Err(Error::InvalidFrame);
        }

        let data = &buf[2..8];
        if data.is_empty() {
            return Err(Error::EmptyDataFrame);
        }

        let checksum: u8 = data.iter().copied().sum::<u8>();
        if checksum != buf[8] {
            return Err(Error::BadChecksum);
        }

        Ok(buf)
    }

    fn process_frame(&self, data: &[u8; 10]) -> Option<Sds011Data> {
        let pm2_5 = (u16::from(data[2]) | (u16::from(data[3]) << 8)) as f32 / 10.0;
        let pm10 = (u16::from(data[4]) | (u16::from(data[5]) << 8)) as f32 / 10.0;

        let checksum: u8 = data[2..8].iter().copied().sum::<u8>();

        if checksum != data[8] {
            debug!(
                "Checksum mismatch in process_frame: calculated {}, received {}",
                checksum, data[8]
            );
            return None;
        }

        debug!("Processed frame - PM2.5: {}, PM10: {}", pm2_5, pm10);
        Some(Sds011Data { pm2_5, pm10 })
    }
}
