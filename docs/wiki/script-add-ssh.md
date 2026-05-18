# add-ssh.sh

> Idempotent interactive script to add or update SSH aliases in `~/.ssh/config` for LXC containers.

## Overview

`scripts/client/add-ssh.sh` manages SSH shorthand aliases on the developer's Linux desktop. Instead of remembering IP addresses, you can `ssh media`, `ssh gateway`, etc. The script reads the current `~/.ssh/config` to show existing aliases and their IPs, and updates entries in-place rather than appending duplicates.

## Usage

```bash
./scripts/client/add-ssh.sh
# or via menu:
./client.sh → Register SSH alias for a new LXC
```

## Interactive Flow

The menu lists:
- **Update: `<stack>`  (IP: `<current_ip>`)** — for stacks that already have an entry
- **Create: `<stack>`** — for stacks in `stacks/` without an existing alias
- **Manually add a custom alias** — for arbitrary hostnames not matching a stack
- **Exit**

After selection, you enter (or confirm) the IPv4 address. The script writes the new or updated `Host` block to `~/.ssh/config`.

## Generated `~/.ssh/config` Entry

```
Host media
    HostName 10.10.10.5
    User root
```

## Idempotency

The script parses the existing `~/.ssh/config` using `awk` to extract all `Host`/`HostName` pairs before showing the menu. If an alias already exists, the Update option pre-fills the current IP. The update is done via a safe atomic write: the new config is built in a temp file and swapped in, so a failure never corrupts the original.

## Rollback Behaviour

A `trap cleanup_on_error EXIT` deletes the temp file if the script fails mid-write, ensuring `~/.ssh/config` is never left in a corrupt state.

## Libraries Used

- [lib-ui.sh](lib-ui.md)

## See also

- [Networking](networking.md)
- [script-client-sh.md](script-client-sh.md)
- [script-bootstrap-lxc.md](script-bootstrap-lxc.md)
