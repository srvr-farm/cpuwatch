use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicalCore {
    pub package_id: i32,
    pub core_id: i32,
    pub logical_cpus: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalCpuStat {
    pub cpu_id: usize,
    pub busy: u64,
    pub total: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CoreUtilization {
    pub package_id: i32,
    pub core_id: i32,
    pub logical_cpus: Vec<usize>,
    pub utilization_percent: Option<f64>,
}

pub fn discover_cores(sys_cpu_root: &Path) -> Vec<PhysicalCore> {
    let mut grouped: BTreeMap<(i32, i32), Vec<usize>> = BTreeMap::new();

    let mut cpu_ids = cpu_ids_in(sys_cpu_root);
    cpu_ids.sort_unstable();

    for cpu_id in cpu_ids {
        let topology = sys_cpu_root.join(format!("cpu{cpu_id}/topology"));
        let package_id = read_i32(&topology.join("physical_package_id")).unwrap_or(0);
        let core_id = read_i32(&topology.join("core_id")).unwrap_or(cpu_id as i32);
        grouped
            .entry((package_id, core_id))
            .or_default()
            .push(cpu_id);
    }

    grouped
        .into_iter()
        .map(|((package_id, core_id), mut logical_cpus)| {
            logical_cpus.sort_unstable();
            PhysicalCore {
                package_id,
                core_id,
                logical_cpus,
            }
        })
        .collect()
}

pub fn read_frequencies_mhz(sys_cpu_root: &Path) -> HashMap<usize, u64> {
    let mut frequencies = HashMap::new();
    let cpufreq_root = sys_cpu_root.join("cpufreq");

    if let Ok(entries) = fs::read_dir(&cpufreq_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let Some(mhz) = read_u64(&path.join("scaling_cur_freq")).map(|khz| khz / 1000) else {
                continue;
            };

            let related = fs::read_to_string(path.join("related_cpus"))
                .ok()
                .and_then(|value| parse_cpu_list(value.trim()).ok())
                .unwrap_or_else(|| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .strip_prefix("policy")
                        .and_then(|id| id.parse::<usize>().ok())
                        .map(|id| vec![id])
                        .unwrap_or_default()
                });

            for cpu_id in related {
                frequencies.insert(cpu_id, mhz);
            }
        }
    }

    if frequencies.is_empty() {
        for cpu_id in cpu_ids_in(sys_cpu_root) {
            let path = sys_cpu_root.join(format!("cpu{cpu_id}/cpufreq/scaling_cur_freq"));
            if let Some(mhz) = read_u64(&path).map(|khz| khz / 1000) {
                frequencies.insert(cpu_id, mhz);
            }
        }
    }

    frequencies
}

pub fn parse_proc_stat(input: &str) -> Vec<LogicalCpuStat> {
    input
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let label = parts.next()?;
            let cpu_id = label.strip_prefix("cpu")?.parse::<usize>().ok()?;
            let values = parts
                .filter_map(|part| part.parse::<u64>().ok())
                .collect::<Vec<_>>();
            if values.is_empty() {
                return None;
            }

            let total = values.iter().sum::<u64>();
            let idle = values.get(3).copied().unwrap_or(0);
            let iowait = values.get(4).copied().unwrap_or(0);
            let busy = total.saturating_sub(idle).saturating_sub(iowait);

            Some(LogicalCpuStat {
                cpu_id,
                busy,
                total,
            })
        })
        .collect()
}

pub fn aggregate_utilization(
    prev: &[LogicalCpuStat],
    curr: &[LogicalCpuStat],
    cores: &[PhysicalCore],
) -> Vec<CoreUtilization> {
    let prev_by_cpu = prev
        .iter()
        .map(|stat| (stat.cpu_id, stat))
        .collect::<HashMap<_, _>>();
    let curr_by_cpu = curr
        .iter()
        .map(|stat| (stat.cpu_id, stat))
        .collect::<HashMap<_, _>>();

    cores
        .iter()
        .map(|core| {
            let mut busy_delta = 0_u64;
            let mut total_delta = 0_u64;

            for cpu_id in &core.logical_cpus {
                let Some(prev_stat) = prev_by_cpu.get(cpu_id) else {
                    continue;
                };
                let Some(curr_stat) = curr_by_cpu.get(cpu_id) else {
                    continue;
                };
                busy_delta += curr_stat.busy.saturating_sub(prev_stat.busy);
                total_delta += curr_stat.total.saturating_sub(prev_stat.total);
            }

            let utilization_percent = if total_delta == 0 {
                None
            } else {
                Some((busy_delta as f64 / total_delta as f64) * 100.0)
            };

            CoreUtilization {
                package_id: core.package_id,
                core_id: core.core_id,
                logical_cpus: core.logical_cpus.clone(),
                utilization_percent,
            }
        })
        .collect()
}

