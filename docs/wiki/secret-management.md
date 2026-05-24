# Secret Management

Secrets (API keys, passwords, tokens) are stored in local uncommitted `.env` files alongside the `docker-compose.yml` they belong to. Do not commit `.env` files to version control. SOPS/Age is no longer used. All previous scripts for SOPS/Age setup or restore have been removed.
## Working with `.env` Files

```bash
# Add a new .env value — just edit the file normally:
echo "MY_SECRET=hunter2" >> stacks/media/radarr/.env

# Stage it — SOPS clean filter encrypts it automatically:
git add stacks/media/radarr/.env

# The committed version is encrypted. Verify:
git show HEAD:stacks/media/radarr/.env  # shows SOPS ciphertext

# In your working directory it is still plaintext — the smudge filter kept it decrypted.
cat stacks/media/radarr/.env            # shows plaintext
```

## The `.sops.yaml` Routing File

```yaml
creation_rules:
  - path_regex: .*
    key_groups:
    - age:
      - age1...  # your public key
```

The `.*` regex matches all files (not just `.env`) to avoid stdin filename matching errors in the Git clean filter. In practice, only `.env` files are filtered via `.gitattributes`.

## Security Notes

- **Never pass `AGE_PASSPHRASE` or `GITHUB_PAT` as CLI arguments** — they would be visible in `ps aux`. Always use a `.env` file on the host or interactive prompts.
- `.env` files in `scripts/host/` are not committed and should be `chmod 600` on the Proxmox host.


## See also

- [script-init-ground-zero.md](script-init-ground-zero.md)
- [script-restore-client.md](script-restore-client.md)
- [script-bootstrap-lxc.md](script-bootstrap-lxc.md)
- [GitOps Flow](gitops-flow.md)
- [Architecture Overview](architecture-overview.md)

## Dynamic Secrets Provisioning (Infisical)

Secrets en gevoelige configuratie worden automatisch geëxporteerd vanuit Infisical naar .env-bestanden per stack/app. Dit gebeurt via de pre-sync.sh scripts vóórdat containers starten, zodat alle apps hun secrets als environment variables krijgen.

- De Infisical CLI wordt aangeroepen door pre-sync.sh om secrets veilig te exporteren naar de juiste .env-bestanden.
- Hierdoor blijven secrets buiten Git, is rotatie en beheer eenvoudig, en hebben containers altijd up-to-date secrets bij elke (her)deploy.

Zie ook: [pre-sync.sh](script-bootstrap-lxc.md).
