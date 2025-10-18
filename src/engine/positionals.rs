// Utilities for positional data

#[derive(Debug, Default, Clone, Copy)]
pub struct Positionals {
    pub bearing: f64,
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: f64,
}

impl Positionals {
    pub fn new() -> Self {
        // For now, return all zeros
        Self {
            bearing: 0.0,
            latitude: 0.0,
            longitude: 0.0,
            altitude: 0.0,
        }
    }
}

#[cfg(target_os = "macos")]
fn get_latitude() -> f64 {
    use std::process::Command;
    let script = r#"tell application \"System Events\" to get the location of current location"#;
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let s = String::from_utf8_lossy(&out.stdout);
            let lat = s.find("latitude:").and_then(|i| s[i..].split(',').next())
                .and_then(|v| v.split(':').nth(1))
                .and_then(|v| v.trim().parse::<f64>().ok());
            println!("Latitude raw: {:?}", lat);
            lat.unwrap_or(0.0)
        }
        _ => {
            println!("Failed to get latitude, defaulting to 0.0");
            0.0
        }
    }
}

#[cfg(target_os = "macos")]
fn get_longitude() -> f64 {
    use std::process::Command;
    let script = r#"tell application \"System Events\" to get the location of current location"#;
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let s = String::from_utf8_lossy(&out.stdout);
            let lon = s.find("longitude:").and_then(|i| s[i..].split(',').next())
                .and_then(|v| v.split(':').nth(1))
                .and_then(|v| v.trim().parse::<f64>().ok());
            println!("Longitude raw: {:?}", lon);
            lon.unwrap_or(0.0)
        }
        _ => {
            println!("Failed to get longitude, defaulting to 0.0");
            0.0
        }
    }
}

#[cfg(target_os = "macos")]
fn get_altitude() -> f64 {
    use std::process::Command;
    let script = r#"tell application \"System Events\" to get the location of current location"#;
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let s = String::from_utf8_lossy(&out.stdout);
            let alt = s.find("altitude:").and_then(|i| s[i..].split(',').next())
                .and_then(|v| v.split(':').nth(1))
                .and_then(|v| v.trim().parse::<f64>().ok());
            println!("Altitude raw: {:?}", alt);
            alt.unwrap_or(0.0)
        }
        _ => {
            println!("Failed to get altitude, defaulting to 0.0");
            0.0
        }
    }
}

pub fn get_positionals() -> Positionals {
    #[cfg(target_os = "macos")]
    {
        let latitude = get_latitude();
        let longitude = get_longitude();
        let altitude = get_altitude();
        println!("Current positionals: latitude={}, longitude={}, altitude={}", latitude, longitude, altitude);
        Positionals {
            bearing: 0.0, // Not available from AppleScript
            latitude,
            longitude,
            altitude,
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Positionals::new()
    }
}
