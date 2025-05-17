/// Represents the operating mode of the SDS011 sensor.
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum DeviceMode {
    /// In Active mode, the sensor automatically reports data.
    Active,
    /// In Passive mode, the sensor only reports data when queried.
    Passive,
}

/// Represents the unique identifier of the SDS011 sensor.
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub struct DeviceID {
    /// The first byte of the device ID.
    pub id1: u8,
    /// The second byte of the device ID.
    pub id2: u8,
}

impl Default for DeviceID {
    /// Returns the default device id.
    fn default() -> DeviceID {
        DeviceID {
            id1: 0xff,
            id2: 0xff,
        }
    }
}

/// Configuration settings for the SDS011 sensor.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Config {
    /// The device ID.
    pub id: DeviceID,
    /// The operating mode of the sensor.
    pub mode: DeviceMode,
}

impl Config {
    /// Creates a new `Config` instance.
    ///
    /// # Arguments
    ///
    /// * `id` - The `DeviceID` for the sensor.
    /// * `mode` - The `DeviceMode` for the sensor.
    ///
    /// # Returns
    ///
    /// A new `Config` instance with the specified ID and mode.
    pub fn new(id: DeviceID, mode: DeviceMode) -> Config {
        Config { id, mode }
    }
    /// Sets the device ID for the configuration.
    ///
    /// # Arguments
    ///
    /// * `id` - The `DeviceID` to set.
    ///
    /// # Returns
    ///
    /// The updated `Config` instance.
    pub fn id(mut self, id: DeviceID) -> Self {
        self.id = id;
        self
    }
    /// Sets the device mode for the configuration.
    ///
    /// # Arguments
    ///
    /// * `mode` - The `DeviceMode` to set.
    ///
    /// # Returns
    ///
    /// The updated `Config` instance.
    pub fn mode(mut self, mode: DeviceMode) -> Self {
        self.mode = mode;
        self
    }
}

/// Provides default configuration values for the SDS011 sensor.
impl Default for Config {
    /// Returns the default configuration.
    ///
    /// The default configuration uses a device ID of `0xFFFF` and `Passive` mode.
    fn default() -> Config {
        Config {
            id: DeviceID::default(),
            mode: DeviceMode::Passive,
        }
    }
}
