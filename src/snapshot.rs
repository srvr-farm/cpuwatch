use crate::cpu::{self, LogicalCpuStat, PhysicalCore};
use crate::power::{self, ConfiguredPowerLimit, PowerConstraint, RaplDomain};
use crate::sensors::SensorReading;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq)]
pub struct ClockSummary {
    pub average_mhz: Option<u64>,
    pub lifetime_min_mhz: Option<u64>,
    pub lifetime_max_mhz: Option<u64>,
    pub current_min_mhz: Option<u64>,
    pub current_max_mhz: Option<u64>,
    pub lifetime_delta_mhz: Option<u64>,
    pub current_delta_mhz: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CoreSnapshot {
    pub core: PhysicalCore,
    pub frequency_mhz: Option<u64>,
    pub utilization_percent: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PowerSnapshot {
    pub domain: String,
    pub watts: Option<f64>,
    pub constraints: Vec<PowerConstraint>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub clocks: ClockSummary,
    pub cores: Vec<CoreSnapshot>,
    pub power: Vec<PowerSnapshot>,
    pub configured_power_limits: Vec<ConfiguredPowerLimit>,
    pub sensors: Vec<SensorReading>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Default)]
pub struct ClockTracker {
    lifetime_min_mhz: Option<u64>,
    lifetime_max_mhz: Option<u64>,
}

impl ClockTracker {
    pub fn summarize(&mut self, frequencies: &[u64]) -> ClockSummary {
        if frequencies.is_empty() {
            return ClockSummary {
                average_mhz: None,
                lifetime_min_mhz: self.lifetime_min_mhz,
                lifetime_max_mhz: self.lifetime_max_mhz,
                current_min_mhz: None,
                current_max_mhz: None,
                lifetime_delta_mhz: lifetime_delta(self.lifetime_min_mhz, self.lifetime_max_mhz),
                current_delta_mhz: None,
            };
        }

        let current_min_mhz = frequencies.iter().min().copied();
        let current_max_mhz = frequencies.iter().max().copied();
        let average_mhz = Some(frequencies.iter().sum::<u64>() / frequencies.len() as u64);

        if let Some(current_min_mhz) = current_min_mhz {
            self.lifetime_min_mhz = Some(
                self.lifetime_min_mhz
                    .map(|previous| previous.min(current_min_mhz))
                    .unwrap_or(current_min_mhz),
            );
        }
        if let Some(current_max_mhz) = current_max_mhz {
            self.lifetime_max_mhz = Some(
                self.lifetime_max_mhz
                    .map(|previous| previous.max(current_max_mhz))
                    .unwrap_or(current_max_mhz),
            );
        }

        ClockSummary {
            average_mhz,
            lifetime_min_mhz: self.lifetime_min_mhz,
            lifetime_max_mhz: self.lifetime_max_mhz,
            current_min_mhz,
            current_max_mhz,
            lifetime_delta_mhz: lifetime_delta(self.lifetime_min_mhz, self.lifetime_max_mhz),
            current_delta_mhz: lifetime_delta(current_min_mhz, current_max_mhz),
        }
    }
}

fn lifetime_delta(min: Option<u64>, max: Option<u64>) -> Option<u64> {
    Some(max?.saturating_sub(min?))
}

#[derive(Debug)]
pub struct Sampler {
    sys_cpu_root: PathBuf,
    proc_stat_path: PathBuf,
    powercap_root: PathBuf,
    plimit_config_path: PathBuf,
    cores: Vec<PhysicalCore>,
    prev_cpu_stats: Option<Vec<LogicalCpuStat>>,
    prev_power_domains: Option<Vec<RaplDomain>>,
    prev_sample_at: Option<Instant>,
    clock_tracker: ClockTracker,
}

impl Default for Sampler {
    fn default() -> Self {
        Self::new_with_plimit_config(
            PathBuf::from("/sys/devices/system/cpu"),
            PathBuf::from("/proc/stat"),
            PathBuf::from(power::DEFAULT_POWERCAP_ROOT),
            PathBuf::from("/etc/intel-plimit.conf"),
        )
    }
}

impl Sampler {
    pub fn new(sys_cpu_root: PathBuf, proc_stat_path: PathBuf, powercap_root: PathBuf) -> Self {
        Self::new_with_plimit_config(
            sys_cpu_root,
            proc_stat_path,
            powercap_root,
            PathBuf::from("/etc/intel-plimit.conf"),
        )
    }

