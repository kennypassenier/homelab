//! Build a DeploySpec from a local stack directory (shared by CLI and TUI).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::Deserialize;

use homelab_proto::{CURRENT_CLIENT_SCHEMA, DeploySpec, FileBlob, GatewayRoute, StackManifest};

use crate::pinexists::PIN_ASK_WIDTH;

/// A typo used to be free. `latch_secret:` instead of `latch_secrets:` parsed
/// cleanly, deployed cleanly, and produced a container with no secrets in it;
/// `gateway_routes:` instead of `gateway_route:` produced a hostname with no
/// route. Both are the shape that cost the downloader its disks on
/// 2026-08-31 — a field the reader did not recognise and silently dropped.
///
/// `deny_unknown_fields` has to sit on the OUTER struct: `flatten` swallows
/// everything it does not recognise and hands it on, so the inner manifest can
/// never see a stray key.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StackFile {
    #[serde(flatten)]
    manifest: StackManifest,
    #[serde(default)]
    gateway_route: Option<GatewayRouteFile>,
    /// fix-91 (routes-outside-repo-unvalidated, 2026-09-27): route files kept
    /// under a name of their own, each read from `routes/<filename>` beside
    /// this file. They exist for the files that were written by hand on the
    /// gateway before the repository held them: renaming one to
    /// `<vmid>-app-<stack>.yml` would leave the old file routing the same
    /// hostname until someone deleted it by hand, so they keep the name they
    /// have. The deploy records them and a destroy removes them, which is
    /// what the derived name guarantees for `gateway_route`.
    #[serde(default)]
    extra_routes: Vec<GatewayRouteFile>,
    /// D12: apps whose .env comes from latch instead of a plaintext file.
    /// Client-side sugar only — the wire and the host vault see the same
    /// env content either way.
    #[serde(default)]
    latch_secrets: Vec<String>,
    /// latch-files (Kenny, 2026-09-30): secret FILES from latch, each for an
    /// absolute path in the container (a native unit's env file, a config
    /// holding a webhook id, a token file a docker app reads).
    #[serde(default)]
    latch_files: Vec<LatchFile>,
    /// fix-100 (apply-no-confirm-creates-drill, 2026-09-27): a stack that
    /// exists to be created and destroyed in one sitting, like the rollback
    /// drill. `homelab apply` and the DR runbook leave it out; it is deployed
    /// only by name. Client-side only, so it never reaches the intent hash.
    #[serde(default)]
    ephemeral: bool,
}

/// One `latch_files` entry: `from` is the file's path in latch under this
/// stack (`<stack>/<from>`), `dest` its absolute path in the container.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LatchFile {
    from: String,
    dest: String,
    /// Octal permission bits, e.g. "640".
    mode: String,
    /// `user:group` inside the container.
    #[serde(default)]
    owner: Option<String>,
    /// A native unit of this stack to restart when the file changed.
    #[serde(default)]
    restarts: Option<String>,
}

/// `deny_unknown_fields` for the same reason as on `StackFile`: a misspelt
/// `external:` would otherwise vanish and the route would be judged as if
/// it declared nothing.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GatewayRouteFile {
    filename: String,
    #[serde(default = "default_gw")]
    gateway_vmid: u16,
    /// fix-91: backends this file routes to on purpose that are not a stack
    /// homelab manages, each exactly as the file names it (Home Assistant,
    /// OPNsense, Proxmox).
    #[serde(default)]
    external: Vec<String>,
}

/// A route file a stack declares, with where it goes.
struct DeclaredRoute {
    decl: homelab_core::routes::RouteDecl,
    gateway_vmid: u16,
    /// `gateway_route` rather than one of the `extra_routes`.
    primary: bool,
}

/// Every route file `stack_file` declares, read from `dir`.
fn declared_routes(dir: &Path, stack_file: &StackFile) -> Result<Vec<DeclaredRoute>, String> {
    let stack = &stack_file.manifest.stack_name;
    let mut out = Vec::new();
    if let Some(g) = stack_file.gateway_route.as_ref() {
        // The filename is declared here and independently DERIVED by
        // destroy (`<vmid>-app-<stack>.yml`), which has no access to this
        // field — the manifest that reaches the host does not carry it.
        // As long as the two can disagree, a stack that names its file
        // anything else deploys fine and leaves a router behind when it is
        // destroyed, still answering for a hostname that has moved. That
        // is F115 exactly. Requiring them to agree removes the class
        // rather than the symptom.
        let derived = format!("{}-app-{}.yml", stack_file.manifest.vmid, stack);
        if g.filename != derived {
            return Err(format!(
                "gateway_route.filename is '{}' but destroy removes '{}' — \
                 they must match, or the route outlives the stack",
                g.filename, derived
            ));
        }
        let route_path = dir.join("traefik-routes.yml");
        let content = std::fs::read_to_string(&route_path)
            .map_err(|e| format!("gateway_route set but {}: {}", route_path.display(), e))?;
        out.push(DeclaredRoute {
            decl: homelab_core::routes::RouteDecl {
                stack: stack.clone(),
                filename: g.filename.clone(),
                content,
                external: g.external.clone(),
            },
            gateway_vmid: g.gateway_vmid,
            primary: true,
        });
    }
    for g in &stack_file.extra_routes {
        if g.filename.contains('/') || g.filename.contains("..") {
            return Err(format!(
                "extra_routes filename '{}' must be a bare file name",
                g.filename
            ));
        }
        let route_path = dir.join("routes").join(&g.filename);
        let content = std::fs::read_to_string(&route_path)
            .map_err(|e| format!("extra_routes names {}: {}", route_path.display(), e))?;
        out.push(DeclaredRoute {
            decl: homelab_core::routes::RouteDecl {
                stack: stack.clone(),
                filename: g.filename.clone(),
                content,
                external: g.external.clone(),
            },
            gateway_vmid: g.gateway_vmid,
            primary: false,
        });
    }
    Ok(out)
}

/// fix-92: every stack in `base` with its address (prefix length dropped),
/// and every route file the stacks declare. Secret-free and offline, like
/// [`route_files`], so `plan` can hold the whole fleet without latch.
#[allow(clippy::type_complexity)]
pub fn fleet_routes(
    base: &Path,
) -> Result<(Vec<(String, String)>, Vec<homelab_core::routes::RouteDecl>), String> {
    let mut stacks = Vec::new();
    let mut routes = Vec::new();
    for (name, dir) in scan_local_stacks(base) {
        let manifest = build_manifest(&dir).map_err(|e| format!("{}: {}", name, e))?;
        let ip = manifest
            .network
            .ip
            .split('/')
            .next()
            .unwrap_or_default()
            .to_string();
        stacks.push((manifest.stack_name, ip));
        routes.extend(route_files(&dir).map_err(|e| format!("{}: {}", name, e))?);
    }
    Ok((stacks, routes))
}

/// fix-92 (routes-outside-repo-unvalidated, 2026-09-27): what is wrong with
/// the routes of the stacks in `base`, taken together — see
/// [`homelab_core::routes::fleet_route_problems`]. Empty when nothing is.
pub fn fleet_route_problems(base: &Path) -> Result<Vec<String>, String> {
    let (stacks, routes) = fleet_routes(base)?;
    let mut problems = Vec::new();
    let mut facts = Vec::new();
    for r in &routes {
        // A file the check cannot read is one whose hostnames it never
        // compared, so it is a problem rather than a skip.
        match crate::routes::facts(r) {
            Ok(f) => facts.push(f),
            Err(e) => problems.push(format!(
                "route file {} (stack {}) is not valid YAML: {} — fix the file",
                r.filename, r.stack, e
            )),
        }
    }
    problems.extend(homelab_core::routes::fleet_route_problems(&stacks, &facts));
    Ok(problems)
}

/// fix-91: every route file a stack directory declares, without secrets,
/// latch or network — what the gateway would be given, and what the fleet
/// route check (fix-92) reads.
pub fn route_files(dir: &Path) -> Result<Vec<homelab_core::routes::RouteDecl>, String> {
    let manifest_path = dir.join("lxc-compose.yml");
    let raw = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("cannot read {}: {}", manifest_path.display(), e))?;
    let stack_file = parse_stack_file(&raw, &manifest_path)?;
    Ok(declared_routes(dir, &stack_file)?
        .into_iter()
        .map(|d| d.decl)
        .collect())
}

fn default_gw() -> u16 {
    104
}

/// Parse `lxc-compose.yml`, reporting a bad value where it actually is.
///
/// `StackFile` flattens the manifest, and a flattened struct is buffered
/// before it is read, so serde_yaml loses every position: a string where
/// `memory_mb` wants a number was reported "at line 2 column 1", whatever
/// line it sat on, and with nothing to do about it (test-plan part A,
/// 2026-09-26, finding 8). When the whole file fails, the manifest alone is
/// parsed again — that pass is not flattened, so its error carries the
/// field and the real line — and the message ends in a remedy, per standing
/// rule 11.
fn parse_stack_file(raw: &str, path: &Path) -> Result<StackFile, String> {
    serde_yaml::from_str::<StackFile>(raw).map_err(|whole| {
        let detail = match serde_yaml::from_str::<StackManifest>(raw) {
            Err(e) => e.to_string(),
            Ok(_) => whole.to_string(),
        };
        format!(
            "manifest parse: {} :: fix that value in {} and run `homelab plan` again",
            detail,
            path.display()
        )
    })
}

/// Just the intent — no files, no secrets, no latch (F291, see REGISTER.md).
///
/// The backup and destroy verbs need the manifest and nothing else: they run
/// entirely on the host against `/appdata` paths.
pub fn build_manifest(dir: &Path) -> Result<homelab_core::manifest::StackManifest, String> {
    let manifest_path = dir.join("lxc-compose.yml");
    let raw = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("cannot read {}: {}", manifest_path.display(), e))?;
    let stack_file = parse_stack_file(&raw, &manifest_path)?;
    Ok(stack_file.manifest)
}

pub fn build_spec(dir: &Path) -> Result<DeploySpec, String> {
    let mut notes = Vec::new();
    let built = spec_without_binaries(dir, &mut notes, true);
    for line in &notes {
        eprintln!("{}", line);
    }
    let mut spec = built?;
    // A5 (Kenny, 2026-09-02): a native stack carries its programs.
    //
    // Staged here rather than on the host because only the client can reach
    // GitHub — the same reason `install-native` ships bytes over the line.
    // A release that cannot be fetched is a warning, never a failed deploy:
    // GitHub being down must not stop a running stack from being reconciled,
    // and the host refuses to START a unit whose program is absent anyway.
    spec.native_binaries = stage_native_binaries(dir, &spec.manifest.natives);
    // registry-cache-plaintext (deep-dive answer, 2026-10-01): pin every
    // tag-only image to the digest its SOURCE registry hands out right now,
    // before the host ever points anything at the pull-through cache. The
    // cache answers over plain HTTP and can be told to serve a different
    // image for the same tag; it cannot do that for a digest docker itself
    // verifies. Best-effort and silent on failure beyond a note: a registry
    // having a bad evening must cost nothing more than staying unpinned for
    // this one deploy (the same stance D60 takes toward the cache itself).
    // fix-213: through `resolve_images_with`, the same function
    // `stack_digest` now calls — see its doc comment.
    let mut image_notes = Vec::new();
    resolve_images_with(&mut spec, &mut image_notes, |registry, repository, tag| {
        crate::pinexists::resolve_digest(registry, repository, tag)
    });
    for line in &image_notes {
        eprintln!("{}", line);
    }
    // fix-141: where these files came from, recorded by the host.
    spec.source = stack_source(dir);
    Ok(spec)
}

/// registry-cache-plaintext: resolve every tag-only `image:` line in `files`
/// against its source registry and rewrite it to `tag@sha256:…`. Returns one
/// note per image that could not be resolved (never fatal).
pub fn resolve_compose_digests(files: &mut [FileBlob]) -> Vec<String> {
    resolve_compose_digests_with(files, |registry, repository, tag| {
        crate::pinexists::resolve_digest(registry, repository, tag)
    })
}

/// fix-213 (fleet-check-and-apply-disagree-on-media): the one place that
/// decides what a compose file's `image:` lines look like for comparison —
/// called by `build_spec` (a real deploy, and `homelab apply`'s plan, which
/// both build every declared stack's spec through this same function) AND by
/// `stack_digest` (`homelab today`/`check`), so the two can never pin an
/// image differently again. Before this, `stack_digest` hashed the compose
/// file exactly as it reads on disk, while `build_spec` pinned every
/// tag-only `image:` line to the digest its registry answers with right now
/// (registry-cache-plaintext, 2026-10-01) before hashing it — so any app
/// whose compose file names a tag instead of a digest (the normal case)
/// hashed differently on each side forever, with nothing in the repository
/// ever having changed. `resolve` is injected so a test can prove the two
/// paths agree without reaching a real registry.
fn resolve_images_with<F>(spec: &mut DeploySpec, notes: &mut Vec<String>, resolve: F)
where
    F: Fn(&str, &str, &str) -> Result<Option<String>, String> + Sync,
{
    notes.extend(resolve_compose_digests_with(&mut spec.files, resolve));
}

