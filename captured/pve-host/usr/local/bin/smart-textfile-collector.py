#!/usr/bin/env python3
"""Export SMART attributes as Prometheus metrics via node_exporter's textfile collector.

Runs smartctl per device and writes one .prom file. Chosen over the standalone
smartctl_exporter because Debian packages smartmontools but not that exporter,
and an unmanaged binary is a maintenance liability (Kenny, S1, 2026-08-29).

Written atomically: node_exporter reads the directory continuously, so a
half-written file would surface as malformed metrics rather than as an error.
"""
import json
import os
import subprocess
import sys
import time

OUT_DIR = "/var/lib/prometheus/node-exporter"
OUT_FILE = os.path.join(OUT_DIR, "smart.prom")

HEADERS = [
    ("smart_device_health_ok", "gauge", "1 if the device passes its own SMART self-assessment."),
    ("smart_device_temperature_celsius", "gauge", "Current drive temperature."),
    ("smart_device_power_on_hours", "counter", "Total hours the device has been powered on."),
    ("smart_device_reallocated_sectors", "gauge", "Sectors remapped after a read/write failure."),
    ("smart_device_pending_sectors", "gauge", "Sectors awaiting remap; the earliest failure signal."),
    ("smart_collector_last_run_seconds", "gauge", "Unix time of the last completed collector run."),
]

# ATA SMART attribute ids. NVMe reports named fields instead of this table.
ATTR_REALLOCATED = 5
ATTR_PENDING = 197


def scan():
    out = subprocess.run(["smartctl", "--scan", "-j"],
                         capture_output=True, text=True, timeout=60)
    return [(d["name"], d["type"]) for d in json.loads(out.stdout).get("devices", [])]


def probe(name, dtype):
    # --scan labels SATA disks behind this controller as "scsi", which makes
    # smartctl exit 4 with an empty attribute table. "auto" negotiates the SCSI
    # to ATA translation correctly, so it is tried first and the scanned type is
    # only the fallback (verified on /dev/sda, 2026-08-29).
    for candidate in ("auto", dtype):
        proc = subprocess.run(["smartctl", "-a", "-j", "-d", candidate, name],
                              capture_output=True, text=True, timeout=120)
        # smartctl returns a bitmask: bits 0-2 mean the command itself failed,
        # higher bits are health warnings that still carry usable data.
        if proc.returncode & 0b111:
            continue
        try:
            return json.loads(proc.stdout)
        except json.JSONDecodeError:
            continue
    return None


def escape(value):
    return str(value).replace("\\", "\\\\").replace('"', '\\"')


def main():
    os.makedirs(OUT_DIR, exist_ok=True)
    lines = []
    for metric, mtype, help_text in HEADERS:
        lines.append(f"# HELP {metric} {help_text}")
        lines.append(f"# TYPE {metric} {mtype}")

    for name, dtype in scan():
        data = probe(name, dtype)
        if data is None:
            continue
        labels = (f'device="{escape(name)}",'
                  f'model="{escape(data.get("model_name", "unknown"))}",'
                  f'serial="{escape(data.get("serial_number", "unknown"))}"')

        passed = data.get("smart_status", {}).get("passed")
        if passed is not None:
            lines.append(f"smart_device_health_ok{{{labels}}} {int(bool(passed))}")

        temp = data.get("temperature", {}).get("current")
        if temp is not None:
            lines.append(f"smart_device_temperature_celsius{{{labels}}} {temp}")

        hours = data.get("power_on_time", {}).get("hours")
        if hours is not None:
            lines.append(f"smart_device_power_on_hours{{{labels}}} {hours}")

        for attr in data.get("ata_smart_attributes", {}).get("table", []):
            raw = attr.get("raw", {}).get("value")
            if raw is None:
                continue
            if attr.get("id") == ATTR_REALLOCATED:
                lines.append(f"smart_device_reallocated_sectors{{{labels}}} {raw}")
            elif attr.get("id") == ATTR_PENDING:
                lines.append(f"smart_device_pending_sectors{{{labels}}} {raw}")

        # NVMe has no ATA attribute table; media_errors is its equivalent
        # early-warning signal and is mapped onto the same metric name.
        health = data.get("nvme_smart_health_information_log")
        if health is not None and "media_errors" in health:
            lines.append(f"smart_device_reallocated_sectors{{{labels}}} {health['media_errors']}")

    lines.append(f"smart_collector_last_run_seconds {int(time.time())}")

    tmp = OUT_FILE + ".tmp"
    with open(tmp, "w") as fh:
        fh.write("\n".join(lines) + "\n")
    os.replace(tmp, OUT_FILE)
    return 0


if __name__ == "__main__":
    sys.exit(main())
