#!/usr/bin/env bash
# Maak het gedeelde netwerk aan als het nog niet bestaat
docker network create paperless_network 2>/dev/null || true
