use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const DEFAULT_POWERCAP_ROOT: &str = "/sys/devices/virtual/powercap";

#[derive(Debug, Clone, PartialEq)]
pub struct ConfiguredPowerLimit {
    pub name: String,
    pub value: f64,
    pub unit: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PowerConstraint {
    pub index: usize,
    pub name: String,
    pub power_limit_watts: Option<f64>,
    pub max_power_watts: Option<f64>,
    pub time_window_secs: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RaplDomain {
    pub path: PathBuf,
    pub name: String,
    pub energy_uj: Option<u64>,
    pub max_energy_range_uj: Option<u64>,
    pub constraints: Vec<PowerConstraint>,
}

pub fn discover_domains(powercap_root: &Path) -> Vec<RaplDomain> {
    let mut domains = Vec::new();
    discover_domains_recursive(powercap_root, &mut domains);
    domains.sort_by(|left, right| left.path.cmp(&right.path));
    domains
}

pub fn read_configured_limits(config_path: &Path) -> Vec<ConfiguredPowerLimit> {
    let Ok(config) = fs::read_to_string(config_path) else {
        return Vec::new();
    };

    let values = config
        .lines()
        .filter_map(parse_config_assignment)
        .collect::<std::collections::HashMap<_, _>>();

    [
        ("AMD_PPT_W", "AMD PPT", "W"),
        ("AMD_TDC_A", "AMD TDC", "A"),
        ("AMD_EDC_A", "AMD EDC", "A"),
    ]
    .into_iter()
    .filter_map(|(key, name, unit)| {
        let value = values.get(key)?;
        let value = value.parse::<f64>().ok()?;
        Some(ConfiguredPowerLimit {
            name: name.to_string(),
            value,
            unit: unit.to_string(),
        })
    })
    .collect()
}

pub fn watts_between(prev: &RaplDomain, curr: &RaplDomain, elapsed: Duration) -> Option<f64> {
    let elapsed_secs = elapsed.as_secs_f64();
    if elapsed_secs <= 0.0 {
        return None;
    }

    let prev_energy_uj = prev.energy_uj?;
    let curr_energy_uj = curr.energy_uj?;

    let delta_uj = if curr_energy_uj >= prev_energy_uj {
        curr_energy_uj - prev_energy_uj
    } else {
        curr.max_energy_range_uj
            .or(prev.max_energy_range_uj)?
            .saturating_sub(prev_energy_uj)
            + curr_energy_uj
    };

    Some(delta_uj as f64 / 1_000_000.0 / elapsed_secs)
}

fn discover_domains_recursive(path: &Path, domains: &mut Vec<RaplDomain>) {
    if let Some(domain) = is_rapl_domain(path).then(|| read_domain(path)).flatten() {
        domains.push(domain);
    }

    let Ok(entries) = fs::read_dir(path) else {
        return;
    };

    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
            discover_domains_recursive(&entry.path(), domains);
        }
    }
}

fn read_domain(path: &Path) -> Option<RaplDomain> {
    Some(RaplDomain {
        path: path.to_path_buf(),
        name: read_trimmed(&path.join("name")).unwrap_or_else(|| {
            path.file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        }),
        energy_uj: read_u64(&path.join("energy_uj")),
        max_energy_range_uj: read_u64(&path.join("max_energy_range_uj")),
        constraints: read_constraints(path),
    })
}

fn is_rapl_domain(path: &Path) -> bool {
    path.join("name").is_file()
        || path.join("energy_uj").is_file()
        || path.join("max_energy_range_uj").is_file()
}

fn read_constraints(path: &Path) -> Vec<PowerConstraint> {
    let mut indexes = fs::read_dir(path)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.flatten())
        .filter_map(|entry| {
            let file_name = entry.file_name().to_string_lossy().to_string();
            let index = file_name
                .strip_prefix("constraint_")?
                .strip_suffix("_name")?
                .parse::<usize>()
                .ok()?;
            Some(index)
        })
        .collect::<Vec<_>>();
    indexes.sort_unstable();

    indexes
        .into_iter()
        .filter_map(|index| {
            let name = read_trimmed(&path.join(format!("constraint_{index}_name")))?;
            Some(PowerConstraint {
                index,
                name,
                power_limit_watts: read_microunits_as_units(
                    &path.join(format!("constraint_{index}_power_limit_uw")),
                ),
                max_power_watts: read_microunits_as_units(
                    &path.join(format!("constraint_{index}_max_power_uw")),
                ),
                time_window_secs: read_microunits_as_units(
                    &path.join(format!("constraint_{index}_time_window_us")),
                ),
            })
        })
        .collect()
}