    pub fn new_with_plimit_config(
        sys_cpu_root: PathBuf,
        proc_stat_path: PathBuf,
        powercap_root: PathBuf,
        plimit_config_path: PathBuf,
    ) -> Self {
        let cores = cpu::discover_cores(&sys_cpu_root);
        Self {
            sys_cpu_root,
            proc_stat_path,
            powercap_root,
            plimit_config_path,
            cores,
            prev_cpu_stats: None,
            prev_power_domains: None,
            prev_sample_at: None,
            clock_tracker: ClockTracker::default(),
        }
    }

    pub fn sample(&mut self) -> Snapshot {
        self.sample_at(Instant::now())
    }

    fn sample_at(&mut self, now: Instant) -> Snapshot {
        if self.cores.is_empty() {
            self.cores = cpu::discover_cores(&self.sys_cpu_root);
        }

        let frequencies = cpu::read_frequencies_mhz(&self.sys_cpu_root);
        let cpu_stats = cpu::read_proc_stat(&self.proc_stat_path);
        let utilizations = self
            .prev_cpu_stats
            .as_ref()
            .map(|prev| cpu::aggregate_utilization(prev, &cpu_stats, &self.cores))
            .unwrap_or_default()
            .into_iter()
            .map(|utilization| {
                (
                    (utilization.package_id, utilization.core_id),
                    utilization.utilization_percent,
                )
            })
            .collect::<HashMap<_, _>>();

        let cores = self
            .cores
            .iter()
            .map(|core| CoreSnapshot {
                core: core.clone(),
                frequency_mhz: cpu::average_frequency_for_core(core, &frequencies),
                utilization_percent: utilizations
                    .get(&(core.package_id, core.core_id))
                    .copied()
                    .flatten(),
            })
            .collect::<Vec<_>>();

        let clock_values = cores
            .iter()
            .filter_map(|core| core.frequency_mhz)
            .collect::<Vec<_>>();
        let clocks = self.clock_tracker.summarize(&clock_values);

        let domains = power::discover_domains(&self.powercap_root);
        let power = self.power_snapshots(&domains, now);
        let configured_power_limits = power::read_configured_limits(&self.plimit_config_path);
        let (sensors, sensor_error) = crate::sensors::collect();
        let diagnostics = diagnostics(
            &self.cores,
            &domains,
            &configured_power_limits,
            sensor_error,
        );

        self.prev_cpu_stats = Some(cpu_stats);
        self.prev_power_domains = Some(domains);
        self.prev_sample_at = Some(now);

        Snapshot {
            clocks,
            cores,
            power,
            configured_power_limits,
            sensors,
            diagnostics,
        }
    }

    fn power_snapshots(&self, domains: &[RaplDomain], now: Instant) -> Vec<PowerSnapshot> {
        let previous = self.prev_power_domains.as_deref().unwrap_or_default();
        let elapsed = self
            .prev_sample_at
            .map(|previous_sample_at| now.saturating_duration_since(previous_sample_at));

        domains
            .iter()
            .map(|domain| {
                let watts = elapsed.and_then(|elapsed| {
                    previous
                        .iter()
                        .find(|previous| {
                            previous.path == domain.path || previous.name == domain.name
                        })
                        .and_then(|previous| power::watts_between(previous, domain, elapsed))
                });

                PowerSnapshot {
                    domain: domain.name.clone(),
                    watts,
                    constraints: domain.constraints.clone(),
                }
            })
            .collect()
    }
}

fn diagnostics(
    cores: &[PhysicalCore],
    domains: &[RaplDomain],
    configured_power_limits: &[ConfiguredPowerLimit],
    sensor_error: Option<String>,
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    if cores.is_empty() {
        diagnostics.push("no CPU topology found under /sys/devices/system/cpu".to_string());
    }
    if domains.is_empty() {
        diagnostics.push("no RAPL/powercap domains found".to_string());
    } else if domains.iter().all(|domain| domain.energy_uj.is_none()) {
        diagnostics.push(
            "RAPL energy counters are unreadable; current watts require energy_uj permission"
                .to_string(),
        );
    }
    if configured_power_limits.is_empty()
        && !domains.is_empty()
        && domains.iter().all(|domain| domain.constraints.is_empty())
    {
        diagnostics.push(
            "no RAPL/powercap constraint files found; power limits and durations are unavailable on this system"
                .to_string(),
        );
    }
    if let Some(sensor_error) = sensor_error {
        diagnostics.push(sensor_error);
    }
    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::power::ConfiguredPowerLimit;
    use std::fs;
    use tempfile::TempDir;

    fn write(path: &std::path::Path, value: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, value).unwrap();
    }

