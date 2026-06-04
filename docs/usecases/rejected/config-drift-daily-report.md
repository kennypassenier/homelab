# Planned Use Case: Config Drift Daily Report

**Tier:** CLIENT + HOST + LXC
**Status:** Planned

## Goal

Generate a lightweight daily drift summary for one-admin operations without requiring enterprise compliance tooling.

## Why Useful

Portainer/Ansible-style systems often surface drift. For homelab, a simple digest is enough: what changed, what diverged, and where manual fixes might be needed.

## Candidate Scope

- compare Git intent vs runtime compose/container state
- report stack-level drift markers (image tag mismatch, missing env target, stopped service)
- produce one compact summary entry (terminal + optional notification)
