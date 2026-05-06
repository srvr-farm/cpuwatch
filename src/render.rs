use crate::snapshot::Snapshot;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;
use std::fmt::Write;

pub fn format_text_report(snapshot: &Snapshot) -> String {
    let mut report = String::new();

    writeln!(report, "clocks:").unwrap();
    writeln!(report, "  avg: {}", format_mhz(snapshot.clocks.average_mhz)).unwrap();
    writeln!(
        report,
        "  min_max: {} - {}",
        format_mhz(snapshot.clocks.lifetime_min_mhz),
        format_mhz(snapshot.clocks.lifetime_max_mhz)
    )
    .unwrap();
    writeln!(
        report,
        "  current: {} - {}",
        format_mhz(snapshot.clocks.current_min_mhz),
        format_mhz(snapshot.clocks.current_max_mhz)
    )
    .unwrap();
    writeln!(
        report,
        "  delta: {} lifetime, {} current",
        format_mhz(snapshot.clocks.lifetime_delta_mhz),
        format_mhz(snapshot.clocks.current_delta_mhz)
    )
    .unwrap();

    writeln!(report, "utilization:").unwrap();
    for core in &snapshot.cores {
        writeln!(
            report,
            "  core_{} cpu{}: {} at {}",
            core.core.core_id,
            format_cpu_list(&core.core.logical_cpus),
            format_percent(core.utilization_percent),
            format_mhz(core.frequency_mhz)
        )
        .unwrap();
    }

    writeln!(report, "power:").unwrap();
    for domain in &snapshot.power {
        writeln!(
            report,
            "  {}: {}",
            domain.domain,
            format_watts(domain.watts)
        )
        .unwrap();
    }

    writeln!(report, "power_levels:").unwrap();
    for domain in &snapshot.power {
        for constraint in &domain.constraints {
            writeln!(
                report,
                "  {} {}: limit={}, max={}, duration={}",
                domain.domain,
                constraint.name,
                format_watts(constraint.power_limit_watts),
                format_watts(constraint.max_power_watts),
                format_seconds(constraint.time_window_secs)
            )
            .unwrap();
        }
    }

    if !snapshot.configured_power_limits.is_empty() {
        writeln!(report, "configured_limits:").unwrap();
        for limit in &snapshot.configured_power_limits {
            writeln!(
                report,
                "  {}: {}",
                limit.name,
                format_limit_value(limit.value, &limit.unit)
            )
            .unwrap();
        }
    }

    writeln!(report, "temps:").unwrap();
    for sensor in &snapshot.sensors {
        writeln!(report, "  {}: {}", sensor.label, sensor.value).unwrap();
    }

    if !snapshot.diagnostics.is_empty() {
        writeln!(report, "diagnostics:").unwrap();
        for diagnostic in &snapshot.diagnostics {
            writeln!(report, "  {diagnostic}").unwrap();
        }
    }

    report
}

pub fn draw(frame: &mut Frame<'_>, snapshot: &Snapshot) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(if snapshot.diagnostics.is_empty() {
                0
            } else {
                4
            }),
        ])
        .split(frame.area());

    let title = Paragraph::new("cpuwatch  q/Esc/Ctrl-C to quit")
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(title, root[0]);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(root[1]);

    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(6)])
        .split(columns[0]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(10), Constraint::Min(6)])
        .split(columns[1]);

    frame.render_widget(
        Paragraph::new(clock_text(snapshot))
            .block(Block::default().title("Clocks").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        left[0],
    );
    frame.render_widget(
        Paragraph::new(utilization_text(snapshot))
            .block(Block::default().title("Utilization").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        left[1],
    );
    frame.render_widget(
        Paragraph::new(power_text(snapshot))
            .block(Block::default().title("Power").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        right[0],
    );
    frame.render_widget(
        Paragraph::new(temperature_text(snapshot))
            .block(Block::default().title("Temps").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        right[1],
    );

    if !snapshot.diagnostics.is_empty() {
        frame.render_widget(
            Paragraph::new(snapshot.diagnostics.join("\n"))
                .style(Style::default().fg(Color::Yellow))
                .block(Block::default().title("Diagnostics").borders(Borders::ALL))
                .wrap(Wrap { trim: false }),
            root[2],
        );
    }
}

fn format_mhz(value: Option<u64>) -> String {
    value
        .map(|value| format!("{value} MHz"))
        .unwrap_or_else(|| "N/A".to_string())
}

fn format_percent(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.1}%"))
        .unwrap_or_else(|| "N/A".to_string())
}

fn format_watts(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.1} W"))
        .unwrap_or_else(|| "N/A".to_string())
}

fn format_seconds(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.3} s"))
        .unwrap_or_else(|| "N/A".to_string())
}

fn format_limit_value(value: f64, unit: &str) -> String {
    format!("{value:.1} {unit}")
}

