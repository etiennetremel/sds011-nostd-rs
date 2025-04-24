# Rust SDS011 driver no_std environments compatible

This library provide an interface of the SDS011 air quality sensor using
asynchronous I/O traits compatible with no_std environments.

## Getting started

Below is an example demonstrating how to initialize the SDS011 sensor, set it
up, and read air quality data from it:

```rust
#![no_std]

// ...

use sds011_nostd_rs::Sds011

// ...

let uart = ...

// Initialize the sensor configuration
let config = Config {
    id: DeviceID {
        id1: 0xFF,
        id2: 0xFF,
    },
    mode: DeviceMode::Passive,
};

// Create a new instance of the sensor
let mut sds011 = Sds011::new(uart, config);

// Initialize the sensor
sds011.init().await.unwrap();

// Read a sample from the sensor
let sample = sds011.read_sample().await.unwrap();

// Print the sample values
println!("PM2.5: {} µg/m³, PM10: {} µg/m³", sample.pm2_5, sample.pm10);
```

## Example

Example implementation of this library on a esp32 chip can be found in the
[etiennetremel/esp32-home-sensor][esp32-home-sensor] repository.

## References

- [SDS011 Control Protocol][control-protocol]


<!-- page links-->

[control-protocol]: https://cdn.sparkfun.com/assets/parts/1/2/2/7/5/Laser_Dust_Sensor_Control_Protocol_V1.3.pdf
[esp32-home-sensor]: https://github.com/etiennetremel/esp32-home-sensor