fn resolve_compose_digests_with<F>(files: &mut [FileBlob], resolve: F) -> Vec<String>
where
    F: Fn(&str, &str, &str) -> Result<Option<String>, String> + Sync,
{
    // Every distinct reference across every compose file, asked once each —
    // same shape as `answer_pins` (gap-37), and for the same reason: a
    // stack's apps often share a registry, and asking it 8-wide instead of
    // one curl at a time is the difference between this costing seconds and
    // costing minutes of every deploy.
    use std::collections::BTreeSet;
    let mut by_ref: BTreeMap<String, homelab_core::ops::pinexists::TaggedImage> = BTreeMap::new();
    for f in files.iter() {
        if !f.path.ends_with("docker-compose.yml") {
            continue;
        }
        for img in homelab_core::ops::pinexists::tagged_images(&f.content) {
            by_ref.entry(img.reference.clone()).or_insert(img);
        }
    }
    let refs: Vec<&homelab_core::ops::pinexists::TaggedImage> = by_ref.values().collect();
    let notes: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let resolved: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());
    let dead: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
    let next = AtomicUsize::new(0);
    let workers = PIN_ASK_WIDTH.min(refs.len().max(1));
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(img) = refs.get(i) else { break };
                    let known_dead = dead
                        .lock()
                        .map(|d| d.contains(&img.registry))
                        .unwrap_or(false);
                    if known_dead {
                        if let Ok(mut n) = notes.lock() {
                            n.push(format!(
                                "[digest] {} not resolved :: {} did not answer — left as a tag",
                                img.reference, img.registry
                            ))
                        }
                        continue;
                    }
                    match resolve(&img.registry, &img.repository, &img.tag) {
                        Ok(Some(digest)) => {
                            if let Ok(mut r) = resolved.lock() {
                                r.insert(img.reference.clone(), digest);
                            }
                        }
                        Ok(None) => {
                            if let Ok(mut n) = notes.lock() {
                                n.push(format!(
                                    "[digest] {} answered with no Docker-Content-Digest — left as \
                                 a tag",
                                    img.reference
                                ));
                            }
                        }
                        Err(e) => {
                            if e.ends_with("did not answer")
                                && let Ok(mut d) = dead.lock()
                            {
                                d.insert(img.registry.clone());
                            }
                            if let Ok(mut n) = notes.lock() {
                                n.push(format!(
                                    "[digest] {} not resolved :: {} — left as a tag",
                                    img.reference, e
                                ));
                            }
                        }
                    }
                }
            });
        }
    });
    let resolved = resolved.into_inner().unwrap_or_default();
    if !resolved.is_empty() {
        for f in files.iter_mut() {
            if f.path.ends_with("docker-compose.yml") {
                f.content = homelab_core::ops::pinexists::rewrite_tag_lines(&f.content, &resolved);
            }
        }
    }
    notes.into_inner().unwrap_or_default()
}

#[cfg(test)]
mod registry_cache_plaintext_tests {
    //! registry-cache-plaintext (deep-dive answer, 2026-10-01): proves the
    //! part `resolve_compose_digests` cannot — the compose-file rewrite and
    //! the best-effort behaviour when a registry fails to answer — with an
    //! injected resolver standing in for `pinexists::resolve_digest`, the
    //! same shape `pin_ask_tests.rs` uses for the sibling gap-37 check.
    use super::*;

    fn blob(path: &str, content: &str) -> FileBlob {
        FileBlob {
            path: path.to_string(),
            content: content.to_string(),
            mode: None,
        }
    }

    #[test]
    fn a_resolved_tag_is_pinned_by_digest_and_an_unresolved_one_is_left_as_written() {
        let mut files = vec![
            blob(
                "stacks/x/docker-compose.yml",
                "services:\n  a:\n    image: ghcr.io/kp/app:1.2.3\n  b:\n    image: redis:7\n",
            ),
            blob("stacks/x/README.md", "image: not-a-compose-file:1\n"),
        ];
        let notes = resolve_compose_digests_with(&mut files, |_registry, repository, _tag| {
            if repository == "kp/app" {
                Ok(Some("sha256:aaaa".to_string()))
            } else {
                Ok(None) // answered, no Docker-Content-Digest header
            }
        });
        assert_eq!(
            files[0].content,
            "services:\n  a:\n    image: ghcr.io/kp/app:1.2.3@sha256:aaaa\n  \
             b:\n    image: redis:7\n"
        );
        // A non-compose file is never touched, even though it has an
        // `image:` line of its own.
        assert_eq!(files[1].content, "image: not-a-compose-file:1\n");
        assert!(
            notes
                .iter()
                .any(|n| n.contains("redis") && n.contains("no Docker-Content-Digest")),
            "{:?}",
            notes
        );
    }

    #[test]
    fn a_dead_registry_is_asked_once_and_every_other_image_on_it_is_noted_the_same_way() {
        let mut files = vec![blob(
            "stacks/x/docker-compose.yml",
            "services:\n  a:\n    image: dead.example/kp/one:1\n  \
             b:\n    image: dead.example/kp/two:1\n",
        )];
        let asked = std::sync::Mutex::new(Vec::new());
        let notes = resolve_compose_digests_with(&mut files, |registry, repository, _tag| {
            asked
                .lock()
                .unwrap()
                .push(format!("{}/{}", registry, repository));
            Err(format!("{} did not answer", registry))
        });
        // Nothing was resolved, so the compose file is byte-for-byte
        // unchanged — the "never fatal" contract this function documents.
        assert_eq!(
            files[0].content,
            "services:\n  a:\n    image: dead.example/kp/one:1\n  \
             b:\n    image: dead.example/kp/two:1\n"
        );
        assert_eq!(notes.len(), 2, "{:?}", notes);
        assert!(
            notes.iter().all(|n| n.contains("did not answer")),
            "{:?}",
            notes
        );
    }