fn format_cpu_list(values: &[usize]) -> String {
    values
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn clock_text(snapshot: &Snapshot) -> String {
    let mut text = String::new();
    writeln!(text, "avg: {}", format_mhz(snapshot.clocks.average_mhz)).unwrap();
    writeln!(
        text,
        "min/max: {} - {}",
        format_mhz(snapshot.clocks.lifetime_min_mhz),
        format_mhz(snapshot.clocks.lifetime_max_mhz)
    )
    .unwrap();
    writeln!(
        text,
        "current: {} - {}",
        format_mhz(snapshot.clocks.current_min_mhz),
        format_mhz(snapshot.clocks.current_max_mhz)
    )
    .unwrap();
    writeln!(
        text,
        "delta: {} lifetime, {} current",
        format_mhz(snapshot.clocks.lifetime_delta_mhz),
        format_mhz(snapshot.clocks.current_delta_mhz)
    )
    .unwrap();
    text
}

fn utilization_text(snapshot: &Snapshot) -> String {
    let mut text = String::new();
    for core in &snapshot.cores {
        writeln!(
            text,
            "core_{:<2} cpu{:<5} {:>6}  {:>9}",
            core.core.core_id,
            format_cpu_list(&core.core.logical_cpus),
            format_percent(core.utilization_percent),
            format_mhz(core.frequency_mhz)
        )
        .unwrap();
    }
    text
}

fn power_text(snapshot: &Snapshot) -> String {
    let mut text = String::new();
    for domain in &snapshot.power {
        writeln!(text, "{:<14} {}", domain.domain, format_watts(domain.watts)).unwrap();
        for constraint in &domain.constraints {
            writeln!(
                text,
                "  {:<10} limit={} duration={}",
                constraint.name,
                format_watts(constraint.power_limit_watts),
                format_seconds(constraint.time_window_secs)
            )
            .unwrap();
        }
    }
    if !snapshot.configured_power_limits.is_empty() {
        if !text.is_empty() {
            writeln!(text).unwrap();
        }
        writeln!(text, "configured limits").unwrap();
        for limit in &snapshot.configured_power_limits {
            writeln!(
                text,
                "  {:<10} {}",
                limit.name,
                format_limit_value(limit.value, &limit.unit)
            )
            .unwrap();
        }
    }
    text
}

fn temperature_text(snapshot: &Snapshot) -> String {
    let mut text = String::new();
    for sensor in &snapshot.sensors {
        writeln!(text, "{:<34} {}", sensor.label, sensor.value).unwrap();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::PhysicalCore;
    use crate::power::ConfiguredPowerLimit;
    use crate::sensors::SensorReading;
    use crate::snapshot::{ClockSummary, CoreSnapshot, PowerSnapshot};

    #[test]
    fn text_report_contains_all_major_sections() {
        let snapshot = Snapshot {
            clocks: ClockSummary {
                average_mhz: Some(4000),
                lifetime_min_mhz: Some(3000),
                lifetime_max_mhz: Some(4500),
                current_min_mhz: Some(3500),
                current_max_mhz: Some(4500),
                lifetime_delta_mhz: Some(1500),
                current_delta_mhz: Some(1000),
            },
            cores: vec![CoreSnapshot {
                core: PhysicalCore {
                    package_id: 0,
                    core_id: 7,
                    logical_cpus: vec![0, 1],
                },
                frequency_mhz: Some(4400),
                utilization_percent: Some(30.0),
            }],
            power: vec![PowerSnapshot {
                domain: "package-0".to_string(),
                watts: Some(42.5),
                constraints: vec![],
            }],
            configured_power_limits: vec![],
            sensors: vec![SensorReading {
                label: "Core 0".to_string(),
                value: "+70.0°C".to_string(),
            }],
            diagnostics: vec![],
        };

        let report = format_text_report(&snapshot);

        assert!(report.contains("clocks:"));
        assert!(report.contains("utilization:"));
        assert!(report.contains("power:"));
        assert!(report.contains("power_levels:"));
        assert!(report.contains("temps:"));
        assert!(report.contains("package-0"));
    }

    #[test]
    fn text_report_contains_configured_power_limits() {
        let snapshot = Snapshot {
            clocks: ClockSummary {
                average_mhz: None,
                lifetime_min_mhz: None,
                lifetime_max_mhz: None,
                current_min_mhz: None,
                current_max_mhz: None,
                lifetime_delta_mhz: None,
                current_delta_mhz: None,
            },
            cores: vec![],
            power: vec![],
            configured_power_limits: vec![
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
            ],
            sensors: vec![],
            diagnostics: vec![],
        };

        let report = format_text_report(&snapshot);

        assert!(report.contains("configured_limits:"));
        assert!(report.contains("AMD PPT: 142.0 W"));
        assert!(report.contains("AMD TDC: 95.0 A"));
    }
}