    #[test]
    fn default_sampler_searches_generic_powercap_root() {
        let sampler = Sampler::default();

        assert_eq!(
            sampler.powercap_root,
            PathBuf::from("/sys/devices/virtual/powercap")
        );
    }

    #[test]
    fn sample_includes_configured_power_limits() {
        let temp = TempDir::new().unwrap();
        let sys_cpu = temp.path().join("sys/devices/system/cpu");
        let proc_stat = temp.path().join("proc/stat");
        let powercap = temp.path().join("sys/devices/virtual/powercap");
        let config = temp.path().join("etc/intel-plimit.conf");

        fs::create_dir_all(&sys_cpu).unwrap();
        fs::create_dir_all(&powercap).unwrap();
        write(&proc_stat, "");
        write(
            &config,
            r#"
POWER_BACKEND=auto
AMD_PPT_W=142
AMD_TDC_A=95
AMD_EDC_A=140
"#,
        );

        let mut sampler = Sampler::new_with_plimit_config(sys_cpu, proc_stat, powercap, config);

        let snapshot = sampler.sample_at(Instant::now());

        assert_eq!(
            snapshot.configured_power_limits,
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
    fn clock_summary_tracks_current_and_lifetime_ranges() {
        let mut tracker = ClockTracker::default();

        let first = tracker.summarize(&[4400, 3300, 3500]);
        assert_eq!(first.average_mhz, Some(3733));
        assert_eq!(first.current_min_mhz, Some(3300));
        assert_eq!(first.current_max_mhz, Some(4400));
        assert_eq!(first.lifetime_min_mhz, Some(3300));
        assert_eq!(first.lifetime_max_mhz, Some(4400));
        assert_eq!(first.current_delta_mhz, Some(1100));
        assert_eq!(first.lifetime_delta_mhz, Some(1100));

        let second = tracker.summarize(&[3000, 4600]);
        assert_eq!(second.current_min_mhz, Some(3000));
        assert_eq!(second.current_max_mhz, Some(4600));
        assert_eq!(second.lifetime_min_mhz, Some(3000));
        assert_eq!(second.lifetime_max_mhz, Some(4600));
        assert_eq!(second.lifetime_delta_mhz, Some(1600));
    }

    #[test]
    fn empty_clock_summary_is_unavailable() {
        let mut tracker = ClockTracker::default();
        let summary = tracker.summarize(&[]);
        assert_eq!(summary.average_mhz, None);
        assert_eq!(summary.current_delta_mhz, None);
    }

    #[test]
    fn diagnostics_report_when_powercap_constraints_are_unavailable() {
        let cores = vec![PhysicalCore {
            package_id: 0,
            core_id: 0,
            logical_cpus: vec![0, 1],
        }];
        let domains = vec![RaplDomain {
            path: "package".into(),
            name: "package-0".to_string(),
            energy_uj: Some(1_000_000),
            max_energy_range_uj: Some(10_000_000),
            constraints: vec![],
        }];

        let diagnostics = diagnostics(&cores, &domains, &[], None);

        assert_eq!(
            diagnostics,
            vec![
                "no RAPL/powercap constraint files found; power limits and durations are unavailable on this system"
                    .to_string()
            ]
        );
    }
}
