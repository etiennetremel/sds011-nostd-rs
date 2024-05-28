#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum DeviceMode {
    Active,
    Passive,
}

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub struct DeviceID {
    pub id1: u8,
    pub id2: u8,
}

#[derive(Debug, Copy, Clone)]
pub struct Config {
    pub id: DeviceID,
    pub mode: DeviceMode,
}

impl Config {
    pub fn id(mut self, id: DeviceID) -> Self {
        self.id = id;
        self
    }
    pub fn mode(mut self, mode: DeviceMode) -> Self {
        self.mode = mode;
        self
    }
}

impl Default for Config {
    fn default() -> Config {
        Config {
            id: DeviceID {
                id1: 0xff,
                id2: 0xff,
            },
            mode: DeviceMode::Passive,
        }
    }
}
