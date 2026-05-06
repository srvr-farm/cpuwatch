# cpuwatch - Linux CPU Monitor for the Terminal

`cpuwatch` is a read-only Linux CPU monitor and terminal TUI for CPU clock
speed, per-core utilization, RAPL package power, configured power limits, and
temperatures. It is useful when you want a lightweight Rust alternative or
companion to tools like `watch`, `sensors`, `htop`, `btop`, and `turbostat`.

The default mode is an interactive terminal UI. A `--once` mode is also
available for scripts, diagnostics, and non-interactive environments.

## What You Can Monitor

Use `cpuwatch` when you want to:

- Monitor CPU frequency and clock speed on Linux from a terminal.
- Watch per-physical-core CPU utilization instead of only per-thread CPU usage.
- Inspect Intel RAPL or AMD RAPL package power draw in watts.
- See Linux powercap limits, power level durations, and configured AMD
  PPT/TDC/EDC values.
- View CPU temperature readings from `lm-sensors` alongside clocks and power.
- Debug laptop, desktop, workstation, homelab, and server CPU performance
  without changing governors, power limits, or fan settings.

## Features

- Groups logical CPUs into physical cores using Linux CPU topology.
- Shows current average, minimum, maximum, and delta CPU clocks.
- Computes physical-core utilization from `/proc/stat` deltas.
- Reads Linux powercap/RAPL domains and reports current watts when energy
  counters are readable.
- Displays RAPL power constraints such as long-term, short-term, and peak power
  limits when the kernel exposes them.
- Displays configured AMD power-limit values from `/etc/intel-plimit.conf` when
  present.
- Collects CPU and GPU-like temperature readings from `sensors`.
- Degrades to `N/A` values and diagnostics when optional data sources are
  missing or unreadable.

## Keywords

Linux CPU monitor, terminal CPU monitor, Rust TUI CPU monitor, CPU frequency
monitor, CPU clock speed monitor, per-core CPU usage, physical core utilization,
Linux RAPL monitor, Intel RAPL, AMD RAPL, Linux powercap, CPU power monitor,
CPU temperature monitor, `lm-sensors`, `cpufreq`, `/proc/stat`, `/sys` hardware
monitor.

## Quick Start

Run from the repository without installing:

```sh
cargo run -- --once
cargo run -- --interval 500ms
```

Build and install the release binary:

```sh
make install
cpuwatch --once
cpuwatch
```

If `/usr/local/bin` is not in your `PATH`, either add it or install with a custom
`PREFIX`, `BINDIR`, or `INSTALL_PATH`.

## Supported Systems

`cpuwatch` targets Linux systems that expose CPU data through procfs and sysfs.

| System type | Support level | Notes |
| --- | --- | --- |
| Linux on Intel x86/x86_64 with powercap/RAPL | Full | Expected to show topology, utilization, clocks, RAPL power, RAPL constraints, and temperatures when permissions and sensors are set up. |
| Linux on AMD x86/x86_64 with powercap/RAPL | Full or partial | Recursive powercap discovery supports AMD RAPL layouts. Configured AMD PPT/TDC/EDC values can also be read from `/etc/intel-plimit.conf`. |
| Linux systems without powercap/RAPL | Partial | CPU topology, clocks, utilization, and temperatures may work. Power sections show `N/A` or diagnostics. |
| Linux VMs, containers, or restricted hosts | Partial | Virtualized or containerized environments often hide cpufreq, topology, sensors, or RAPL files. |
| Non-x86 Linux | Partial | CPU topology/utilization may work if standard procfs/sysfs files exist. RAPL power data is usually unavailable. |
| macOS, Windows, BSD, WSL without Linux hardware sysfs | Not supported for useful runtime data | The crate may compile on some non-Linux targets, but the monitor expects Linux `/proc` and `/sys` hardware interfaces. |

The TUI requires an interactive terminal. Use `--once` for CI, logs, scripts, or
other non-interactive use.

## Data Sources

| Data | Source |
| --- | --- |
| CPU topology | `/sys/devices/system/cpu/cpu*/topology` |
| CPU frequency | `/sys/devices/system/cpu/cpufreq/policy*/scaling_cur_freq`, with a fallback to per-CPU cpufreq paths |
| CPU utilization | `/proc/stat` |
| Power domains and energy counters | `/sys/devices/virtual/powercap` |
| RAPL constraints | `constraint_*` files under powercap domains |
| Configured AMD limits | `/etc/intel-plimit.conf` keys `AMD_PPT_W`, `AMD_TDC_A`, and `AMD_EDC_A` |
| Temperatures | `sensors -j`, falling back to plain `sensors` output |

`cpuwatch` does not write power limits, CPU governors, fan settings, or any other
hardware controls.

## Prerequisites

Required for building:

- Rust 1.75 or newer.
- Cargo.

Optional but recommended:

- `make`, for the repository build/install targets.
- `sudo`, `setcap`, and `getcap`, for installing the binary with the file
  capability used to read protected kernel files.
- `lm-sensors`, for the `sensors` command and temperature readings.

On Debian or Ubuntu-style systems, the optional runtime tools are typically in:

```sh
sudo apt install make libcap2-bin lm-sensors
```

On Fedora-style systems:

```sh
sudo dnf install make libcap lm_sensors
```

Distribution package names vary. If `setcap`, `getcap`, or `sensors` are not in
`PATH`, install the package that provides them for your distribution.

## Building

Build a debug binary:

```sh
cargo build
```

Build an optimized release binary:

```sh
cargo build --release
```

The release binary is written to:

```sh
target/release/cpuwatch
```

The Makefile wraps the release build:

```sh
make build
```

## Development Checks

Run the full local check suite:

```sh
make check
```

That runs:

