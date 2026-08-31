"""Seed Uptime Kuma with a monitor set for the whole fleet.

Idempotent: a monitor whose name already exists is left exactly as it is,
so this can be re-run after adding a service without touching Kenny's own
edits. Every endpoint below was probed first and answered; a monitor that
is red from birth teaches you to ignore the dashboard.
"""
import sys
from uptime_kuma_api import UptimeKumaApi, MonitorType

OK = ["200-299"]
OK_REDIRECT = ["200-299", "302"]
OK_TRAEFIK = ["404"]  # correct answer for a host header it does not route

HTTP = [
    # (name, url, accepted)
    ("gateway · traefik",      "http://10.10.10.4:80/",                    OK_TRAEFIK),
    ("gateway · grafana",      "http://10.10.10.4:3000/api/health",        OK),
    ("gateway · loki",         "http://10.10.10.4:3100/ready",             OK),
    ("gateway · uptime-kuma",  "http://10.10.10.4:3001/",                  OK_REDIRECT),
    ("metrics · prometheus",   "http://10.10.10.13:9090/-/healthy",        OK),
    ("metrics · alertmanager", "http://10.10.10.13:9093/-/healthy",        OK),
    ("media · jellyfin",       "http://10.10.10.6:8096/health",            OK),
    ("media · seerr",          "http://10.10.10.6:5055/api/v1/status",     OK),
    ("media · sonarr",         "http://10.10.10.6:8989/ping",              OK),
    ("media · radarr",         "http://10.10.10.6:7878/ping",              OK),
    ("media · prowlarr",       "http://10.10.10.6:9696/ping",              OK),
    ("media · bazarr",         "http://10.10.10.6:6767/",                  OK),
    ("downloader · qbittorrent", "http://10.10.10.5:8080/",                OK),
    ("syncthing · web",        "http://10.10.10.8:8384/rest/noauth/health", OK),
    ("almanac · healthz",      "http://10.10.10.12:8080/healthz",          OK),
    ("paperwork · actual",     "http://10.10.10.14:5006/",                 OK),
    ("paperwork · stirling",   "http://10.10.10.14:8080/login",            OK),
    ("paperwork · paperless",  "http://10.10.10.14:8000/accounts/login/",  OK),
    # Through cloudflared and Traefik. Every *.kp-soft.dev name sits behind
    # Cloudflare Access, so a healthy answer is the 302 to the login page.
    # The edge is one wildcard route, so a single external check is enough to
    # see the tunnel fail; two guard against a fluke in one of them.
    ("extern · fin.kp-soft.dev",  "https://fin.kp-soft.dev/",  OK_REDIRECT),
    ("extern · docs.kp-soft.dev", "https://docs.kp-soft.dev/", OK_REDIRECT),
]

PING = [
    ("host · gateway",      "10.10.10.4"),
    ("host · downloader",   "10.10.10.5"),
    ("host · media",        "10.10.10.6"),
    ("host · synctest",     "10.10.10.8"),
    ("host · kyu",          "10.10.10.9"),
    ("host · productivity", "10.10.10.11"),
    ("host · almanac",      "10.10.10.12"),
    ("host · metrics",      "10.10.10.13"),
    ("host · paperwork",    "10.10.10.14"),
]

api = UptimeKumaApi("http://10.10.10.4:3001")
api.login("kenny", sys.argv[1])
try:
    have = {m["name"] for m in api.get_monitors()}
    added = skipped = 0
    for name, url, accepted in HTTP:
        if name in have:
            skipped += 1
            continue
        api.add_monitor(type=MonitorType.HTTP, name=name, url=url,
                        interval=60, maxretries=2,
                        accepted_statuscodes=accepted)
        added += 1
        print("  +", name)
    for name, host in PING:
        if name in have:
            skipped += 1
            continue
        api.add_monitor(type=MonitorType.PING, name=name, hostname=host,
                        interval=60, maxretries=2)
        added += 1
        print("  +", name)
    print(f"\n{added} toegevoegd, {skipped} bestonden al")
finally:
    api.disconnect()
