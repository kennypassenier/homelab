# Rust service → homelab app in three steps (G9)

The homelab manages Docker images; your Rust repos make GitHub releases.
These two files bridge the gap — after that, your service is a normal app:
deploys, nightly auto-updates with rollback, backups, parking, all of it.

1. **In your Rust repo**: copy `Dockerfile` (replace `MYSERVICE` with your
   binary name) and `release-image.yml` → `.github/workflows/`. Tag a
   release (`vX.Y.Z`). After the FIRST push, set the GHCR package to
   public once (repo → Packages → settings → Change visibility) so the
   host can pull anonymously.

2. **In this repo**: copy `presets/rust-service/` to `presets/<yourname>/`,
   point the `myservice` compose at your image
   (`ghcr.io/<user>/<repo>:latest`), rename dirs/services to taste, and
   drop the RabbitMQ app if you don't need it.

3. **Deploy**: wizard (`n`) → pick the preset → done. Updates ride the
   existing chain: tag a release in the app repo → CI publishes the image
   → the nightly run picks it up (`com.homelab.update.policy=auto`) with
   automatic rollback — or `homelab update stacks/<name>` right away.

See docs/PRESET_GUIDE.md § "Your own Rust services" for details.