fn read_trimmed(path: &Path) -> Option<String> {
    let value = fs::read_to_string(path).ok()?.trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn read_u64(path: &Path) -> Option<u64> {
    read_trimmed(path)?.parse().ok()
}

fn read_microunits_as_units(path: &Path) -> Option<f64> {
    read_u64(path).map(|value| value as f64 / 1_000_000.0)
}

fn parse_config_assignment(line: &str) -> Option<(String, String)> {
    let line = line
        .split_once('#')
        .map_or(line, |(before, _)| before)
        .trim();
    if line.is_empty() {
        return None;
    }

    let (key, value) = line.split_once('=')?;
    let key = key.trim();
    let value = value.trim().trim_matches('"').trim_matches('\'');
    if key.is_empty() || value.is_empty() {
        return None;
    }

    Some((key.to_string(), value.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write(path: &Path, value: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, value).unwrap();
    }

    #[test]
    fn parses_amd_configured_limits_from_intel_plimit_config() {
        let temp = TempDir::new().unwrap();
        let config = temp.path().join("intel-plimit.conf");
        write(
            &config,
            r#"
POWER_BACKEND=auto
AMD_PPT_W=142
AMD_TDC_A=95
AMD_EDC_A=140
"#,
        );

        let limits = read_configured_limits(&config);

        assert_eq!(
            limits,
            vec![
                ConfiguredPowerLimit {
                    name: "AMD PPT".to_string(),
                    value: 142.0,
                    unit: "W".to_string(),
                },
                ConfiguredPowerLimit {
                    name: "AMD TDC".to_string(),
                    value: 95.0,
                    unit: "A".to_string(),
                },
                ConfiguredPowerLimit {
                    name: "AMD EDC".to_string(),
                    value: 140.0,
                    unit: "A".to_string(),
                },
            ]
        );
    }

    #[test]
    fn skips_empty_configured_limits() {
        let temp = TempDir::new().unwrap();
        let config = temp.path().join("intel-plimit.conf");
        write(
            &config,
            r#"
POWER_BACKEND=auto
AMD_PPT_W=
AMD_TDC_A=95
AMD_EDC_A=
"#,
        );

        let limits = read_configured_limits(&config);

        assert_eq!(
            limits,
            vec![ConfiguredPowerLimit {
                name: "AMD TDC".to_string(),
                value: 95.0,
                unit: "A".to_string(),
            }]
        );
    }

    #[test]
    fn parses_rapl_constraints_as_watts_and_seconds() {
        let temp = TempDir::new().unwrap();
        let domain = temp.path().join("intel-rapl:0");
        write(&domain.join("name"), "package-0\n");
        write(&domain.join("energy_uj"), "123456\n");
        write(&domain.join("max_energy_range_uj"), "1000000\n");
        write(&domain.join("constraint_0_name"), "long_term\n");
        write(&domain.join("constraint_0_power_limit_uw"), "65000000\n");
        write(&domain.join("constraint_0_max_power_uw"), "125000000\n");
        write(&domain.join("constraint_0_time_window_us"), "1000000\n");

        let domains = discover_domains(temp.path());

        assert_eq!(domains.len(), 1);
        assert_eq!(domains[0].name, "package-0");
        assert_eq!(domains[0].energy_uj, Some(123456));
        assert_eq!(
            domains[0].constraints,
            vec![PowerConstraint {
                index: 0,
                name: "long_term".to_string(),
                power_limit_watts: Some(65.0),
                max_power_watts: Some(125.0),
                time_window_secs: Some(1.0),
            }]
        );
    }

    #[test]
    fn discovers_amd_powercap_domains_under_generic_root() {
        let temp = TempDir::new().unwrap();
        let domain = temp.path().join("amd-rapl/amd-rapl:0");
        write(&domain.join("name"), "package-0\n");
        write(&domain.join("energy_uj"), "123456\n");
        write(&domain.join("max_energy_range_uj"), "1000000\n");

        let domains = discover_domains(temp.path());

        assert_eq!(domains.len(), 1);
        assert_eq!(domains[0].path, domain);
        assert_eq!(domains[0].name, "package-0");
        assert_eq!(domains[0].energy_uj, Some(123456));
    }

    #[test]
    fn calculates_watts_from_energy_delta() {
        let prev = RaplDomain {
            path: "package".into(),
            name: "package-0".to_string(),
            energy_uj: Some(1_000_000),
            max_energy_range_uj: Some(10_000_000),
            constraints: vec![],
        };
        let curr = RaplDomain {
            energy_uj: Some(4_000_000),
            ..prev.clone()
        };

        assert_eq!(
            watts_between(&prev, &curr, Duration::from_secs(2)),
            Some(1.5)
        );
    }

    #[test]
    fn calculates_watts_across_energy_wraparound() {
        let prev = RaplDomain {
            path: "package".into(),
            name: "package-0".to_string(),
            energy_uj: Some(9_000_000),
            max_energy_range_uj: Some(10_000_000),
            constraints: vec![],
        };
        let curr = RaplDomain {
            energy_uj: Some(1_000_000),
            ..prev.clone()
        };

        assert_eq!(
            watts_between(&prev, &curr, Duration::from_secs(1)),
            Some(2.0)
        );
    }

    #[cfg(unix)]
    #[test]
    fn skips_symlinked_directories_when_discovering_domains() {
        let temp = TempDir::new().unwrap();
        let domain = temp.path().join("intel-rapl:0");
        write(&domain.join("name"), "package-0\n");
        write(&domain.join("energy_uj"), "123456\n");
        std::os::unix::fs::symlink(&domain, temp.path().join("subsystem")).unwrap();

        let domains = discover_domains(temp.path());

        assert_eq!(domains.len(), 1);
        assert_eq!(domains[0].path, domain);
    }

    #[test]
    fn keeps_domain_constraints_when_energy_is_unavailable() {
        let temp = TempDir::new().unwrap();
        let domain = temp.path().join("intel-rapl:0");
        write(&domain.join("name"), "package-0\n");
        write(&domain.join("constraint_0_name"), "long_term\n");
        write(&domain.join("constraint_0_power_limit_uw"), "65000000\n");
        write(&domain.join("constraint_0_time_window_us"), "1000000\n");

        let domains = discover_domains(temp.path());

        assert_eq!(domains.len(), 1);
        assert_eq!(domains[0].energy_uj, None);
        assert_eq!(domains[0].constraints[0].name, "long_term");
    }
}
