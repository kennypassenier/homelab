# container.sh — Container Manager

> Interactive menu inside an LXC container for triggering a manual GitOps sync.

## Overview

`container.sh` is the entry point for operations run from inside an LXC container. Its primary use is triggering a manual sync outside the 5-minute cron window — useful when you've just pushed a fix and don't want to wait.

Run it from the repository root inside the LXC (usually `/opt/gitops/`).

## Usage

```bash
./container.sh
```

## Menu Options

| Option | Description |
|---|---|
| 1 | Trigger Node Sync — pull from Git and deploy immediately |
| 0 | Exit |

## Auto-Detection of Stack Name

`container.sh` does not prompt for the stack name. It reads the stack from the cron job installed by [bootstrap-lxc.sh](script-bootstrap-lxc.md):

```bash
grep -o 'node-sync.sh [^ ]*' /etc/cron.d/gitops-sync | awk '{print $2}'
```

This means you never need to know or type the stack name — the container always knows which stack it is.

## See also

- [script-node-sync.md](script-node-sync.md)
- [script-client-sh.md](script-client-sh.md)
- [GitOps Flow](gitops-flow.md)
