# Planned Use Case: Template Catalog and Day-2 Recipes

**Tier:** CLIENT + GitOps repo
**Status:** Planned

## Goal

Provide reusable stack/app templates and guided day-2 operations (upgrade, migrate, rotate secrets, scale).

## Why It Matters

Portainer-class tools gain adoption from template catalogs; Ansible-class workflows gain reliability from repeatable runbooks. This bridges both: fast bootstrap + controlled lifecycle operations.

## Candidate Capabilities

- curated template catalog (official + local templates)
- parameterized scaffold (ports, volumes, labels, backup profile)
- day-2 recipe library for common tasks (major upgrade, data migration, key rotation)
- recipe dry-run mode with required prechecks

## Suggested Guardrails

- recipe version pinning and compatibility checks
- mandatory backup checkpoint before risky recipes
- recipe execution transcript attached to operation history

## Dependencies

- template metadata schema
- recipe execution engine with preconditions/postconditions
- docs generation from template and recipe metadata
