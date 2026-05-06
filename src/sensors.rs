use serde_json::Value;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SensorReading {
    pub label: String,
    pub value: String,
}

pub fn parse_sensors_json(input: &str) -> Vec<SensorReading> {
    let Ok(value) = serde_json::from_str::<Value>(input) else {
        return Vec::new();
    };
    let mut readings = Vec::new();

    let Value::Object(devices) = value else {
        return readings;
    };

    for (device, value) in devices {
        collect_json_temperatures(device, &value, &mut readings);
    }

    readings
}

pub fn parse_sensors_text(input: &str) -> Vec<SensorReading> {
    input
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let (label, rest) = line.split_once(':')?;
            let label = label.trim();
            if !is_relevant_temperature_label(label) {
                return None;
            }
            let value = rest.split_whitespace().next()?;
            if !value.contains("°C") {
                return None;
            }
            Some(SensorReading {
                label: label.to_string(),
                value: value.to_string(),
            })
        })
        .collect()
}

pub fn collect() -> (Vec<SensorReading>, Option<String>) {
    match Command::new("sensors").arg("-j").output() {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let readings = parse_sensors_json(&stdout);
            if !readings.is_empty() {
                return (readings, None);
            }
        }
        Ok(_) | Err(_) => {}
    }

    match Command::new("sensors").output() {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            (parse_sensors_text(&stdout), None)
        }
        Ok(output) => (
            Vec::new(),
            Some(format!("sensors exited with status {}", output.status)),
        ),
        Err(error) => (Vec::new(), Some(format!("sensors unavailable: {error}"))),
    }
}

fn collect_json_temperatures(label: String, value: &Value, readings: &mut Vec<SensorReading>) {
    let Value::Object(fields) = value else {
        return;
    };

    if let Some(temp) = fields.iter().find_map(|(key, value)| {
        if key.starts_with("temp") && key.ends_with("_input") {
            value.as_f64()
        } else {
            None
        }
    }) {
        readings.push(SensorReading {
            label: label.clone(),
            value: format_temperature(temp),
        });
    }

    for (key, child) in fields {
        if child.is_object() {
            collect_json_temperatures(format!("{label} {key}"), child, readings);
        }
    }
}

fn format_temperature(value: f64) -> String {
    format!("{value:+.1}°C")
}

fn is_relevant_temperature_label(label: &str) -> bool {
    label.starts_with("Core ")
        || label.starts_with("CPU")
        || label.starts_with("Tctl")
        || label.starts_with("Tdie")
        || label.starts_with("Tccd")
        || label.starts_with("Sensor")
        || label.starts_with("Package")
        || label.starts_with("GPU")
        || label.starts_with("Video")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_temperature_readings_from_sensors_json() {
        let input = r#"{
          "coretemp-isa-0000": {
            "Adapter": "ISA adapter",
            "Package id 0": { "temp1_input": 81.0 },
            "Core 0": { "temp2_input": 70.0 }
          },
          "dell_ddv-virtual-0": {
            "CPU": { "temp1_input": 81.0 },
            "Video": { "temp9_input": 82.0 }
          }
        }"#;

        let readings = parse_sensors_json(input);

        assert!(readings.contains(&SensorReading {
            label: "coretemp-isa-0000 Package id 0".to_string(),
            value: "+81.0°C".to_string(),
        }));
        assert!(readings.contains(&SensorReading {
            label: "coretemp-isa-0000 Core 0".to_string(),
            value: "+70.0°C".to_string(),
        }));
        assert!(readings.contains(&SensorReading {
            label: "dell_ddv-virtual-0 Video".to_string(),
            value: "+82.0°C".to_string(),
        }));
    }

    #[test]
    fn preserves_relevant_plain_text_sensor_lines() {
        let input = "\
Core 0:        +70.0°C  (high = +80.0°C, crit = +100.0°C)
CPU:           +81.0°C
Sensor 1:      +82.0°C
fan1:        3994 RPM
";

        let readings = parse_sensors_text(input);

        assert_eq!(
            readings,
            vec![
                SensorReading {
                    label: "Core 0".to_string(),
                    value: "+70.0°C".to_string(),
                },
                SensorReading {
                    label: "CPU".to_string(),
                    value: "+81.0°C".to_string(),
                },
                SensorReading {
                    label: "Sensor 1".to_string(),
                    value: "+82.0°C".to_string(),
                },
            ]
        );
    }

    #[test]
    fn parses_amd_temperature_labels_from_plain_text_sensors() {
        let input = "\
Tdie:         +36.0°C  (high = +95.0°C)
Tctl:         +36.0°C
Tccd1:        +38.5°C
fan1:        3994 RPM
";

        let readings = parse_sensors_text(input);

        assert_eq!(
            readings,
            vec![
                SensorReading {
                    label: "Tdie".to_string(),
                    value: "+36.0°C".to_string(),
                },
                SensorReading {
                    label: "Tctl".to_string(),
                    value: "+36.0°C".to_string(),
                },
                SensorReading {
                    label: "Tccd1".to_string(),
                    value: "+38.5°C".to_string(),
                },
            ]
        );
    }
}
