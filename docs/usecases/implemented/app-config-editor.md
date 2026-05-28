# Use Case: App Config Editor

**Tier:** CLIENT
**Status:** Implemented

## Behavior

- App rows in the Scaffolding tab open an app-level config editor.
- The editor updates Git-managed app metadata without touching secrets.
- The initial implementation supports safe Docker image edits in each app's `docker-compose.yml`.
- Changes are committed through the existing GitOps path.

## Implemented In

- client-app/src/events.rs
- client-app/src/blast_radius.rs
- client-app/src/ui.rs
- client-app/src/stack_features.rs