# Planned Use Case: Incident Capture Bundle

**Tier:** CLIENT + HOST + LXC
**Status:** Planned

## Goal

Capture a reproducible troubleshooting bundle in one action when something breaks.

## Why Useful

Comparable platforms improve supportability with diagnostics export. For one-admin use, this saves time and prevents missing key logs during incidents.

## Candidate Scope

- collect recent daemon logs, compose status, container health, git revision
- redact known secret patterns
- package into timestamped archive for local analysis