pub fn read_proc_stat(path: &Path) -> Vec<LogicalCpuStat> {
    fs::read_to_string(path)
        .map(|input| parse_proc_stat(&input))
        .unwrap_or_default()
}

pub fn average_frequency_for_core(
    core: &PhysicalCore,
    frequencies: &HashMap<usize, u64>,
) -> Option<u64> {
    let values = core
        .logical_cpus
        .iter()
        .filter_map(|cpu_id| frequencies.get(cpu_id).copied())
        .collect::<Vec<_>>();
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<u64>() / values.len() as u64)
    }
}

fn cpu_ids_in(sys_cpu_root: &Path) -> Vec<usize> {
    fs::read_dir(sys_cpu_root)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.flatten())
        .filter_map(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .strip_prefix("cpu")
                .and_then(|id| id.parse::<usize>().ok())
        })
        .collect()
}

fn read_i32(path: &Path) -> Option<i32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn read_u64(path: &Path) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn parse_cpu_list(value: &str) -> Result<Vec<usize>, ()> {
    let mut cpus = Vec::new();

    for part in value.split(',').filter(|part| !part.is_empty()) {
        if let Some((start, end)) = part.split_once('-') {
            let start = start.parse::<usize>().map_err(|_| ())?;
            let end = end.parse::<usize>().map_err(|_| ())?;
            cpus.extend(start..=end);
        } else {
            cpus.push(part.parse::<usize>().map_err(|_| ())?);
        }
    }

    Ok(cpus)
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
    fn groups_logical_cpus_by_package_and_core_id() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        write(&root.join("cpu0/topology/physical_package_id"), "0\n");
        write(&root.join("cpu0/topology/core_id"), "7\n");
        write(&root.join("cpu1/topology/physical_package_id"), "0\n");
        write(&root.join("cpu1/topology/core_id"), "7\n");
        write(&root.join("cpu2/topology/physical_package_id"), "0\n");
        write(&root.join("cpu2/topology/core_id"), "8\n");

        let cores = discover_cores(root);

        assert_eq!(
            cores,
            vec![
                PhysicalCore {
                    package_id: 0,
                    core_id: 7,
                    logical_cpus: vec![0, 1],
                },
                PhysicalCore {
                    package_id: 0,
                    core_id: 8,
                    logical_cpus: vec![2],
                },
            ]
        );
    }

    #[test]
    fn reads_policy_frequencies_as_mhz_for_related_cpus() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        write(&root.join("cpufreq/policy0/related_cpus"), "0-1\n");
        write(&root.join("cpufreq/policy0/scaling_cur_freq"), "4400000\n");
        write(&root.join("cpufreq/policy2/related_cpus"), "2\n");
        write(&root.join("cpufreq/policy2/scaling_cur_freq"), "3300500\n");

        let frequencies = read_frequencies_mhz(root);

        assert_eq!(frequencies.get(&0), Some(&4400));
        assert_eq!(frequencies.get(&1), Some(&4400));
        assert_eq!(frequencies.get(&2), Some(&3300));
    }

    #[test]
    fn parses_proc_stat_busy_and_total_jiffies() {
        let stats = parse_proc_stat(
            "cpu  1 2 3 4 5 6 7 8 9 10\ncpu0 10 1 5 80 5 0 0 0 0 0\ncpu1 4 0 6 90 0 0 0 0 0 0\n",
        );

        assert_eq!(
            stats,
            vec![
                LogicalCpuStat {
                    cpu_id: 0,
                    busy: 16,
                    total: 101,
                },
                LogicalCpuStat {
                    cpu_id: 1,
                    busy: 10,
                    total: 100,
                },
            ]
        );
    }

    #[test]
    fn aggregates_utilization_across_sibling_threads() {
        let cores = vec![PhysicalCore {
            package_id: 0,
            core_id: 7,
            logical_cpus: vec![0, 1],
        }];
        let prev = vec![
            LogicalCpuStat {
                cpu_id: 0,
                busy: 10,
                total: 100,
            },
            LogicalCpuStat {
                cpu_id: 1,
                busy: 20,
                total: 100,
            },
        ];
        let curr = vec![
            LogicalCpuStat {
                cpu_id: 0,
                busy: 40,
                total: 200,
            },
            LogicalCpuStat {
                cpu_id: 1,
                busy: 50,
                total: 200,
            },
        ];

        let utilization = aggregate_utilization(&prev, &curr, &cores);

        assert_eq!(utilization.len(), 1);
        assert_eq!(utilization[0].logical_cpus, vec![0, 1]);
        assert_eq!(utilization[0].utilization_percent, Some(30.0));
    }
}
