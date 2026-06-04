# Use Case: Replace Infisical with Latch for Secret Management

## Overview

Replace Infisical with [Latch](https://github.com/kennypassenier/latch-rs) as the primary secret management and environment variable synchronization tool across the homelab infrastructure. Latch provides superior grouping capabilities for keeping multiple `.env` files in sync and streamlined authentication across distributed LXC containers.

## Current State (Infisical)

- Centralized secret management
- Manual environment variable configuration per service
- Complex deployment process for multi-container setups
- Inconsistent .env synchronization across services

## Proposed State (Latch)

### 1. **Multi-Service .env Synchronization via Grouping**

**Primary Use Case: Promtail Setup**
- Create a Latch group containing all `.env` files used by:
  - Promtail collectors (multiple LXC containers)
  - Loki log aggregation endpoint credentials
  - API keys for log processors
  
When a secret is updated in Latch, all grouped `.env` files automatically sync, eliminating manual propagation to individual containers.

```bash
# Example: Define a Promtail group
latch group create promtail-group
latch group add-member promtail-group /etc/promtail/.env.prod
latch group add-member promtail-group /var/containers/promtail-01/.env
latch group add-member promtail-group /var/containers/promtail-02/.env
```

### 2. **Simplified Multi-Container Authentication via Clone Command**

**Use Case: LXC Container Provisioning**

The `clone` command eliminates repetitive login processes when deploying Latch across multiple LXC containers.

```bash
# On the host machine (already authenticated)
latch clone lxc-container-01

# Instantly authenticates the container without manual credential entry
# Perfect for rapid container scaling
```

**Benefits:**
- Zero-downtime authentication propagation
- Automated CI/CD pipelines for container creation
- No exposed credentials during setup
- Rapid disaster recovery

### 3. **Expanded Grouping Opportunities**

**Database Credentials Group**
- MySQL/PostgreSQL connection strings
- Automatic sync across application servers
- Rotation updates propagate instantly

**API Keys Group**
- External service integrations (monitoring, notifications)
- Keep staging and production separate via different groups
- Audit trail of all credential changes

**Infrastructure Group**
- VPN credentials
- SSH keys for inter-container communication
- TLS certificates for internal services

## Implementation Strategy

### Phase 1: Proof of Concept (Promtail)
1. Install Latch on primary Promtail node
2. Create `promtail-secrets` group
3. Migrate Infisical Promtail secrets to Latch
4. Test group synchronization across 2-3 LXC containers
5. Validate log ingestion consistency

### Phase 2: Multi-Container Rollout
1. Use `latch clone` for automated LXC deployment
2. Extend grouping to related services (Loki, API aggregators)
3. Remove Infisical from Promtail infrastructure
4. Document clone workflow for infrastructure automation

### Phase 3: Full Migration
1. Migrate remaining service credentials
2. Establish groups by functional domain (database, API, infrastructure)
3. Implement Latch secret rotation policies
4. Decommission Infisical

## Key Advantages Over Infisical

| Feature | Infisical | Latch |
|---------|-----------|-------|
| .env Group Sync | ❌ Manual per-service | ✅ Automatic via groups |
| Container Auth | Manual per container | ✅ `clone` command |
| Promtail Integration | Complex setup | ✅ Purpose-built |
| LXC Native Support | Indirect | ✅ Direct |
| Operational Overhead | High | Low |

## Configuration Examples

### Promtail Group Definition
```bash
# Create group
latch group create promtail-prod

# Add all Promtail container .env files
for i in {01..10}; do
  latch group add-member promtail-prod /var/lxc/promtail-$i/.env
done

# Set group-level secret
latch secret set promtail-prod LOKI_URL "https://loki.homelab:3100"
```

### Clone for New Container
```bash
# Authenticate new LXC container instantly
latch clone lxc-promtail-11

# Container inherits all group secrets automatically
```

## Risk Mitigation

- **Backup**: Maintain Infisical as secondary until Latch fully operational
- **Gradual Migration**: Migrate by service, not all-at-once
- **Testing**: Validate group sync in staging before production
- **Audit**: Track all secret access and modifications

## Success Metrics

- ✅ 100% .env synchronization across Promtail group (< 1s latency)
- ✅ Container provisioning time reduced by 70% via `clone`
- ✅ Zero manual credential updates after initial setup
- ✅ Infisical decommissioned within 60 days

## Next Steps

1. Review Latch repository and test clone command locally
2. Set up staging Latch instance
3. Begin Phase 1 with Promtail group
4. Document all group schemas and clone procedures
