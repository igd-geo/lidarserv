use log::{info, warn};
use std::thread::sleep;
use std::time::Duration;
use std::error::Error;

fn processor_cooldown_generic() {
    info!("Sleeping for 1 minute to avoid thermal throttling.");
    sleep(Duration::from_secs(60));
}

#[cfg(not(target_os = "linux"))]
pub fn processor_cooldown() {
    // do nothing  on non-linux platforms
    processor_cooldown_generic();
}

#[cfg(target_os = "linux")]
pub fn get_fans_rpm() -> Result<Vec<(String, u32)>, Box<dyn Error>> {
    let mut all_fans = Vec::new();
    use libmedium::{
        parse_hwmons,
        sensors::{Input, Sensor},
    };
    let hwmons = parse_hwmons()?;
    for (_, _, hwmon) in &hwmons {
        for fan in hwmon.fans().values() {
            let rpm = fan.read_input()?.as_rpm();
            let name = fan.name();
            all_fans.push((name, rpm));
        }
    }
    Ok(all_fans)
}

#[cfg(target_os = "linux")]
pub fn processor_cooldown() {
    use log::{info, warn};
    use std::error::Error;
    info!("Taking a break to avoid thermal throttling...");
    loop {
        let fans = match get_fans_rpm() {
            Ok(r) => r,
            Err(e) => {
                warn!(
                "Could not initialize lm_sensors. Will use naive cooldown instead. The error was: {}",
                e
            );
                return processor_cooldown_generic();
            }
        };
        let (name, rpm) =
            if let Some(processor_fan) = fans.iter().find(|it| it.0.contains("Processor")) {
                processor_fan.clone()
            } else if !fans.is_empty() {
                fans[0].clone()
            } else {
                warn!("No fans were found. Will use naive cooldown instead.");
                return processor_cooldown_generic();
            };
        let wait = rpm > 0;
        if wait {
            info!("Fan '{}' is at {} rpm.", name, rpm);
            sleep(Duration::from_secs(5));
        } else {
            info!(
                "Fan '{}' is at {} rpm. Will wait for two additional minutes before continuing with program execution.",
                name, rpm
            );
            sleep(Duration::from_secs(120));
            return;
        }
    }
}