    #[test]
    fn an_identical_reference_across_two_files_is_asked_once() {
        let mut files = vec![
            blob(
                "stacks/x/docker-compose.yml",
                "services:\n  a:\n    image: ghcr.io/kp/app:1\n",
            ),
            blob(
                "stacks/x/other/docker-compose.yml",
                "services:\n  b:\n    image: ghcr.io/kp/app:1\n",
            ),
        ];
        let asks = std::sync::atomic::AtomicUsize::new(0);
        resolve_compose_digests_with(&mut files, |_r, _repo, _tag| {
            asks.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(Some("sha256:bbbb".to_string()))
        });
        assert_eq!(asks.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(files[0].content.contains("@sha256:bbbb"));
        assert!(files[1].content.contains("@sha256:bbbb"));
    }

    /// fix-213 (fleet-check-and-apply-disagree-on-media): a resolver standing
    /// in for a registry that always answers — any registry, pinned or not,
    /// reproduces the fault, since before this fix `stack_digest` ran no
    /// resolver at all.
    fn fix_213_resolve(
        _registry: &str,
        _repository: &str,
        _tag: &str,
    ) -> Result<Option<String>, String> {
        Ok(Some("sha256:deadbeef".to_string()))
    }

    #[test]
    fn fix_213_check_and_deploy_pin_the_same_compose_image_the_same_way() {
        // Reproduces the media case measured 2026-10-02: a compose file
        // naming a tag (the normal, unpinned case), nothing in the
        // repository touched between the two builds, the same registry
        // answer both times. Before fix-213, `stack_digest` (what `homelab
        // today`/`check` sends) hashed the file exactly as written, while
        // `build_spec` (what a deploy records in `StackState` via
        // `component_digests`) pinned the image to a digest first — so this
        // assertion fails on the old code even though the two sides agree on
        // every byte of the repository.
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let dir = std::env::temp_dir().join(format!("homelab-fix-213-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(
            repo.join("stacks/syncthing/lxc-compose.yml"),
            dir.join("lxc-compose.yml"),
        )
        .unwrap();
        std::fs::write(dir.join("traefik-routes.yml"), "http: {}\n").unwrap();
        std::fs::create_dir_all(dir.join("syncthing")).unwrap();
        let compose = "services:\n  syncthing:\n    image: syncthing/syncthing:1.27\n";
        std::fs::write(dir.join("syncthing/docker-compose.yml"), compose).unwrap();
        std::fs::write(dir.join("syncthing/.env"), "SECRET=x\n").unwrap();
        std::fs::write(dir.join("syncthing/checks.yml"), "{}\n").unwrap();

        // What a deploy would record (`build_spec`'s own sequence, minus
        // native binaries and `.source`, which `component_digests` never
        // hashes).
        let mut notes = Vec::new();
        let mut deployed_spec = spec_without_binaries(&dir, &mut notes, false).unwrap();
        resolve_images_with(&mut deployed_spec, &mut notes, fix_213_resolve);
        let deployed = homelab_core::manifest::component_digests(&deployed_spec);

        // What the fleet check builds for the same directory, same resolver.
        let checked = stack_digest_with(&dir, fix_213_resolve).expect("digest");

        assert_eq!(
            checked.component_digests.files, deployed.files,
            "a compose file naming only a tag must hash the same for the fleet check and for \
             what a deploy records — otherwise `homelab today` and `homelab apply --plan` \
             disagree about every app that names a tag instead of a digest, as media's six apps \
             did on 2026-10-02"
        );
        assert_ne!(
            checked.files["syncthing/docker-compose.yml"],
            homelab_core::manifest::sha256_hex(compose.as_bytes()),
            "the file sent for comparison must be the pinned one, not the raw repository text"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// fix-219 (dashboard-apply-plan-ignores-pinning, locking test): a stack
    /// whose repository equals what the host recorded except for a
    /// tag-only `image:` line — pinned by the same injected resolver on
    /// both sides, standing in for the digest the host's own deploy
    /// recorded — must never redeploy. Before fix-219, `local_intent_hash`
    /// (the dashboard's cheap Apply-plan preview, `apply_plan(false)`) never
    /// ran `resolve_images_with`, so it hashed the bare tag while the host's
    /// record was built from `build_spec`'s pinned one — the exact fix-213
    /// fault, reopened in the one path fix-213 never reached. Routes
    /// through `homelab_client::apply::plan`, the one function `homelab
    /// apply` and the dashboard's Apply page both call to decide deploy vs
    /// unchanged (`admin::core::applyview::plan` is a thin wrapper over it).
    #[test]
    fn fix_219_an_unchanged_stack_with_only_a_tag_only_image_line_never_redeploys() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let dir = std::env::temp_dir().join(format!("homelab-fix-219-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(
            repo.join("stacks/syncthing/lxc-compose.yml"),
            dir.join("lxc-compose.yml"),
        )
        .unwrap();
        std::fs::write(dir.join("traefik-routes.yml"), "http: {}\n").unwrap();
        std::fs::create_dir_all(dir.join("syncthing")).unwrap();
        let compose = "services:\n  syncthing:\n    image: syncthing/syncthing:1.27\n";
        std::fs::write(dir.join("syncthing/docker-compose.yml"), compose).unwrap();
        std::fs::write(dir.join("syncthing/.env"), "SECRET=x\n").unwrap();
        std::fs::write(dir.join("syncthing/checks.yml"), "{}\n").unwrap();

        // What the host recorded at its last real deploy (`build_spec`'s own
        // sequence, pinned — fix-213's own shape).
        let mut notes = Vec::new();
        let mut deployed_spec = spec_without_binaries(&dir, &mut notes, false).unwrap();
        resolve_images_with(&mut deployed_spec, &mut notes, fix_213_resolve);
        let host_hash = homelab_core::manifest::intent_hash(&deployed_spec);

        // The dashboard's cheap preview path — no specs, no download, the
        // one `apply_plan(false)` actually calls.
        let (local_hash, _) = local_intent_hash_with(&dir, fix_213_resolve).expect("hash");
        assert_eq!(
            local_hash, host_hash,
            "a stack whose only unpinned line is a tag-only image must hash the same as what \
             the host recorded — otherwise the dashboard's Apply page (and the TUI's drift \
             badge) redeploys it forever"
        );

        let plan = crate::apply::plan(
            &[("syncthing".to_string(), local_hash)],
            &["syncthing".to_string()],
            &[("syncthing".to_string(), host_hash)],
        );
        assert_eq!(
            plan.unchanged,
            vec!["syncthing".to_string()],
            "unchanged except for pinning :: {:?}",
            plan
        );
        assert!(plan.deploy.is_empty(), "{:?}", plan);

        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// fix-141 (expert panel 2026-09-27, changes-reach-prod-without-ci): the
/// commit a stack directory is read from, and every file under it that
/// differs from that commit (modified, staged or untracked; ignored files
/// such as a local `.env` are not). None outside a git tree.
///
/// `deploy` and `apply` send what is on disk, not what is committed, and
/// nothing said so: a stack file could be live that exists in no commit.
/// The host records this with the deploy; the client warns about it.
pub fn stack_source(dir: &Path) -> Option<homelab_proto::SourceRev> {
    let git = |args: &[&str]| -> Option<String> {
        let o = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            // The stack directory's own repository, whatever a calling
            // hook exported.
            .env_remove("GIT_DIR")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_WORK_TREE")
            .args(args)
            .output()
            .ok()?;
        o.status
            .success()
            .then(|| String::from_utf8_lossy(&o.stdout).into_owned())
    };
    let commit = git(&["rev-parse", "HEAD"])?.trim().to_string();
    // `-z`: paths verbatim, one `XY path` entry per NUL; without renames an
    // entry is always one path, relative to the repository root.
    let status = git(&[
        "status",
        "--porcelain",
        "-z",
        "--untracked-files=all",
        "--no-renames",
        "--",
        ".",
    ])?;
    let mut uncommitted: Vec<String> = status
        .split('\0')
        .filter(|e| e.len() > 3)
        .map(|e| e[3..].to_string())
        .collect();
    uncommitted.sort();
    Some(homelab_proto::SourceRev {
        commit,
        uncommitted,
        client: crate::BUILD.to_string(),
    })
}

/// fix-142 (expert panel 2026-09-27, check-blind-to-repo-drift): a stack
/// directory as `homelab check` sends it, for comparison with what the host
/// last applied: the parsed manifest (None with only a `service.yml`) and a
/// sha256 of every file a deploy would send into the container.
///
/// Built from the same `collect` a deploy uses, so the two cannot disagree
/// about which files count; `.env` content is read by `collect` but never
/// leaves this function, and latch is not asked (F291: a check must not
/// depend on a credential it does not need).
///
/// fix-201 (today-reports-just-deployed-stacks-as-drifted): a compose stack's
/// manifest and files are now built through `spec_without_binaries` — the
/// same derivation a deploy's spec goes through, tile `probe` fields
/// included — rather than `build_manifest`'s raw parse, and
/// `component_digests` is hashed from that same spec. `build_manifest`
/// skips the route lookups that fill `probe`, so its manifest permanently
/// differed from `StackState::manifest` (which deploy records post-lookup)
/// for any stack with a tile: `homelab today` named `lxc-compose.yml` as
/// differing on every such stack, forever, right after a clean deploy. Still
/// `fetch_secrets: false` — env and secret files are excluded from both the
/// digests this produces and the comparison `evaluate_repo_drift` makes with
/// them.
///
/// fix-213 (fleet-check-and-apply-disagree-on-media): also pins every
/// tag-only `image:` line through `resolve_images_with` — the same step
/// `build_spec` runs before a real deploy records `StackState`'s
/// `component_digests`. Without it, every compose file naming a tag instead
/// of a digest (the normal case) hashed differently here than it did at
/// deploy time, forever, with nothing in the repository ever having changed:
/// `evaluate_repo_drift`'s raw-spec-vs-raw-spec comparison (fix-201) is only
/// as good as the two specs being built the same way.
pub fn stack_digest(dir: &Path) -> Result<homelab_core::ops::fleetcheck::StackDigest, String> {
    stack_digest_with(dir, |registry, repository, tag| {
        crate::pinexists::resolve_digest(registry, repository, tag)
    })
}

fn stack_digest_with<F>(
    dir: &Path,
    resolve: F,
) -> Result<homelab_core::ops::fleetcheck::StackDigest, String>
where
    F: Fn(&str, &str, &str) -> Result<Option<String>, String> + Sync,
{
    let stack = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or_else(|| format!("{}: not a stack directory", dir.display()))?;
    if !dir.join("lxc-compose.yml").exists() {
        // Native-only (`service.yml`): no manifest, no component digests —
        // `spec_without_binaries` requires `lxc-compose.yml` to exist, so
        // this stays on the old `collect`-only path.
        let mut files: Vec<FileBlob> = Vec::new();
        let mut env: BTreeMap<String, String> = BTreeMap::new();
        let mut checks: BTreeMap<String, homelab_core::checks::ServiceChecks> = BTreeMap::new();
        collect(dir, dir, &mut files, &mut env, &mut checks)?;
        return Ok(homelab_core::ops::fleetcheck::StackDigest {
            stack,
            manifest: None,
            files: files
                .into_iter()
                .map(|f| {
                    let h = homelab_core::manifest::sha256_hex(f.content.as_bytes());
                    (f.path, h)
                })
                .collect(),
            component_digests: Default::default(),
        });
    }
    let mut notes = Vec::new();
    let mut spec = spec_without_binaries(dir, &mut notes, false)?;
    resolve_images_with(&mut spec, &mut notes, resolve);
    let files = spec
        .files
        .iter()
        .map(|f| {
            let h = homelab_core::manifest::sha256_hex(f.content.as_bytes());
            (f.path.clone(), h)
        })
        .collect();
    let component_digests = homelab_core::manifest::component_digests(&spec);
    Ok(homelab_core::ops::fleetcheck::StackDigest {
        stack,
        manifest: Some(spec.manifest),
        files,
        component_digests,
    })
}

/// fix-141: what `deploy`/`apply` print before sending a stack whose files
/// differ from the commit. A warning, not a refusal: deploying a change
/// before committing it is a normal way to try it, and the host records it
/// either way.
pub fn uncommitted_warning(stack: &str, src: &homelab_proto::SourceRev) -> Option<String> {
    if src.uncommitted.is_empty() {
        return None;
    }
    const SHOWN: usize = 5;
    let mut names = src
        .uncommitted
        .iter()
        .take(SHOWN)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if src.uncommitted.len() > SHOWN {
        names.push_str(&format!(" and {} more", src.uncommitted.len() - SHOWN));
    }
    Some(format!(
        "⚠ {}: {} uncommitted file(s) go live that exist in no commit ({}); the host \
         records this deploy as {} — commit them to make it reproducible",
        stack,
        src.uncommitted.len(),
        names,
        src.summary()
    ))
}

/// fix-69 (see REGISTER.md): the intent hash of a local stack, for the TUI's
/// drift badge, with what would have been printed handed back as lines. The
/// programs are not part of the hash, so they are not fetched. The secrets
/// are, because the host's hash includes them, so `latch` still runs; the
/// caller runs this off the UI thread.
///
/// fix-219 (dashboard-apply-plan-ignores-pinning): pins every tag-only
/// `image:` line first, through `resolve_images_with` — the same step
/// `build_spec`/`stack_digest` already run (fix-213). Without it this cheap
/// path (no specs, no deploy) hashed a compose file's tag exactly as written
/// while the host's recorded hash was built from the pinned digest, so the
/// dashboard's Apply page (which calls this, never `build_spec`, for its own
/// preview) called every stack with a tag-only image changed forever, with
/// nothing in the repository ever having moved.
pub fn local_intent_hash(dir: &Path) -> Result<(String, Vec<String>), String> {
    local_intent_hash_with(dir, |registry, repository, tag| {
        crate::pinexists::resolve_digest(registry, repository, tag)
    })
}

fn local_intent_hash_with<F>(dir: &Path, resolve: F) -> Result<(String, Vec<String>), String>
where
    F: Fn(&str, &str, &str) -> Result<Option<String>, String> + Sync,
{
    let mut notes = Vec::new();
    let mut spec = spec_without_binaries(dir, &mut notes, true)?;
    resolve_images_with(&mut spec, &mut notes, resolve);
    Ok((homelab_core::manifest::intent_hash(&spec), notes))
}

/// fix-192 (media-redeploys-without-changing, Kenny 2026-10-02): the
/// per-component digests of a local stack, for the apply plan's reason text
/// — same cheap path as `local_intent_hash` (no native binaries, no registry
/// lookups for anything but the compose images, pinned the same way), with
/// `source` stamped too so a manifest-only reason can name the building
/// client's own version.
///
/// fix-219: also pins compose images first, for the same reason
/// `local_intent_hash` now does — `apply_plan`'s per-stack "why" text
/// (`applyview::deploy_reasons`) compares this against the host's own
/// (pinned) `component_digests`, and disagreed on every tag-only image
/// before this.
pub fn local_component_digests(
    dir: &Path,
) -> Result<(homelab_core::manifest::ComponentDigests, Vec<String>), String> {
    local_component_digests_with(dir, |registry, repository, tag| {
        crate::pinexists::resolve_digest(registry, repository, tag)
    })
}

fn local_component_digests_with<F>(
    dir: &Path,
    resolve: F,
) -> Result<(homelab_core::manifest::ComponentDigests, Vec<String>), String>
where
    F: Fn(&str, &str, &str) -> Result<Option<String>, String> + Sync,
{
    let mut notes = Vec::new();
    let mut spec = spec_without_binaries(dir, &mut notes, true)?;
    resolve_images_with(&mut spec, &mut notes, resolve);
    spec.source = stack_source(dir);
    Ok((homelab_core::manifest::component_digests(&spec), notes))
}

/// gap-34: `prune-orphans` needs `DeploySpec.files` (the paths a deploy
/// would write) to tell which files on the container are no longer in the
/// repository — `orphan_files_keeping` reads only `.files`, never `.env` or
/// `.secret_files`. Running latch to fill those two fields was pure cost: a
/// stack with `latch_secrets` or `latch_files` could not `prune-orphans`
/// without a working latch session even though no secret ever left this
/// function. This builds the same spec with `env` left as whatever is on
/// disk (never latch) and `secret_files` empty.
pub fn build_spec_files_only(dir: &Path) -> Result<DeploySpec, String> {
    let mut notes = Vec::new();
    let spec = spec_without_binaries(dir, &mut notes, false)?;
    for line in &notes {
        eprintln!("{}", line);
    }
    Ok(spec)
}

/// TUI parity round: a stack's files as a deploy sends them, without its
/// secrets and programs (no latch, no download), for the dashboard's plan:
/// the Deploy review and the Apply page diff these against what the host
/// applied (`GetApplied`), as `homelab apply` does.
///
/// fix-219 (dashboard-apply-plan-ignores-pinning): pins every tag-only
/// `image:` line first (fix-213's step, via `resolve_images_with`). Before
/// this, `deploy_diff` (the dashboard's Deploy review and Apply-page diff)
/// compared this function's UNPINNED compose file against the host's
/// PINNED `GetApplied` copy and called every app naming a tag instead of a
/// digest changed, on every read, with nothing in the repository ever
/// having moved — the fix-213 fault, in the one path fix-213 did not reach.
pub fn stack_files(dir: &Path) -> Result<Vec<FileBlob>, String> {
    stack_files_with(dir, |registry, repository, tag| {
        crate::pinexists::resolve_digest(registry, repository, tag)
    })
}

fn stack_files_with<F>(dir: &Path, resolve: F) -> Result<Vec<FileBlob>, String>
where
    F: Fn(&str, &str, &str) -> Result<Option<String>, String> + Sync,
{
    let mut files: Vec<FileBlob> = Vec::new();
    let mut env: BTreeMap<String, String> = BTreeMap::new();
    let mut checks: BTreeMap<String, homelab_core::checks::ServiceChecks> = BTreeMap::new();
    if !dir.join("lxc-compose.yml").is_file() {
        return Err(format!("{} has no lxc-compose.yml", dir.display()));
    }
    collect(dir, dir, &mut files, &mut env, &mut checks)?;
    let _notes = resolve_compose_digests_with(&mut files, resolve);
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// Everything of the deploy spec but the native programs.
///
/// `fetch_secrets`: whether to run latch at all. A caller that only needs
/// `.files` (gap-34: `prune-orphans`) passes `false` — `env` then holds
/// exactly what is on disk and `secret_files` stays empty, which costs
/// nothing because neither field is ever read for that purpose. A real
/// deploy, or anything that hashes the spec to detect drift, passes `true`.
fn spec_without_binaries(
    dir: &Path,
    notes: &mut Vec<String>,
    fetch_secrets: bool,
) -> Result<DeploySpec, String> {
    let manifest_path = dir.join("lxc-compose.yml");
    let raw = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("cannot read {}: {}", manifest_path.display(), e))?;
    let mut stack_file = parse_stack_file(&raw, &manifest_path)?;

    let mut files: Vec<FileBlob> = Vec::new();
    let mut env: BTreeMap<String, String> = BTreeMap::new();
    let mut checks: BTreeMap<String, homelab_core::checks::ServiceChecks> = BTreeMap::new();
    collect(dir, dir, &mut files, &mut env, &mut checks)?;
    // MR1 (Kenny, 2026-09-05): which source wins, and saying so out loud.
    //
    // A local `.env` beats latch. The rule it replaces refused the deploy
    // outright when an app had both, on the reasoning that a stale plaintext
    // file silently shadowing latch is what D12 exists to kill. Measured
    // before changing it: the whole repository held TWO local .env files
    // against thirteen latch-backed apps, and the guard had never once fired
    // on a real collision — the only time it ever fired was on a file put
    // there deliberately to get a blocked deploy moving.
    //
    // So the guard was asking "do both exist?" when the question that matters
    // is "which one did you use?" — F263's shape exactly. Precedence is not
    // the danger; silence is. Hence the report below: every app says where
    // its secrets came from, on every deploy, whether or not latch was
    // involved.
    let from_disk: std::collections::BTreeSet<String> = env.keys().cloned().collect();
    let secret_files = if fetch_secrets {
        fetch_latch_secrets(
            dir,
            &stack_file.latch_secrets,
            &stack_file.manifest.apps,
            &mut env,
            notes,
        )?;
        notes.extend(env_sources(&from_disk, &stack_file.latch_secrets, &env));
        fetch_latch_files(dir, &stack_file, notes)?
    } else {
        Vec::new()
    };

    // The external declarations stay here: they are the client's plan-time
    // question (fix-92), and the host writes a route the same either way.
    let mut gateway_route = None;
    let mut extra_routes = Vec::new();
    // checks-link (Kenny, 2026-09-30): each app's manual checks and probes carry where
    // the app is opened, read from the router whose service is that app,
    // unless its checks.yml names an address itself.
    let mut addresses = std::collections::BTreeMap::new();
    for d in declared_routes(dir, &stack_file)? {
        for (service, url) in crate::routes::service_addresses(&d.decl.content)? {
            addresses.entry(service).or_insert(url);
        }
    }
    for (app, sc) in checks.iter_mut() {
        if sc.url.is_none() && (!sc.manual.is_empty() || !sc.probes.is_empty()) {
            sc.url = addresses.get(app).cloned();
        }
    }

    // tile-watch (owner decision "Afgeleid uit de tegels", 2026-09-30): each
    // tile's plain probe address, so the firewall derivation and the
    // dashboard's minute watch both reach it directly — never through
    // Traefik (fix-89 closed that door for a measurement: a Host-header
    // proxy hop answers for the front door, not the backend). The address
    // resolved is `watch_url` when the tile sets one (for example an app's
    // own health endpoint, rather than the page the tile opens), else
    // `url`. An explicit address that already names this container's own
    // address is used as written; otherwise its host (or the tile's own
    // key when it has no `url`/`watch_url`) is looked up in this stack's
    // own declared route files. Neither: the tile carries no probe and is
    // not watched — said here once, rather than discovered later as a
    // silent gap.
    let own_ip = stack_file
        .manifest
        .network
        .ip
        .split('/')
        .next()
        .unwrap_or("")
        .to_string();
    let route_contents: Vec<String> = declared_routes(dir, &stack_file)?
        .into_iter()
        .map(|d| d.decl.content)
        .collect();
    for (host, t) in stack_file.manifest.tiles.iter_mut() {
        let url = t
            .watch_url
            .clone()
            .or_else(|| t.url.clone())
            .unwrap_or_else(|| format!("https://{}/", host));
        let url_host = url
            .split_once("://")
            .map(|(_, rest)| rest)
            .and_then(|rest| rest.split(['/', '?', '#']).next())
            .map(|authority| authority.rsplit_once(':').map_or(authority, |(h, _)| h))
            .unwrap_or_default();
        if url_host == own_ip {
            t.probe = Some(url);
            continue;
        }
        t.probe = route_contents
            .iter()
            .find_map(|c| crate::routes::backend_for_host(c, url_host));
        if t.probe.is_none() {
            notes.push(format!(
                "tile {}: no route in stacks/{}/traefik-routes.yml forwards {} to a backend — \
                 not watched",
                host, stack_file.manifest.stack_name, url_host
            ));
        }
    }

    for d in declared_routes(dir, &stack_file)? {
        let route = GatewayRoute {
            gateway_vmid: d.gateway_vmid,
            filename: d.decl.filename,
            content: d.decl.content,
        };
        if d.primary {
            gateway_route = Some(route);
        } else {
            extra_routes.push(route);
        }
    }

    // The programs are staged by `build_spec`, and only there.
    let native_binaries = BTreeMap::new();

    Ok(DeploySpec {
        secret_files,
        source: None,
        manifest: stack_file.manifest,
        files,
        env,
        gateway_route,
        extra_routes,
        checks,
        native_binaries,
        native_manifests: native_manifests_for(dir),
        client_schema: CURRENT_CLIENT_SCHEMA,
    })
}

/// Every native manifest a stack directory holds, keyed by unit name.
///
/// Pure and public so the layout question has one answer and a test can ask
/// it without a network: see F301 for what two readers of the same layout
/// cost.
pub fn native_manifests_for(dir: &Path) -> BTreeMap<String, homelab_proto::NativeServiceManifest> {
    native_services(dir)
        .into_iter()
        .map(|(m, _unit_file)| (m.unit.clone(), m))
        .collect()
}

/// Fetch each native unit's binary from its own release, verified.
fn stage_native_binaries(dir: &Path, natives: &[String]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    // F301: resolved through `native_services`, which knows BOTH layouts, and
    // not by guessing `<unit>/service.yml`. A stack with one native keeps its
    // file at the top (`stacks/almanac/service.yml`); a stack with several
    // gives each a directory. kyu is both at once — three natives, with its
    // own file still at the top from when it was the only one — so this loop
    // silently skipped the hub itself. Every deploy staged kyu-runner and
    // http-switchboard and never once the program the container exists for,
    // with nothing in the transcript to say so. Two readers of the same
    // layout is what let them disagree; there is one now.
    let resolved = native_manifests_for(dir);
    for unit in natives {
        let Some(m) = resolved.get(unit).cloned() else {
            eprintln!(
                "  · {} is declared in natives but no service.yml describes it — \
                 binary not shipped",
                unit
            );
            continue;
        };
        // No release_repo is a deliberate state, not an oversight: the
        // service is adopt-only and where its binary comes from is not
        // written down. Saying so is more useful than a silent skip.
        let Some(repo) = m.release_repo.clone() else {
            eprintln!(
                "  · {} declares no release_repo — its binary is not shipped with this deploy",
                unit
            );
            continue;
        };
        let Some(tag) = crate::release::latest_tag_of(&repo) else {
            eprintln!(
                "  · {}: no release found in {} — binary not shipped",
                unit, repo
            );
            continue;
        };
        match crate::release::stage_asset(&repo, &tag, m.asset_name()) {
            Ok(b64) => {
                println!(
                    "  · {} {} staged from {} (checksum verified)",
                    unit, tag, repo
                );
                out.insert(unit.clone(), b64);
            }
            Err(e) => eprintln!("  · {}: {} — binary not shipped", unit, e),
        }
    }
    out
}

fn collect(
    root: &Path,
    dir: &Path,
    files: &mut Vec<FileBlob>,
    env: &mut BTreeMap<String, String>,
    checks: &mut BTreeMap<String, homelab_core::checks::ServiceChecks>,
) -> Result<(), String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };
    for entry in entries.flatten() {
        let path: PathBuf = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            // fix-91: `routes/` at the top holds the stack's extra route
            // files — orchestrator input like traefik-routes.yml, bound for
            // the gateway and not for this container.
            if dir == root && name == "routes" {
                continue;
            }
            collect(root, &path, files, env, checks)?;
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .to_string();
        // Orchestrator input, not container content: service.yml describes a
        // native unit to the homelab and has no business inside the container.
        if rel == "lxc-compose.yml" || rel == "traefik-routes.yml" || name == "service.yml" {
            continue;
        }
        // Same reason: checks.yml says what healthy MEANS for this service.
        // That is the homelab's question about the service, not something the
        // service has to be told, so it travels beside the spec rather than
        // into the container.
        if name == "checks.yml" {
            let app = path
                .parent()
                .and_then(|p| p.file_name())
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {}", rel, e))?;
            let parsed: homelab_core::checks::ServiceChecks =
                serde_yaml::from_str(&text).map_err(|e| format!("{}: {}", rel, e))?;
            checks.insert(app, parsed);
            continue;
        }
        let content = std::fs::read_to_string(&path)
            .map_err(|_| format!("non-utf8 file not supported: {}", rel))?;
        if name == ".env" {
            let app = path
                .parent()
                .and_then(|p| p.file_name())
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            env.insert(app, content);
        } else {
            files.push(FileBlob {
                path: rel,
                content,
                mode: None,
            });
        }
    }
    Ok(())
}

/// D12: fill in each declared app's env by asking latch, in memory only —
/// no plaintext .env needs to exist on the workstation. The latch project
/// root is the stacks/ directory (one `latch init` there, once), so the
/// path inside the project is `<stack>/<app>/.env`. `--expand` is
/// deliberate: docker compose does its own ${VAR} interpolation on .env
/// content, so raw latch templates would collide with it — latch resolves
/// them first or fails hard.
/// One line per app saying where its secrets came from (MR1).
///
/// Pure on purpose (D110/MR1, see REGISTER.md): the report IS the measure
/// Kenny chose, so it has to be testable without capturing stderr.
pub fn env_sources(
    from_disk: &std::collections::BTreeSet<String>,
    latch_secrets: &[String],
    env: &BTreeMap<String, String>,
) -> Vec<String> {
    env.keys()
        .map(|app| {
            let source = match (from_disk.contains(app), latch_secrets.contains(app)) {
                // The case MR1 is about: declared in latch, answered on disk.
                (true, true) => "local .env (latch skipped)",
                (true, false) => "local .env",
                (false, _) => "latch",
            };
            format!("[env] {} <- {}", app, source)
        })
        .collect()
}

/// latch-files: read every declared file from latch, checked before any
/// latch call, so a typo fails the plan and not the container.
fn fetch_latch_files(
    dir: &Path,
    stack_file: &StackFile,
    notes: &mut Vec<String>,
) -> Result<Vec<homelab_core::manifest::SecretFile>, String> {
    let mut out = Vec::new();
    if stack_file.latch_files.is_empty() {
        return Ok(out);
    }
    for f in &stack_file.latch_files {
        check_latch_file(f, &stack_file.manifest.natives)?;
    }
    let latch_env = std::env::var("HOMELAB_LATCH_ENV").map_err(|_| {
        "latch_files is set but HOMELAB_LATCH_ENV is not :: set it to the \
         latch environment to read (e.g. HOMELAB_LATCH_ENV=prod in .env)"
            .to_string()
    })?;
    let stack = &stack_file.manifest.stack_name;
    let project_root = dir.parent().unwrap_or(dir);
    for f in &stack_file.latch_files {
        let rel = format!("{}/{}", stack, f.from);
        // Raw, not `--expand`: a file is stored exactly as it must land, and
        // a config may carry `${VAR}` for its own program to resolve
        // (http-switchboard's `token = "${KYU_TOKEN}"`), which latch would
        // otherwise try to expand and refuse.
        let got = std::process::Command::new("latch")
            .args(["cat", &rel, "--env", &latch_env])
            .current_dir(project_root)
            .output()
            .map_err(|e| {
                format!(
                    "cannot run latch for {}: {} :: install latch (or remove \
                     latch_files from the stack file)",
                    rel, e
                )
            })?;
        if !got.status.success() {
            return Err(format!(
                "latch cat {} --env {} failed: {}",
                rel,
                latch_env,
                String::from_utf8_lossy(&got.stderr).trim()
            ));
        }
        if !got.stderr.is_empty() {
            notes.push(format!(
                "[latch] {}",
                String::from_utf8_lossy(&got.stderr).trim()
            ));
        }
        let content = String::from_utf8(got.stdout)
            .map_err(|_| format!("latch returned non-utf8 content for {}", rel))?;
        if content.trim().is_empty() {
            return Err(format!(
                "latch returned empty content for {} in env '{}' :: commit+push \
                 the file in latch first",
                rel, latch_env
            ));
        }
        // latch parses every file of an environment as variables when any
        // one of them is read with `--expand`; a `${VAR}` it cannot resolve
        // in THIS file then refuses every other stack's secrets too. On
        // 2026-09-30 http-switchboard's `token = "${KYU_TOKEN}"` did exactly
        // that to the metrics deploy. Refused here so the next deploy of
        // this stack names the file, rather than another stack failing.
        if content.contains("${") {
            return Err(format!(
                "{} in latch contains `${{`, which latch reads as a template in \
                 every --expand of this environment :: write the value itself, \
                 then `latch put {} --env {}`",
                rel, rel, latch_env
            ));
        }
        notes.push(format!("[secret] {} <- latch {}", f.dest, rel));
        out.push(homelab_core::manifest::SecretFile {
            path: f.dest.clone(),
            content,
            mode: f.mode.clone(),
            owner: f.owner.clone(),
            restarts: f.restarts.clone(),
        });
    }
    Ok(out)
}

/// What a `latch_files` entry may say. `dest` absolute and plain, `mode`
/// three or four octal digits, `owner` a `user:group` of names or numbers,
/// `restarts` a native unit of this stack, `from` a relative path latch
/// accepts.
fn check_latch_file(f: &LatchFile, natives: &[String]) -> Result<(), String> {
    let plain = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-'))
            && !s.split('/').any(|p| p == "..")
    };
    if !f.dest.starts_with('/') || !plain(&f.dest) {
        return Err(format!(
            "latch_files: dest '{}' must be an absolute path of letters, digits, \
             '/', '.', '_' and '-'",
            f.dest
        ));
    }
    if f.from.starts_with('/') || !plain(&f.from) {
        return Err(format!(
            "latch_files: from '{}' must be a relative path inside the stack",
            f.from
        ));
    }
    let octal = (3..=4).contains(&f.mode.len()) && f.mode.chars().all(|c| ('0'..='7').contains(&c));
    if !octal {
        return Err(format!(
            "latch_files: mode '{}' for {} must be octal like \"640\"",
            f.mode, f.dest
        ));
    }
    if let Some(o) = &f.owner {
        let part = |p: &str| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        };
        let ok = matches!(o.split_once(':'), Some((u, g)) if part(u) && part(g));
        if !ok {
            return Err(format!(
                "latch_files: owner '{}' for {} must be user:group",
                o, f.dest
            ));
        }
    }
    if let Some(u) = &f.restarts
        && !natives.contains(u)
    {
        return Err(format!(
            "latch_files: restarts '{}' for {} is not a native unit of this \
                 stack :: natives are [{}]",
            u,
            f.dest,
            natives.join(", ")
        ));
    }
    Ok(())
}

fn fetch_latch_secrets(
    dir: &Path,
    apps: &[String],
    manifest_apps: &[String],
    env: &mut BTreeMap<String, String>,
    notes: &mut Vec<String>,
) -> Result<(), String> {
    if apps.is_empty() {
        return Ok(());
    }
    // A latch_secrets entry that names no real app would fetch secrets into
    // the void; manifest app names are also what keeps the latch path free
    // of characters latch refuses (validated [a-z0-9-], so no '__').
    for app in apps {
        if !manifest_apps.contains(app) {
            return Err(format!(
                "latch_secrets names '{}' but the stack has no such app :: \
                 apps are [{}]",
                app,
                manifest_apps.join(", ")
            ));
        }
    }
    let latch_env = std::env::var("HOMELAB_LATCH_ENV").map_err(|_| {
        "latch_secrets is set but HOMELAB_LATCH_ENV is not :: set it to the \
         latch environment to read (e.g. HOMELAB_LATCH_ENV=prod in .env)"
            .to_string()
    })?;
    let stack = dir
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or_else(|| "stack dir has no name".to_string())?;
    let project_root = dir.parent().unwrap_or(dir);
    for app in apps {
        // MR1: a local file wins, and the caller reports it. Skipping here
        // rather than letting latch overwrite is deliberate — this map is
        // filled from disk first, so removing the refusal without skipping
        // would silently give latch precedence, the opposite of the decision.
        if env.contains_key(app) {
            continue;
        }
        let rel = format!("{}/{}/.env", stack, app);
        let out = std::process::Command::new("latch")
            .args(["cat", &rel, "--env", &latch_env, "--expand"])
            .current_dir(project_root)
            .output()
            .map_err(|e| {
                format!(
                    "cannot run latch for app '{}': {} :: install latch (or \
                     remove latch_secrets from the stack file)",
                    app, e
                )
            })?;
        if !out.status.success() {
            return Err(format!(
                "latch cat {} --env {} failed: {}",
                rel,
                latch_env,
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        // latch keeps content and messages strictly separated: informational
        // notes (e.g. the offline stale-cache notice) arrive on stderr with
        // exit 0 — pass them on rather than swallowing them.
        if !out.stderr.is_empty() {
            notes.push(format!(
                "[latch] {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        let content = String::from_utf8(out.stdout)
            .map_err(|_| format!("latch returned non-utf8 content for '{}'", app))?;
        if content.trim().is_empty() {
            return Err(format!(
                "latch returned empty content for app '{}' ({} in env '{}') :: \
                 commit+push the env file in latch first",
                app, rel, latch_env
            ));
        }
        env.insert(app.clone(), content);
    }
    Ok(())
}

/// Scan a directory for deployable stack dirs (those with lxc-compose.yml).
pub fn scan_local_stacks(base: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(base) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join("lxc-compose.yml").exists() {
                out.push((entry.file_name().to_string_lossy().to_string(), path));
            }
        }
    }
    out.sort();
    out
}

/// fix-100: does this stack directory say `ephemeral: true`? A file that
/// does not parse is not ephemeral: the verbs that read it report the parse
/// error themselves.
pub fn is_ephemeral(dir: &Path) -> bool {
    let path = dir.join("lxc-compose.yml");
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| parse_stack_file(&raw, &path).ok())
        .is_some_and(|f| f.ephemeral)
}

/// fix-100 (see REGISTER.md): the stacks the repository declares as part of
/// the fleet, which `apply` holds the host to and the DR runbook rebuilds.
pub fn declared_stacks(base: &Path) -> Vec<(String, PathBuf)> {
    scan_local_stacks(base)
        .into_iter()
        .filter(|(_, dir)| !is_ephemeral(dir))
        .collect()
}
// ── E7: the disaster-recovery runbook ───────────────────────────────────────
//
// Rewritten at the Phase 8 gate (Kenny, "Herschrijven", 2026-09-27): every
// command below is taken from the code that does the same thing when the
// daemon is up, and every path and name is read from that code or from the
// stack files rather than typed into the prose. Where the code keeps no
// answer (where the offline password copy is kept) the text says so.

/// The host's state directory. Derived from the restic password file, which
/// `BackupCfg::default()` places in `<state_dir>/secrets/`, so the document
/// and the backup code cannot name two different directories.
/// The daemon's units as the binary carries them (`homelab_core::hostunits`),
/// so a pve rebuilt from this runbook gets the watchdog and the rollback,
/// not a minimal unit without either (expert panel, daemon-units-outside-repo).
fn runbook_host_units() -> String {
    let mut out = String::from(
        "**The units.** The daemon runs under these files; the binary carries them and \
         every self-update puts them in place, and `homelab doctor` names any that differ. \
         On a rebuilt host, write them before the first start, then \
         `systemctl daemon-reload && systemctl enable --now homelab-host`.\n\n",
    );
    for u in homelab_core::hostunits::UNITS {
        let lang = if u.path.ends_with(".sh") { "sh" } else { "ini" };
        out.push_str(&format!(
            "`{}` (mode {:o}):\n\n```{}\n{}```\n\n",
            u.path, u.mode, lang, u.content
        ));
    }
    out
}

fn runbook_state_dir(bcfg: &homelab_core::ops::backup::BackupCfg) -> String {
    Path::new(&bcfg.password_file)
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "/var/lib/homelab".into())
}

/// `rclone:<remote>:<path>` split into the remote name and the folder, so
/// the rclone check in Layer 3 names the same remote restic will use.
fn rclone_parts(restic_base: &str) -> Option<(String, String)> {
    let rest = restic_base.strip_prefix("rclone:")?;
    let (remote, path) = rest.split_once(':')?;
    Some((remote.to_string(), path.to_string()))
}

/// The vault file deploy.rs keeps for a file a native unit reads:
/// `<state_dir>/secrets/<stack>/<parent dir>/<name>` since fix-37. Calls the
/// deploy's own `vault_key`, so the runbook cannot name another file than the
/// one the deploy restores from.
fn vault_file(vault: &str, stack: &str, path: &str) -> String {
    format!(
        "{}/{}/{}",
        vault,
        stack,
        homelab_core::ops::deploy::vault_key(path)
    )
}

/// Where a native unit's `service.yml` sits, for `homelab adopt`, which reads
/// `<dir>/service.yml`. A stack with one unit keeps it at the top; a stack
/// with several gives each unit a directory (see `native_services`).
fn native_service_dir(stack_dir: &Path, dir_name: &str, unit: &str) -> Option<String> {
    let top = stack_dir.join("service.yml");
    let sub = stack_dir.join(unit).join("service.yml");
    let unit_of = |p: &Path| -> Option<String> {
        let raw = std::fs::read_to_string(p).ok()?;
        serde_yaml::from_str::<homelab_proto::NativeServiceManifest>(&raw)
            .ok()
            .map(|m| m.unit)
    };
    if unit_of(&sub).as_deref() == Some(unit) {
        return Some(format!("stacks/{}/{}", dir_name, unit));
    }
    if unit_of(&top).as_deref() == Some(unit) {
        return Some(format!("stacks/{}", dir_name));
    }
    None
}

fn is_native_stack(m: &homelab_core::manifest::StackManifest) -> bool {
    m.native_only || (m.apps.is_empty() && !m.natives.is_empty())
}

/// One stack's section. Compose and native stacks come back by different
/// routes, and the data of each lives in a different shape of snapshot, so
/// the two are written differently.
fn runbook_stack_section(
    stack_dir: &Path,
    dir_name: &str,
    m: &homelab_core::manifest::StackManifest,
    bcfg: &homelab_core::ops::backup::BackupCfg,
    vault: &str,
) -> String {
    let mut s = format!("### {} (vmid {})\n\n", m.stack_name, m.vmid);
    s.push_str(&format!(
        "- Container: hostname `{}`, ip `{}` on `{}`{}, {} core(s), {} MiB RAM, {} MiB swap, \
         {} GiB disk on `{}`, {} template `{}`, boot order {}.\n",
        m.hostname,
        m.network.ip,
        m.network.bridge,
        m.network
            .vlan
            .map(|v| format!(" VLAN {}", v))
            .unwrap_or_default(),
        m.resources.cores,
        m.resources.memory_mb,
        m.resources.swap_mb,
        m.resources.disk_gb,
        m.resources.storage,
        if m.lxc.unprivileged {
            "unprivileged,"
        } else {
            "privileged,"
        },
        m.lxc.template,
        m.boot
            .order
            .map(|o| o.to_string())
            .unwrap_or_else(|| "unset".into()),
    ));

    let groups = homelab_core::ops::backup::owner_groups(m);
    let native = is_native_stack(m);
    if native {
        s.push_str("- Runs no docker: native systemd services only.\n");
        let services = native_services(stack_dir);
        if services.is_empty() {
            s.push_str(&format!(
                "- No `service.yml` was found in `stacks/{}`, so `homelab adopt stacks/{}` \
                 and `homelab install-native` have nothing to read until one is written. \
                 Rebuild the container by the native route in Layer 2.\n",
                dir_name, dir_name
            ));
        }
        // Several units may read files with the same basename; the vault keeps
        // one file per basename per stack, so say which ones share.
        let mut claims: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (svc, unit_file) in &services {
            let mut files: Vec<String> = Vec::new();
            if let Some(uf) = unit_file {
                let need = homelab_core::native::unit_prereqs(uf);
                files.extend(need.env_files.iter().cloned());
                files.extend(need.credentials.iter().cloned());
            } else if let Some(e) = &svc.env_file {
                files.push(e.clone());
            }
            files.sort();
            files.dedup();
            for f in files {
                claims
                    .entry(vault_file(vault, &m.stack_name, &f))
                    .or_default()
                    .push(format!("{} (`{}`)", svc.unit, f));
            }
        }
        for (svc, unit_file) in &services {
            let adopt_dir = native_service_dir(stack_dir, dir_name, &svc.unit)
                .unwrap_or_else(|| format!("stacks/{}", dir_name));
            s.push_str(&format!("- Unit `{}`:\n", svc.unit));
            if let Some(note) = &svc.restore_note {
                s.push_str(&format!(
                    "  - **Restore note:** {}\n",
                    note.split_whitespace().collect::<Vec<_>>().join(" ")
                ));
            }
            s.push_str(&format!(
                "  - program `{}`, {}; update policy {}.\n",
                svc.binary,
                match &svc.release_repo {
                    Some(r) => format!(
                        "from the GitHub release `{}` (asset `{}`)",
                        r,
                        svc.asset_name()
                    ),
                    None => "no release_repo, so no recorded source for the binary".into(),
                },
                match svc.update_policy {
                    homelab_core::native::UpdatePolicy::Auto => "auto",
                    homelab_core::native::UpdatePolicy::Manual => "manual",
                    homelab_core::native::UpdatePolicy::OwnVerb => "self",
                }
            ));
            // The deploy reads `<unit>/<unit>.service` and nothing else; the
            // top-level spot is where `native_services` also looks.
            let in_unit_dir = stack_dir
                .join(&svc.unit)
                .join(format!("{}.service", svc.unit));
            let unit_rel = if in_unit_dir.exists() {
                format!("`stacks/{}/{}/{}.service`", dir_name, svc.unit, svc.unit)
            } else if unit_file.is_some() {
                format!(
                    "`stacks/{}/{}.service` (NOT where the deploy looks, which is \
                     `{}/{}.service`)",
                    dir_name, svc.unit, svc.unit, svc.unit
                )
            } else {
                "NOT FOUND".to_string()
            };
            s.push_str(&format!(
                "  - unit file {} in the repository; the container's copy is \
                 `/etc/systemd/system/{}.service`.\n",
                unit_rel, svc.unit
            ));
            if svc.stateless || svc.data_dirs.is_empty() {
                s.push_str("  - data: none, declared stateless, so no repository.\n");
            } else {
                let what = match &svc.backup_from_newest {
                    Some(glob) => format!(
                        "the newest file matching `{}` (the service's own verified copy, \
                         refused when older than {} h), not the live directory. That copy is \
                         a complete database: put it back as the live file and delete any \
                         `-wal`/`-shm` beside it (the comment above `backup_from_newest` in \
                         `{}/service.yml` names the file)",
                        glob,
                        homelab_core::ops::native::MAX_OWN_COPY_AGE_S / 3600,
                        adopt_dir
                    ),
                    None => format!(
                        "a tar of {}",
                        svc.data_dirs
                            .iter()
                            .map(|d| format!("`{}`", d))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                };
                s.push_str(&format!(
                    "  - data: repository `{}/{}-config`, archive `/{}-data.tar` holding {}.\n",
                    bcfg.restic_base, svc.unit, svc.unit, what
                ));
            }
            let mine: Vec<(&String, &Vec<String>)> = claims
                .iter()
                .filter(|(_, who)| who.iter().any(|w| w.starts_with(&format!("{} ", svc.unit))))
                .collect();
            for (vf, who) in mine {
                let path = who
                    .iter()
                    .find(|w| w.starts_with(&format!("{} ", svc.unit)))
                    .cloned()
                    .unwrap_or_default();
                if who.len() > 1 {
                    s.push_str(&format!(
                        "  - vault copy of {}: `{}`, SHARED with {}. The vault keeps one file \
                         per directory and name per stack, so it holds whichever unit's file \
                         was written last; check it against each unit before a rebuild relies \
                         on it.\n",
                        path,
                        vf,
                        who.iter()
                            .filter(|w| !w.starts_with(&format!("{} ", svc.unit)))
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                } else {
                    s.push_str(&format!("  - vault copy of {}: `{}`.\n", path, vf));
                }
            }
            s.push_str(&format!(
                "  - re-register a running unit after the daemon lost its state (needs the \
                 daemon): `homelab adopt {}`. Adoption only records a service that is \
                 already active; it never starts one.\n",
                adopt_dir
            ));
        }
        if services.is_empty() {
            // The stack names its re-registration command even with no
            // service file, so the gap above is read next to it.
            s.push_str(&format!(
                "- Re-register (needs the daemon and a service.yml): `homelab adopt stacks/{}`.\n",
                dir_name
            ));
        }
        s.push_str(
            "- Rebuild: the native route in Layer 2, then this stack's data by Layer 4 \
             (native services).\n",
        );
    } else {
        s.push_str(&format!(
            "- Apps (docker compose, started in this order): {}. Files in the container \
             under `/opt/{}/<app>/`.\n",
            m.apps.join(", "),
            m.stack_name
        ));
        s.push_str(&format!(
            "- Rebuild (needs the daemon): `homelab deploy stacks/{}`, which also refills \
             every empty data directory from its latest snapshot before the apps start. \
             Without the daemon: Layer 2.\n",
            dir_name
        ));
        if groups.is_empty() {
            s.push_str("- Data: no backed-up paths, so nothing to restore.\n");
        } else {
            s.push_str("- Data, one restic repository per owning app:\n");
            for (owner, paths) in &groups {
                s.push_str(&format!(
                    "  - `{}/{}-config`: {}\n",
                    bcfg.restic_base,
                    owner,
                    paths
                        .iter()
                        .map(|p| format!("`{}`", p))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
    }
    for st in &m.storage {
        if let Some(why) = &st.no_backup {
            let why = why.trim().trim_end_matches('.');
            s.push_str(&format!(
                "- NOT backed up, on purpose: `{}`. The stack file's reason: {}.\n",
                st.host_path, why
            ));
        } else if st.no_data {
            s.push_str(&format!(
                "- Holds nothing by declaration (`no_data`), so no repository: `{}`.\n",
                st.host_path
            ));
        }
    }
    for dm in &m.data_mounts {
        s.push_str(&format!(
            "- Host directory mounted in, never created or backed up by this suite: `{}` at \
             `{}`{}.\n",
            dm.host_path,
            dm.mount_point,
            dm.note
                .as_deref()
                .map(|n| format!(" ({})", n.trim().trim_end_matches('.')))
                .unwrap_or_default()
        ));
    }
    s.push('\n');
    s
}

/// E7: generate the disaster-recovery runbook from the local stacks dir.
/// Deliberately plain markdown with copy-pasteable commands: this document
/// must be useful when the TUI, the host daemon, or the whole host is down.
/// Returns the number of stacks included.
pub fn generate_runbook(stacks_dir: &Path, out_path: &str) -> Result<usize, String> {
    use homelab_core::ops::backup::{BackupCfg, RESTIC_CACHE_DIR};
    // fix-100: a stack made to be destroyed in the same sitting is not
    // something a rebuild should bring back.
    let stacks = declared_stacks(stacks_dir);
    let bcfg = BackupCfg::default();
    let su = homelab_core::ops::selfupdate::SelfUpdateCfg::default();
    let state = runbook_state_dir(&bcfg);
    let vault = format!("{}/secrets", state);
    let base = bcfg.restic_base.clone();
    let pw = bcfg.password_file.clone();
    // Where host.toml lives when HOMELAB_CONFIG does not say otherwise:
    // `load_config` in host/src/main.rs, and the one path outside the state
    // directory that `backup_host_meta` snapshots.
    let host_toml = "/etc/homelab/host.toml";
    let port = "8443"; // `load_config`: listen defaults to 0.0.0.0:8443
    let (remote, folder) =
        rclone_parts(&base).unwrap_or_else(|| ("<remote>".into(), "<folder>".into()));
    let client_host = crate::repo_config::load(stacks_dir)
        .ok()
        .flatten()
        .and_then(|(_, c)| c.host);
    let no_touch = homelab_core::safety::DEFAULT_NO_TOUCH
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(", ");

    // Parse every stack once; the fleet-wide facts are read off the set.
    let mut parsed: Vec<(
        String,
        PathBuf,
        Option<homelab_core::manifest::StackManifest>,
    )> = Vec::new();
    for (name, path) in &stacks {
        let raw = std::fs::read_to_string(path.join("lxc-compose.yml"))
            .map_err(|e| format!("{}: {}", name, e))?;
        let m = serde_yaml::from_str::<homelab_core::manifest::StackManifest>(&raw).ok();
        parsed.push((name.clone(), path.clone(), m));
    }
    let manifests: Vec<&homelab_core::manifest::StackManifest> =
        parsed.iter().filter_map(|(_, _, m)| m.as_ref()).collect();
    let mut networks: Vec<String> = manifests
        .iter()
        .map(|m| {
            format!(
                "`{}`{} (gateway `{}`)",
                m.network.bridge,
                m.network
                    .vlan
                    .map(|v| format!(" VLAN {}", v))
                    .unwrap_or_default(),
                m.network.gateway
            )
        })
        .collect();
    networks.sort();
    networks.dedup();
    let mut templates: Vec<String> = manifests
        .iter()
        .map(|m| {
            format!(
                "`{}` ({})",
                m.lxc.template,
                if m.lxc.unprivileged {
                    "unprivileged"
                } else {
                    "privileged"
                }
            )
        })
        .collect();
    templates.sort();
    templates.dedup();
    // Pools: the first path component of every data mount, with the stacks
    // that mount something from it.
    let mut pools: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for m in &manifests {
        for dm in &m.data_mounts {
            if let Some(first) = dm.host_path.trim_start_matches('/').split('/').next()
                && !first.is_empty()
            {
                let e = pools.entry(first.to_string()).or_default();
                if !e.contains(&m.stack_name) {
                    e.push(m.stack_name.clone());
                }
            }
        }
    }
    let pools_line = if pools.is_empty() {
        "none of the stack files mount a host directory from a pool".to_string()
    } else {
        pools
            .iter()
            .map(|(p, who)| format!("`{}` (mounted by {})", p, who.join(", ")))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut boot: Vec<(u16, u16, String)> = manifests
        .iter()
        .map(|m| {
            (
                m.boot.order.unwrap_or(u16::MAX),
                m.vmid,
                m.stack_name.clone(),
            )
        })
        .collect();
    boot.sort();

    let mut doc = String::new();
    doc.push_str("# Disaster-recovery runbook\n\n");
    doc.push_str(
        "*Generated by `homelab runbook` from the stack files under `stacks/`, \
         `config/client.toml` and the code named in each section. Do not edit by hand: \
         regenerate after changing a stack file or the backup code, and a test fails \
         until you do.*\n\n",
    );
    doc.push_str(
        "Written for the worst case: the TUI is unavailable, the `homelab-host` daemon is \
         down, and the Proxmox host may be a fresh install. Every step is a shell command \
         run as root on the Proxmox host unless it says otherwise. A step that uses the \
         `homelab` client says *needs the daemon*, and has a shell equivalent beside it.\n\n\
         Placeholders: `<vmid>`, `<stack>`, `<app>` and `<unit>` are filled in from the \
         Stacks section near the end.\n\n",
    );
    // Kenny, Phase 8 (2026-09-27): diagrams where they make things clearer.
    // Which layer to open depends on what is still standing; the chart is
    // the same decision the layer headings below walk through in prose.
    doc.push_str(
        "Which layer to open depends on what still works:\n\n\
         ```mermaid\n\
         flowchart TD\n\
         \x20   start([Something is down]) --> host{Proxmox host<br/>still running?}\n\
         \x20   host -- no --> full[Full-host rebuild order<br/>at the end of this document]\n\
         \x20   host -- yes --> daemon{homelab-host<br/>daemon answers?}\n\
         \x20   daemon -- no --> l1[Layer 1<br/>Recover the daemon]\n\
         \x20   l1 --> meta{Its state and<br/>secrets present?}\n\
         \x20   meta -- no --> l3[Layer 3<br/>Restore host-meta]\n\
         \x20   meta -- yes --> stack\n\
         \x20   l3 --> stack\n\
         \x20   daemon -- yes --> stack{A stack's container<br/>missing or broken?}\n\
         \x20   stack -- yes --> l2[Layer 2<br/>Rebuild the stack]\n\
         \x20   stack -- no --> data{Only its data<br/>lost or damaged?}\n\
         \x20   l2 --> data\n\
         \x20   data -- yes --> l4[Layer 4<br/>Restore the data]\n\
         \x20   data -- no --> l5[Layer 5<br/>ZFS replicas, if a<br/>replicated pool is affected]\n\
         ```\n\n",
    );

    // ── Layer 0 ──
    doc.push_str("## Layer 0: What runs where\n\n");
    doc.push_str(&format!(
        "- **The daemon.** `homelab-host`, a systemd service on the Proxmox host. Program \
         `{cur}`; the one before the last self-update is kept at `{prev}`. Configuration \
         `{toml}` (the `HOMELAB_CONFIG` variable overrides the path). State directory \
         `{state}` (the `state_dir` setting overrides it). Listens on port {port} over TLS.\n\
         - **Clients** reach the daemon at {client}. The address and the certificate pin are \
         in `config/client.toml` in the repository; each machine's token is `HOMELAB_TOKEN` \
         in `~/.config/homelab/env`.\n\
         - **The state directory** holds `state.json` (what is deployed where), `repo/` (a git \
         history of every file each deploy sent), `secrets/` (the vault, below), \
         `tls-cert.pem` and `tls-key.pem` (the daemon's certificate), `journal.jsonl` and \
         `incidents/` (operation records) and `restic-cache/`.\n\
         - **The vault** `{vault}`: `restic.pw` (the one password for every repository), \
         `<stack>/<app>.env` for each compose app that has an `.env`, and \
         `<stack>/<dir>/<file>` for each env or credential file a native unit reads, where \
         `<dir>` is the directory that file sits in (two units may both read a `token.env`).\n\
         - **Backups** are restic repositories behind `{base}`. The name is \
         `<owner>-config`: one per owning app for a compose stack, one per unit for a native \
         stack, `host-meta-config` for the daemon's own state, and `<name>-config` for each \
         `[[device_backups]]` entry in `{toml}`. Every repository opens with the same \
         password file `{pw}`. (`BackupCfg::default()` in core/src/ops/backup.rs; \
         `restic_base` and `restic_password_file` in `{toml}` override both.)\n\
         - **A second copy** of every repository, when `second_copy_dataset` is set in \
         `{toml}` (fix-96): each night after the backups, `restic copy` writes \
         `<owner>-config` into a local repository of the same name on that ZFS dataset \
         (`HDD4TB/restic` mounts at `/HDD4TB/restic`), with the same password file and the \
         same retention, and the ZFS replication carries it to its replica pool. When Google \
         Drive is unreachable or damaged, use it in place of `{base}` in every command below: \
         `RESTIC_REPOSITORY=/HDD4TB/restic/<owner>-config`. One repository per night is \
         checked with `restic check` on both copies (core/src/ops/secondcopy.rs).\n\
         - **App data** lives on the host under `/appdata/<stack>/<app>-config` and is \
         bind-mounted into the container at the same path, so a container can be rebuilt \
         without touching it.\n\
         - **ZFS pools** referenced by the stack files: {pools}. Replication jobs are the \
         `[[zfs_jobs]]` entries in `{toml}` (Layer 5).\n\
         - **Guests this suite never touches**, whatever a stack file says: vmid {no_touch} \
         (`DEFAULT_NO_TOUCH` in core/src/safety.rs; a `no_touch` list in `{toml}` can only \
         add to it). They come back from Proxmox's own backups, not from this runbook.\n\n",
        cur = su.current,
        prev = su.prev,
        toml = host_toml,
        state = state,
        port = port,
        client = client_host
            .as_deref()
            .map(|h| format!("`{}`", h))
            .unwrap_or_else(|| {
                "the address in `config/client.toml` (not found beside this stacks directory)"
                    .into()
            }),
        vault = vault,
        base = base,
        pw = pw,
        pools = pools_line,
        no_touch = no_touch,
    ));

    // ── Layer 1 ──
    doc.push_str("## Layer 1: Recover the host daemon\n\n");
    doc.push_str(&format!(
        "Is it running, and if not, why:\n\n```sh\n\
         systemctl status {svc}\n\
         journalctl -u {svc} -n 50 --no-pager\n\
         curl -sk https://127.0.0.1:{port}/api/health     # answers: ok\n\
         {cur} --version        # the installed version (/api/version takes the token)\n\
         ```\n\n\
         The daemon refuses to start, and says so in the journal, when `{toml}` does not \
         parse as TOML, when it is not a valid host config, or when the token is shorter \
         than 16 characters. A key it does not know is a `WARNING` line, not a refusal.\n\n\
         **A self-update that never came up.** A self-update copies the running program to \
         `{prev}`, installs the new one, and writes `{marker}`; the new program deletes the \
         marker once it has answered its first authenticated request. A marker that is still \
         there means no client has had an answer from the new program yet; when `homelab ping` \
         cannot get one either, put the previous program back:\n\n```sh\n\
         ls -l {cur} {prev} {marker}\n\
         {prev} --selfcheck                 # prints its version when it can run\n\
         install -m 755 {prev} {cur}\n\
         rm -f {marker}\n\
         systemctl restart {svc}\n\
         ```\n\n\
         **No usable program at all.** Fetch the released one on a workstation with an \
         authenticated `gh` (the release carries `homelab-host` and `SHA256SUMS`), check it, \
         copy it over, and install it on the host:\n\n```sh\n\
         # workstation\n\
         gh release download --repo {repo} --pattern homelab-host --pattern SHA256SUMS\n\
         sha256sum -c --ignore-missing SHA256SUMS\n\
         scp homelab-host root@<proxmox-host>:{cur}.new\n\
         # Proxmox host\n\
         {cur}.new --selfcheck && install -m 755 {cur}.new {cur}\n\
         systemctl restart {svc}\n\
         ```\n\n\
         To build it instead, `make host-binary` in the repository builds it against \
         Debian 12 in docker and leaves it at `target-debian/release/homelab-host`.\n\n\
         {units}\
         **The certificate pin.** The daemon's certificate is `{state}/tls-cert.pem` with \
         `{state}/tls-key.pem`; when either is missing, empty or unreadable at start it makes a \
         new pair, each file written whole (fix-128) \
         (host/src/tls.rs). The client trusts one certificate: the fingerprint built into \
         the client, taken from `pin` in `config/client.toml` when it was compiled. It refuses \
         any other, on a first connection too, and it also refuses when that `pin` or the \
         copy a machine keeps in `~/.config/homelab/pin` disagrees with it. Compare:\n\n```sh\n\
         journalctl -u {svc} | grep 'TLS fingerprint' | tail -1\n\
         openssl x509 -in {state}/tls-cert.pem -noout -fingerprint -sha256\n\
         ```\n\n\
         with `pin` in `config/client.toml` on a workstation. If they differ because the pair \
         was regenerated, every installed client refuses the daemon until one of two things \
         happens. Either restore both files from `host-meta-config` (Layer 3) and restart: \
         the clients work again unchanged. Or accept the new certificate: write the new \
         fingerprint into `pin` in `config/client.toml`, commit it, and put a client built \
         from that tree on every machine (`make install`, or cut a release and run \
         `homelab self-install` on each, which needs `gh` and not the daemon), then delete \
         `~/.config/homelab/pin` on each. A client never falls back to trusting the \
         certificate it sees.\n\n\
         **The token** is `token` in `{toml}` (or `HOMELAB_TOKEN` in the daemon's \
         environment). A new token means updating `HOMELAB_TOKEN` in \
         `~/.config/homelab/env` on every client machine.\n\n",
        svc = su.service,
        units = runbook_host_units(),
        port = port,
        toml = host_toml,
        cur = su.current,
        prev = su.prev,
        marker = su.marker,
        repo = crate::release::REPO,
        state = state,
    ));

    // ── Layer 2 ──
    doc.push_str("## Layer 2: Recover a stack without the daemon\n\n");
    doc.push_str(&format!(
        "Every stack is one LXC container. Start it and look:\n\n```sh\n\
         pct list\n\
         pct start <vmid>\n\
         ```\n\n\
         **A compose stack.** The deploy puts each app's files in `/opt/<stack>/<app>/` \
         inside the container and starts the apps in the order the stack file lists them. \
         By hand:\n\n```sh\n\
         pct exec <vmid> -- sh -c 'cd /opt/<stack>/<app> && docker compose up -d'\n\
         pct exec <vmid> -- sh -c 'cd /opt/<stack>/<app> && docker compose ps'\n\
         ```\n\n\
         If the files are gone from the container, the last deployed copy of every file is \
         in the daemon's git history at `{state}/repo/stacks/<stack>/`, one commit per \
         deploy. A file under `rootfs/` belongs at the same absolute path in the container \
         (`rootfs/etc/x` goes to `/etc/x`); every other file goes to `/opt/<stack>/`. The \
         app's `.env` is not in that history; when the app has one, its copy is in the vault:\n\n```sh\n\
         git -C {state}/repo log --oneline -- stacks/<stack>\n\
         pct exec <vmid> -- mkdir -p /opt/<stack>/<app>\n\
         pct push <vmid> {state}/repo/stacks/<stack>/<app>/docker-compose.yml /opt/<stack>/<app>/docker-compose.yml\n\
         pct push <vmid> {vault}/<stack>/<app>.env /opt/<stack>/<app>/.env --perms 600\n\
         ```\n\n\
         When the daemon is up, `homelab deploy stacks/<stack>` does all of this from the \
         repository, and recreates the container first when it is missing.\n\n\
         **A native stack** runs systemd units and no docker. By hand:\n\n```sh\n\
         pct exec <vmid> -- systemctl status <unit>\n\
         pct exec <vmid> -- journalctl -u <unit> -n 50 --no-pager\n\
         ```\n\n\
         A unit starts only when its program, its account and the files named by \
         `EnvironmentFile=` and `LoadCredential=` exist. The deploy prepares them in this \
         order before it starts anything; by hand it is the same order:\n\n```sh\n\
         pct push <vmid> {state}/repo/stacks/<stack>/<unit>/<unit>.service /etc/systemd/system/<unit>.service\n\
         pct exec <vmid> -- useradd --system --no-create-home --shell /usr/sbin/nologin <user>   # User= in the unit\n\
         pct push <vmid> {vault}/<stack>/<dir>/<file> <path-the-unit-reads> --perms 600\n\
         pct push <vmid> ./<asset> <program-path> --perms 755   # the verified release binary, see below\n\
         pct exec <vmid> -- systemctl daemon-reload\n\
         pct exec <vmid> -- systemctl enable --now <unit>\n\
         ```\n\n\
         The program comes from the unit's GitHub release (named per unit in the Stacks \
         section). On a workstation: `gh release download --repo <release_repo> --pattern \
         <asset> --pattern SHA256SUMS`, then `sha256sum -c --ignore-missing SHA256SUMS`, then \
         copy it to the host.\n\n\
         When the daemon is up, `homelab deploy stacks/<stack>` rebuilds a native stack too: \
         it creates the container, writes each `<unit>/<unit>.service`, creates the unit's \
         account, places the program from the unit's release where none exists (it never \
         replaces one), puts back env and credential files from the vault, and starts a unit \
         only when all of that is present. A unit that is not running and whose data \
         directories are empty gets its newest snapshot unpacked back first (fix-146); a \
         unit archived from its own copy (`backup_from_newest`) is then left stopped \
         until that copy is put in place as the live file. A unit left unstarted is named in the output; a \
         missing program is installed with `homelab install-native stacks/<stack>/<unit>` (or \
         `stacks/<stack>` for the unit whose `service.yml` sits at the top).\n\n",
        state = state,
        vault = vault,
    ));

    // ── Layer 3 ──
    doc.push_str("## Layer 3: Restore the daemon's own state (host-meta)\n\n");
    // fix-111 (host-meta-gaps, 2026-09-27): the extras are the list the backup
    // itself uses, so the two cannot drift apart again.
    let extras = homelab_core::ops::backup::HOST_META_EXTRAS
        .iter()
        .map(|p| format!("{}\n", p))
        .collect::<String>();
    doc.push_str(&format!(
        "Do this first when the host itself was lost: every later step needs the vault it \
         brings back. The repository is `{base}/host-meta-config`, written nightly and by \
         `homelab backup-host-meta` (`backup_host_meta` in core/src/ops/backup.rs). A \
         snapshot holds:\n\n```sh\n\
         {state}/secrets        # the vault, including restic.pw\n\
         {state}/state.json     # what is deployed where\n\
         {state}/tls-cert.pem   # the certificate the clients pin\n\
         {state}/tls-key.pem\n\
         {state}/repo           # the history of every deployed file\n\
         {toml}     # token, webhooks, zfs_jobs and every other setting\n\
         ```\n\n\
         and, each only when it was present on the host (`HOST_META_EXTRAS` in \
         core/src/ops/backup.rs): the SMART collector, the network bridges, Proxmox's \
         storage and job definitions, the VM configurations (Home Assistant's USB \
         passthrough), the swappiness drop-in and rclone's own configuration:\n\n```sh\n\
         {extras}\
         ```\n\n\
         Not in it: the `homelab-host` program and its unit file (Layer 1), `journal.jsonl`, \
         `incidents/` and `restic-cache/`.\n\n\
         **The password is inside the thing it opens.** `restic.pw` is in this repository, \
         and the same password opens every repository, so an offline copy of it is the one \
         thing this whole runbook cannot do without. The code keeps no second copy. The \
         offline copy is in Kenny's Bitwarden: the one statement in this document that no \
         code can confirm.\n\n\
         **Do not start the daemon before this restore.** A daemon that starts on an empty \
         host takes a host-meta snapshot of that empty state in its first night, and from \
         then on `latest` is the empty host. So pick the snapshot by its ID, never by \
         `latest`.\n\n\
         **rclone first.** restic reaches Google Drive through rclone's remote `{remote}`. \
         Its configuration is in this repository, which cannot be opened without it: on a \
         fresh host install `restic` and `rclone`, run `rclone config` to create the remote \
         `{remote}` again (or restore from the second copy, when its pool survived), \
         then:\n\n```sh\n\
         rclone lsd {remote}:{folder}          # the remote works and the folder is there\n\
         install -d -m 700 {vault}\n\
         # write the offline copy of the password to {pw}, mode 600\n\
         export RESTIC_REPOSITORY={base}/host-meta-config\n\
         export RESTIC_PASSWORD_FILE={pw}\n\
         export RESTIC_CACHE_DIR={cache}\n\
         restic snapshots                      # the newest one from before the loss: note its ID\n\
         restic ls <ID> | head -50\n\
         restic restore <ID> --target /\n\
         ```\n\n\
         The snapshot stores absolute paths, so `--target /` puts every file back where it \
         was. Only then start the daemon (Layer 1, when its program is not there yet) and do \
         Layer 1's pin check: the restored certificate keeps the fingerprint the \
         clients already pin.\n\n",
        base = base,
        state = state,
        toml = host_toml,
        extras = extras,
        remote = remote,
        folder = folder,
        vault = vault,
        pw = pw,
        cache = RESTIC_CACHE_DIR,
    ));

    // ── Layer 4 ──
    doc.push_str("## Layer 4: Restore a stack's data\n\n");
    doc.push_str(&format!(
        "Repositories are named per owning app (or per native unit), not per stack: \
         `{base}/<app>-config`. The exact names are in the Stacks section. Set once per \
         shell:\n\n```sh\n\
         export RESTIC_PASSWORD_FILE={pw}\n\
         export RESTIC_CACHE_DIR={cache}\n\
         ```\n\n\
         **A compose stack.** When the daemon is up, `homelab restore stacks/<stack> \
         [snapshot]` does the following, and by hand it is the same (`restore` in \
         core/src/ops/backup.rs). It asks for the stack name first (`--yes` for scripts) \
         and, with the stack down, copies the current data to \
         `/var/lib/homelab/pre-restore/<stack>-<unix time>/` before restic writes over it \
         (fix-64; `--no-safety-copy` skips that copy). With that copy taken it empties each \
         data directory first, so no file the snapshot lacks stays behind. `--app <app>` \
         restores one app and leaves the others running. A stack with several repositories \
         is restored to one night: the newest `run-<unix time>` tag every one of its \
         repositories has, or the night of the snapshot ID given (fix-112):\n\n```sh\n\
         pct exec <vmid> -- sh -c 'cd /opt/<stack>/<app> && docker compose down'   # every app\n\
         export RESTIC_REPOSITORY={base}/<app>-config                   # every repository of the stack\n\
         restic snapshots --tag run-<unix time>                          # the same night in each\n\
         find /appdata/<stack>/<app>-config -mindepth 1 -delete          # only after copying it aside\n\
         restic restore <ID> --target /\n\
         pct exec <vmid> -- sh -c 'cd /opt/<stack>/<app> && docker compose up -d'  # every app, in order\n\
         ```\n\n\
         The snapshots store the absolute host paths (`/appdata/<stack>/<app>-config`), so \
         `--target /` puts the files back in place with the owners they had. The code starts \
         the apps again even when the restore failed, and so should you. A rebuild does not \
         need this step: `homelab deploy` restores each data directory it finds empty from \
         that path's latest snapshot before the apps start, and when that fails it continues \
         with the directory EMPTY and prints `AUTO-RESTORE FAILED` for it.\n\n\
         **A native stack.** The nightly backup of a native unit is not a directory \
         snapshot: it streams `tar` out of the container into restic, stored as one file \
         `/<unit>-data.tar` (`backup_native` in core/src/ops/native.rs). Unpack it back \
         inside the container, so file owners stay the container's own:\n\n```sh\n\
         export RESTIC_REPOSITORY={base}/<unit>-config\n\
         restic snapshots --path /<unit>-data.tar\n\
         pct exec <vmid> -- systemctl stop <unit>\n\
         restic dump --path /<unit>-data.tar latest /<unit>-data.tar | pct exec <vmid> -- tar -xf - -C /\n\
         pct exec <vmid> -- systemctl start <unit>\n\
         ```\n\n\
         `homelab restore` refuses a native stack (gap-28): the compose route's `restic \
         restore latest --target /` would write the archive itself to `/<unit>-data.tar` on \
         the host and unpack nothing. For the same reason a rebuild's automatic restore \
         finds no snapshot for a native unit's directory and leaves it empty: its data \
         always comes back by the commands above.\n\n\
         A native unit may carry a restore note of its own; it is printed under that \
         unit in the Stacks section.\n\n",
        base = base,
        pw = pw,
        cache = RESTIC_CACHE_DIR,
    ));

    // ── Layer 5 ──
    doc.push_str("## Layer 5: ZFS replicas\n\n");
    doc.push_str(&format!(
        "The daemon replicates the datasets named in `[[zfs_jobs]]` (`source`, `target`) in \
         `{toml}` every night, and on `homelab zfs-replicate` (core/src/ops/zfs.rs). Each run \
         takes `zfs snapshot -r <source>@{prefix}YYYYMMDD-HHMM` and then sends every \
         dataset of the source on its own, never as one `-R` stream: `zfs send -I \
         <replica's newest snapshot> <new> | zfs receive -F -x mountpoint <target dataset>`, \
         or a full send when the target dataset does not exist yet or holds no snapshots at \
         all. It prunes only snapshots whose name starts with `{prefix}`: the source with \
         the configured tiers, the replica with its own longer ones, keeping whatever either \
         policy keeps (fix-85).\n\n\
         The replica keeps its own history. A snapshot destroyed on the source stays on the \
         replica until the replica's retention thins it; a dataset destroyed on the source \
         stays on the replica untouched, is never pruned, and is named in a warning every \
         night. When the replica's newest snapshot of a dataset has left the source, that \
         dataset is not sent: receiving from an older shared snapshot would roll the \
         replica back and destroy what came after it, so the job fails and names it.\n\n\
         `-x mountpoint` keeps a replica from arriving with its source's mountpoint, which \
         would put the copy at the live path (F177, a replica claiming the live path of what \
         it copies). A replica received before that change can still carry it, so check \
         before mounting anything:\n\n```sh\n\
         grep -A2 zfs_jobs {toml}\n\
         zpool import                                   # pools a fresh install can see\n\
         zfs list -r -o name,mountpoint,canmount,mounted <target>\n\
         zfs list -H -t snapshot -o name -s creation -r <source> | tail -3\n\
         ```\n\n\
         A replica is a copy for reading. Never give it a mountpoint a stack uses.\n\n\
         When a source and its target share no snapshot and the target already holds \
         snapshots, the job stops rather than re-seeding, because a re-seed destroys that \
         history. The choice is a person's: find out why the chain broke, or wipe the target \
         with `zfs destroy -r <target>` and run the job again for a fresh full send.\n\n",
        toml = host_toml,
        prefix = homelab_core::ops::zfs::SNAP_PREFIX,
    ));

    // ── Stacks ──
    doc.push_str("## Stacks\n\n");
    doc.push_str(&format!(
        "One section per directory under `stacks/`, read from its `lxc-compose.yml` and, for \
         a native stack, each unit's `service.yml` and `.service` file. Repository names \
         use the default base `{}`.\n\n",
        base
    ));
    let mut included = 0usize;
    for (name, path, m) in &parsed {
        let Some(m) = m else {
            doc.push_str(&format!(
                "### {} (LEGACY: not a v2 stack file, not deployable by this version)\n\n\
                 Recover it by hand per Layer 2, or migrate it to a v2 stack file first.\n\n",
                name
            ));
            continue;
        };
        included += 1;
        doc.push_str(&runbook_stack_section(path, name, m, &bcfg, &vault));
    }

    // A directory with a service.yml and no lxc-compose.yml is a service that
    // was adopted into a container this suite did not build and cannot
    // rebuild. `scan_local_stacks` skips it, so it is listed here instead of
    // vanishing from the one document read after a loss.
    let mut adopted_only: Vec<(String, homelab_proto::NativeServiceManifest)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(stacks_dir) {
        let mut dirs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        dirs.sort();
        for d in dirs {
            if !d.is_dir() || d.join("lxc-compose.yml").exists() {
                continue;
            }
            let name = d
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            for (svc, _) in native_services(&d) {
                adopted_only.push((name.clone(), svc));
            }
        }
    }
    if !adopted_only.is_empty() {
        doc.push_str(
            "## Adopted services without a container file

",
        );
        doc.push_str(
            "These directories hold a `service.yml` and no `lxc-compose.yml`: the service was \
             adopted into a container this suite did not build, so nothing here can rebuild \
             the container. Once adopted, its data is backed up nightly like any native unit's (Layer 4).\n\n",
        );
        for (dir_name, svc) in &adopted_only {
            doc.push_str(&format!(
                "- **{}** (vmid {}, hostname `{}`): unit `{}`, program `{}`; data {}; \
                 re-register with `homelab adopt stacks/{}` (needs the daemon).\n",
                svc.stack_name,
                svc.vmid,
                svc.hostname,
                svc.unit,
                svc.binary,
                if svc.stateless || svc.data_dirs.is_empty() {
                    "none".to_string()
                } else {
                    format!(
                        "in `{}/{}-config` as `/{}-data.tar`, a tar of {}",
                        bcfg.restic_base,
                        svc.unit,
                        svc.unit,
                        svc.data_dirs
                            .iter()
                            .map(|d| format!("`{}`", d))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                },
                dir_name
            ));
        }
        doc.push('\n');
    }

    // ── Full host ──
    doc.push_str("## Full-host rebuild order\n\n");
    doc.push_str(&format!(
        "1. Install Proxmox and recreate the networks the stack files use: {nets}. Clients \
         expect the daemon at {client}. For an unattended install, build the answer file \
         INTO the installer with `proxmox-auto-install-assistant prepare-iso <iso> \
         --fetch-from iso --answer-file answer.toml`: attached as a second CD beside the \
         stock ISO, the 2026-10-01 rehearsal's answer file set the network, hostname and \
         disk but not `root-password` or `root-ssh-keys`, which left the new host \
         unreachable (fix-212). After the install, a changed boot order only takes effect \
         on a full `qm stop` and `qm start` of a rehearsal VM, never on `qm reset`.\n\
         2. Import the ZFS pools the stack files mount from ({pools}), and any pool named in \
         `[[zfs_jobs]]`. Check replica mountpoints before anything mounts (Layer 5).\n\
         3. Install `restic` and `rclone`, recreate the rclone remote `{remote}`, write \
         `{pw}` from the offline copy, and restore `host-meta-config` (Layer 3).\n\
         4. Put back the `homelab-host` program and its unit file, start it, and check the \
         certificate fingerprint against the pin (Layer 1).\n\
         5. Restore the guests this suite never touches (vmid {no_touch}) from Proxmox's own \
         backups: `qmrestore` for a VM, `pct restore` for a container.\n\
         6. Rebuild the templates the stacks clone: {templates}. `homelab template-build \
         <vmid> <version>` builds an unprivileged one at that vmid, and \
         `homelab template-build <vmid> <version> --privileged` a privileged one.\n\
         7. Rebuild every stack (Layer 2; with the daemon, `homelab deploy stacks/<stack>`), \
         in boot order: {order}. A compose stack refills its empty data directories from \
         restic while it deploys; a native stack's data comes back by Layer 4 afterwards.\n",
        nets = if networks.is_empty() {
            "none found".to_string()
        } else {
            networks.join(", ")
        },
        client = client_host
            .as_deref()
            .map(|h| format!("`{}`", h))
            .unwrap_or_else(|| "the address in `config/client.toml`".into()),
        pools = pools_line,
        remote = remote,
        pw = pw,
        no_touch = no_touch,
        templates = if templates.is_empty() {
            "none found".to_string()
        } else {
            templates.join(", ")
        },
        order = boot
            .iter()
            .map(|(_, vmid, n)| format!("{} ({})", n, vmid))
            .collect::<Vec<_>>()
            .join(", "),
    ));
    // rule-public-docs: this repository is public; the stack files and
    // `config/client.toml` just quoted above are real fleet addresses, so
    // every one of them is replaced before the document is written — by
    // name when the address belongs to a stack, the host or a gateway, by
    // an RFC 5737 placeholder otherwise.
    let addr_map = crate::netredact::build_address_map(stacks_dir, client_host.as_deref());
    let doc = crate::netredact::redact(&doc, &addr_map);
    std::fs::write(out_path, &doc).map_err(|e| e.to_string())?;
    Ok(included)
}

// ── D11: stack export/import bundles ────────────────────────────────────────

/// A shareable single-file stack bundle: manifest + files, NEVER secrets.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Bundle {
    pub bundle_version: u32,
    pub exported_from: String,
    pub manifest: StackManifest,
    pub files: Vec<FileBlob>,
}

/// Export a stack directory to a single YAML bundle. `.env` files are
/// excluded by construction (build_spec routes them into `env`, which is
/// deliberately not part of the bundle).
pub fn export_bundle(dir: &Path, out_path: &str) -> Result<usize, String> {
    let spec = build_spec(dir)?;
    let bundle = Bundle {
        bundle_version: 1,
        exported_from: spec.manifest.stack_name.clone(),
        manifest: spec.manifest,
        files: spec.files,
    };
    let raw = serde_yaml::to_string(&bundle).map_err(|e| e.to_string())?;
    std::fs::write(out_path, &raw).map_err(|e| e.to_string())?;
    Ok(bundle.files.len())
}

/// Import a bundle as a NEW stack: substitute the old stack identity (name,
/// vmid, hostname, ip, /appdata paths, _net network) for the new one, then
/// write a normal stack directory. Secrets must be added afterwards as
/// stacks/<name>/<app>/.env — they are never in a bundle.
pub fn import_bundle(
    bundle_path: &Path,
    stacks_dir: &Path,
    new_name: &str,
    new_vmid: u16,
) -> Result<PathBuf, String> {
    let raw = std::fs::read_to_string(bundle_path).map_err(|e| e.to_string())?;
    let dest = stacks_dir.join(new_name);
    if dest.exists() {
        return Err(format!("stacks/{} already exists", new_name));
    }
    let files = imported_files(&raw, new_name, new_vmid)?;
    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    for (rel, content) in files {
        let path = dest.join(&rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, content).map_err(|e| e.to_string())?;
    }
    Ok(dest)
}

/// TUI parity round: [`import_bundle`] without the disk, so the dashboard
/// can commit the new stack through its edit transaction instead of
/// writing into its working copy. Every file of the new stack (relative to
/// `stacks/<new_name>/`, `lxc-compose.yml` first) with its content.
pub fn imported_files(
    raw: &str,
    new_name: &str,
    new_vmid: u16,
) -> Result<Vec<(String, String)>, String> {
    let bundle: Bundle = serde_yaml::from_str(raw).map_err(|e| format!("bundle parse: {}", e))?;
    if bundle.bundle_version != 1 {
        return Err(format!(
            "unsupported bundle version {}",
            bundle.bundle_version
        ));
    }
    let old = &bundle.exported_from;
    let old_vmid = bundle.manifest.vmid;
    let defaults = crate::scaffold::StackDefaults::default();
    let new_ip_host = format!("{}{}", defaults.ip_prefix, new_vmid.saturating_sub(100));

    // Manifest: identity fields + derived values + path renames.
    let mut m = bundle.manifest.clone();
    m.stack_name = new_name.to_string();
    m.vmid = new_vmid;
    m.hostname = format!("{}-app-{}", new_vmid, new_name);
    // Keep the CIDR suffix from the original ip.
    let cidr = m.network.ip.split('/').nth(1).unwrap_or("24").to_string();
    m.network.ip = format!("{}/{}", new_ip_host, cidr);
    for mount in m.storage.iter_mut() {
        mount.host_path = mount.host_path.replace(
            &format!("/appdata/{}/", old),
            &format!("/appdata/{}/", new_name),
        );
        mount.mount_point = mount.mount_point.replace(
            &format!("/appdata/{}/", old),
            &format!("/appdata/{}/", new_name),
        );
    }
    let manifest_yaml = serde_yaml::to_string(&m).map_err(|e| format!("manifest render: {}", e))?;
    let mut out = vec![("lxc-compose.yml".to_string(), manifest_yaml)];

    // Files: same substitutions inside content (D7 mechanics).
    let old_host = format!("{}-app-{}", old_vmid, old);
    for f in &bundle.files {
        if f.path.split('/').any(|p| p == ".." || p.is_empty()) || f.path.starts_with('/') {
            return Err(format!(
                "the bundle names a path outside the stack: {}",
                f.path
            ));
        }
        let content = f
            .content
            .replace(&format!("{}_net", old), &format!("{}_net", new_name))
            .replace(
                &format!("/appdata/{}/", old),
                &format!("/appdata/{}/", new_name),
            )
            .replace(&old_host, &m.hostname);
        out.push((f.path.clone(), content));
    }
    Ok(out)
}

/// TUI parity round: the export bundle's text, from the stack's files alone
/// (no latch, no download: a bundle never carries secrets or programs), and
/// how many files it holds. The dashboard offers it as a download.
pub fn bundle_text(dir: &Path) -> Result<(String, usize), String> {
    let manifest = build_manifest(dir)?;
    let files = stack_files(dir)?;
    let bundle = Bundle {
        bundle_version: 1,
        exported_from: manifest.stack_name.clone(),
        manifest,
        files,
    };
    let raw = serde_yaml::to_string(&bundle).map_err(|e| e.to_string())?;
    Ok((raw, bundle.files.len()))
}

/// Y4: every stack directory in the repository with the vmid it claims. Only
/// the client can see this — the host has the intent repo of what it actually
/// deployed, not the files sitting in front of the author.
pub fn stack_files_with_vmids(base: &str) -> Vec<(String, u16)> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(base) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let manifest = dir.join("lxc-compose.yml");
        let native = dir.join("service.yml");
        let path = if manifest.exists() {
            manifest
        } else if native.exists() {
            native
        } else {
            continue;
        };
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        // Deliberately a line scan rather than a full parse: a file too broken
        // to deserialize is exactly the one worth reporting, and refusing to
        // look at it would hide it.
        if let Some(vmid) = raw
            .lines()
            .find_map(|l| l.trim().strip_prefix("vmid:"))
            .and_then(|v| v.trim().parse::<u16>().ok())
        {
            out.push((
                format!("{}/{}", base, entry.file_name().to_string_lossy()),
                vmid,
            ));
        }
    }
    out.sort();
    out
}

/// T71: every native service a stack directory declares, with the unit file
/// that makes it exist.
///
/// A native stack keeps one `service.yml` per unit, and where it sits depends
/// on how many there are: a stack with a single service puts it at the top
/// (`stacks/almanac/service.yml`), and a stack with several gives each its
/// own directory (`stacks/kyu/kyu-runner/service.yml`). Both shapes are real
/// on this fleet, so both are read here rather than in each caller.
///
/// The unit file is returned alongside because install-native cannot do
/// anything without it, and a service whose `.service` file is missing from
/// the repository is exactly the one worth reporting: it would rebuild into a
/// container that holds the program and nothing to run it.
pub fn native_services(
    dir: &std::path::Path,
) -> Vec<(homelab_proto::NativeServiceManifest, Option<String>)> {
    let mut out = Vec::new();
    let mut paths = vec![dir.join("service.yml")];
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut subs: Vec<std::path::PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .map(|p| p.join("service.yml"))
            .filter(|p| p.exists())
            .collect();
        subs.sort();
        paths.extend(subs);
    }
    for path in paths {
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(m) = serde_yaml::from_str::<homelab_proto::NativeServiceManifest>(&raw) else {
            continue;
        };
        let unit_name = format!("{}.service", m.unit);
        let parent = path.parent().unwrap_or(dir);
        let unit_file = [parent.join(&unit_name), dir.join(&m.unit).join(&unit_name)]
            .iter()
            .find_map(|p| std::fs::read_to_string(p).ok());
        out.push((m, unit_file));
    }
    out.sort_by(|a, b| a.0.unit.cmp(&b.0.unit));
    out.dedup_by(|a, b| a.0.unit == b.0.unit);
    out
}
