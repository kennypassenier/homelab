# Part 8: Centralized Monitoring & Observability

In a distributed homelab where applications are isolated across multiple Proxmox LXC containers, logging into each container individually to read Docker logs becomes inefficient. To solve this, we use a centralized observability stack: **Grafana, Loki, and Promtail**.

This guide explains how our logging pipeline works and how you can view your infrastructure's logs from a single dashboard.

---

## 1. The Monitoring Architecture

Our monitoring stack is built on the "PLG" (Promtail, Loki, Grafana) architecture. It is lightweight, highly efficient, and specifically designed for containerized environments.

### A. Grafana (The UI)
Grafana provides the graphical interface. It sits in the `monitoring` stack and connects to Loki to visualize logs and metrics. We use automated provisioning, meaning Grafana is already pre-configured to use Loki as its primary data source out of the box.

### B. Loki (The Database)
Loki is a horizontally scalable, highly available log aggregation system. Unlike Elasticsearch, Loki does not index the contents of the logs, but only indexes labels (like `stack_name` or `container_name`). This makes it extremely fast and lightweight—perfect for a homelab. Loki also runs centrally in the `monitoring` stack.

### C. Promtail (The Agent)
Promtail is the agent that actually collects the logs. Instead of running centrally, **a Promtail container is deployed to every single stack** (e.g., inside the `media` LXC, the `paperless` LXC, etc.).
*   It mounts the host's `/var/log` (for system logs) and `/var/lib/docker/containers` (for Docker logs).
*   It tails these files in real-time and ships them securely over the network to the central Loki server.

Each Promtail instance also scrapes `/var/log/node-sync.log` via a dedicated `node_sync` job. This log contains structured **logfmt** output from `node-sync.sh` (e.g. `ts=... level=warn stack=media app=jellyfin msg="..."`). Promtail parses this format and promotes `level`, `stack`, and `app` as Loki labels, making GitOps sync events fully queryable in Grafana.

---

## 2. Automated Configuration (GitOps & Secrets)

Setting up a Promtail agent in a new stack is completely automated by our GitOps pipeline.

1.  **Generation:** When you run `./client.sh` and select **`1. Create a new Stack`**, the wizard will ask if you want to include a central Promtail container.
2.  **Environment Expansion:** Promtail configurations (`config.yml`) usually require hardcoding the IP address of the Loki server. To avoid committing static IP addresses to Git, we launch Promtail with the `-config.expand-env=true` flag.
3.  **Environment Injection:** The Loki IP is stored in the stack's `.env` file as `LOKI_IP=10.10.10.x`. When the `node-sync.sh` script deploys the stack on the server, Docker Compose injects the `LOKI_IP` dynamically into the Promtail configuration.

---

## 3. How to View Your Logs (User Guide)

When you need to troubleshoot an issue or simply want to monitor your applications, you do not need to SSH into the servers.

### Step 1: Access Grafana
1. Open your web browser and navigate to your Grafana instance (e.g., `http://<monitoring_lxc_ip>:3000` or your reverse proxy domain).
2. Log in with your admin credentials.

### Step 2: Use the Explore Tab
1. In the left-hand menu, click on the **Explore** icon (it looks like a compass).
2. At the top left of the Explore page, ensure the data source dropdown is set to **Loki**.

### Step 3: Query Your Logs
Loki uses LogQL (Log Query Language). Because Promtail automatically tags all logs with labels, you can filter them effortlessly.

Click on the **Label filters** button or type a query directly:
*   To see all logs from the `media` stack:
    `{host="media"}`
*   To see only Docker container logs from the `gateway` stack:
    `{host="gateway", job="docker"}`
*   To search for the word "error" in the `paperless` stack:
    `{host="paperless"} |= "error"`
*   To see all GitOps sync warnings across all stacks:
    `{job="node_sync", level="warn"}`
*   To see sync events for a specific app:
    `{job="node_sync", stack="media", app="jellyfin"}`

Press **Run query** (or Shift+Enter) to stream the logs in real-time.