```sh
cargo fmt --check
cargo test
cargo clippy -- -D warnings
```

You can also run individual targets:

```sh
make fmt
make test
make clippy
```

## Installing

The recommended install path is through the Makefile:

```sh
make install
```

By default this:

1. Builds `target/release/cpuwatch` if needed.
2. Installs it to `/usr/local/bin/cpuwatch`.
3. Applies the `cap_dac_read_search+ep` file capability.
4. Prints the resulting capability with `getcap`.

Verify the installed command:

```sh
command -v cpuwatch
cpuwatch --once
```

If you prefer to run the privileged install step explicitly, build first and
then run install under `sudo`:

```sh
make build
sudo make install
```

The prebuild matters because `sudo make install` runs as root and the Makefile
expects the release binary to already exist in that case.

### Custom Install Paths

Install under a different prefix:

```sh
PREFIX="$HOME/.local" make install
```

Install to a specific binary directory:

```sh
BINDIR="$HOME/.local/bin" make install
```

Install to an exact path:

```sh
INSTALL_PATH="$HOME/.local/bin/cpuwatch" make install
```

### Installing Without Capabilities

To install only the binary:

```sh
make install-binary
```

Without the capability, `cpuwatch` still runs, but protected powercap files may
be unreadable and current watts may show as `N/A`.

You can apply or reapply the capability later:

```sh
make capability
```

Check the installed capability:

```sh
make show-capability
getcap "$(command -v cpuwatch)"
```

Remove the installed binary:

```sh
make uninstall
```

### Cargo Install

You can also install with Cargo:

```sh
cargo install --path .
```

Cargo does not apply Linux file capabilities. If you need protected RAPL energy
counters, apply the capability to the installed binary manually or use
`make install`.

## Runtime Setup

### Powercap and RAPL

Power readings require Linux powercap/RAPL files under:

```sh
/sys/devices/virtual/powercap
```

Check whether your system exposes domains:

```sh
ls /sys/devices/virtual/powercap
```

Check for energy counters:

```sh
find /sys/devices/virtual/powercap -name energy_uj -print
```

If domains exist but watts are `N/A`, the energy counters are probably
unreadable by your user. Install with `make install`, run the installed binary,
or run a one-off check with elevated privileges:

```sh
sudo target/release/cpuwatch --once
```

The Makefile uses this default capability:

```sh
cap_dac_read_search+ep
```

Some filesystems or security policies may not preserve file capabilities. If a
binary is copied or replaced after install, run `make capability` again.

### Temperatures

Temperature readings use the `sensors` command from `lm-sensors`.

Verify JSON output:

```sh
sensors -j
```

If `sensors` is missing or returns no matching temperature labels, the
temperature panel will be empty or a diagnostic will be shown. On many Linux
systems, sensor detection and kernel driver setup are distribution-specific; use
your distro's `lm-sensors` setup documentation when readings are missing.

### Configured AMD Limits

If `/etc/intel-plimit.conf` exists, `cpuwatch` reads these optional keys:

```sh
AMD_PPT_W=142
AMD_TDC_A=95
AMD_EDC_A=140
```

Readable non-empty values appear in the power panel as configured limits.
Missing, unreadable, empty, or unparsable values are ignored.

## Usage

Start the interactive TUI:

```sh
cpuwatch
```

Exit the TUI with any of:

- `q`
- `Esc`
- `Ctrl-C`

Use a custom update interval:

```sh
cpuwatch --interval 500ms
cpuwatch --interval 2s
```

Print one text report and exit:

```sh
cpuwatch --once
```

Use a custom sampling interval for the one-shot report:

```sh
cpuwatch --once --interval 250ms
```

In `--once` mode, `cpuwatch` takes an initial sample, waits for the interval, then
takes a second sample so utilization and watts can be computed from deltas.

Show CLI help:

```sh
cpuwatch --help
```

Current options:

```text
Usage: cpuwatch [OPTIONS]

Options:
      --interval <INTERVAL>  [default: 1s]
      --once
  -h, --help                 Print help
```

## Troubleshooting

- `no CPU topology found under /sys/devices/system/cpu`: the process cannot see
  the Linux CPU sysfs tree. This commonly happens on unsupported OSes,
  restricted containers, or unusual minimal environments.
- `no RAPL/powercap domains found`: the kernel did not expose powercap domains
  to this environment. Power readings are unavailable, but CPU
  clocks/utilization and temperatures may still work.
- `RAPL energy counters are unreadable`: power domains were found, but
  `energy_uj` files could not be read. Install with `make install`, reapply
  `make capability`, or run as a user with permission to read those files.
- No temperatures appear: install and configure `lm-sensors`, then verify that
  `sensors -j` returns CPU or GPU-like temperature readings.
- CPU clocks show `N/A`: the system may not expose cpufreq files, or the process
  may be running in a VM/container where host CPU frequency is hidden.
- The TUI does not render correctly: use a real interactive terminal with enough
  width and height, or use `cpuwatch --once` for plain text output.

## Repository Layout

- `src/main.rs`: binary entry point.
- `src/lib.rs`: mode selection, TUI loop, and terminal lifecycle.
- `src/cli.rs`: command-line options.
- `src/cpu.rs`: topology, frequency, and utilization readers.
- `src/power.rs`: powercap/RAPL discovery, constraints, and watt calculations.
- `src/sensors.rs`: `sensors` collection and parsing.
- `src/snapshot.rs`: combined sampling state.
- `src/render.rs`: TUI rendering and one-shot text reports.
- `Makefile`: build, install, capability, and check targets.

## Suggested GitHub Topics

For the repository About sidebar, useful topics would be:

```text
linux
rust
tui
terminal
cpu-monitor
hardware-monitor
performance-monitoring
rapl
powercap
lm-sensors
cpufreq
ratatui
```
