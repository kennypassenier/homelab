//! What the working copy's stacks directory holds: native releases, the
//! declared stacks for apply and drift, and a stack's files one by one.

use std::path::Path;

use super::stackfiles::no_repo;
use super::{LocalStacks, RepoFiles, StackFiles};
use crate::core::actions::Refusal;

type NativeRelease = (homelab_proto::NativeServiceManifest, String, String);
type NativeService = (homelab_proto::NativeServiceManifest, Option<String>);

impl RepoFiles {
    pub(super) fn native_release_of(
        &self,
        stack: &str,
        unit: Option<&str>,
    ) -> Result<NativeRelease, Refusal> {
        let what = format!("install-native {stack}");
        self.sync_copy(&what);
        let _held = self.hold_copy();
        if !self.present() {
            return Err(no_repo(&self.repo, what));
        }
        let dir = self.repo.join("stacks").join(stack);
        let services = homelab_client::spec::native_services(&dir);
        let (m, unit_file) = pick_native(services, stack, unit, &what)?;
        let unit_file = checked_release(&m, unit_file, stack, what)?;
        let rel = release_dir(&dir, stack, &m.unit);
        Ok((m, unit_file, rel))
    }

    pub(super) fn local_stacks_of(&self, with_specs: bool) -> Result<LocalStacks, Refusal> {
        self.sync_copy("reading the stacks");
        let _held = self.hold_copy();
        if !self.present() {
            return Err(no_repo(&self.repo, "the stacks directory".into()));
        }
        let base = self.repo.join("stacks");
        let declared = homelab_client::spec::declared_stacks(&base);
        let mut out = stack_listing(&base, &declared);
        for (name, dir) in declared {
            if with_specs {
                add_with_spec(&mut out, name, &dir);
            } else {
                add_hash_only(&mut out, name, &dir);
            }
        }
        Ok(out)
    }

    pub(super) fn stack_files_of(
        &self,
        stack: &str,
    ) -> Result<Vec<homelab_proto::FileBlob>, Refusal> {
        let _held = self.hold_copy();
        if !self.present() {
            return Err(no_repo(&self.repo, format!("the files of {stack}")));
        }
        homelab_client::spec::stack_files(&self.repo.join("stacks").join(stack)).map_err(|why| {
            Refusal::new(
                format!("the files of {stack}"),
                why,
                format!("fix stacks/{stack} in the working copy"),
            )
        })
    }
}

/// The service `unit` names, or the stack's only one.
fn pick_native(
    services: Vec<NativeService>,
    stack: &str,
    unit: Option<&str>,
    what: &str,
) -> Result<NativeService, Refusal> {
    let pick = match unit {
        Some(u) => services.into_iter().find(|(m, _)| m.unit == u),
        None if services.len() == 1 => services.into_iter().next(),
        None => {
            return Err(Refusal::new(
                what,
                format!(
                    "stacks/{stack} holds {} native services; name the one to install",
                    services.len()
                ),
                "pick the unit in the form",
            ));
        }
    };
    pick.ok_or_else(|| {
        Refusal::new(
            what,
            format!(
                "stacks/{stack} has no native service {}",
                unit.unwrap_or("(none at all)")
            ),
            "pick a unit the stack's service.yml files name",
        )
    })
}

/// The unit file, once the manifest is valid and names a release.
fn checked_release(
    m: &homelab_proto::NativeServiceManifest,
    unit_file: Option<String>,
    stack: &str,
    what: String,
) -> Result<String, Refusal> {
    homelab_core::native::validate_native(m).map_err(|p| {
        Refusal::new(
            what.clone(),
            format!("service.yml invalid: {}", p.join("; ")),
            format!("fix stacks/{stack}"),
        )
    })?;
    if m.release_repo.is_none() {
        return Err(Refusal::new(
            what,
            format!(
                "{} declares no release_repo: there is no release to install",
                m.unit
            ),
            "add release_repo to its service.yml",
        ));
    }
    unit_file.ok_or_else(|| {
        Refusal::new(
            what,
            format!(
                "no {}.service beside its service.yml: a rebuilt container would have the \
                 binary and nothing to run it",
                m.unit
            ),
            "commit the unit file next to the service.yml",
        )
    })
}

/// The directory the CLI's install-native names: the one whose service.yml
/// describes this unit.
fn release_dir(dir: &Path, stack: &str, unit: &str) -> String {
    if dir.join(unit).join("service.yml").is_file() {
        format!("stacks/{stack}/{unit}")
    } else {
        format!("stacks/{stack}")
    }
}

/// The ephemeral stacks and the directory names, before any spec is built.
fn stack_listing(base: &Path, declared: &[(String, std::path::PathBuf)]) -> LocalStacks {
    let mut out = LocalStacks {
        ephemeral: homelab_client::spec::scan_local_stacks(base)
            .into_iter()
            .map(|(n, _)| n)
            .filter(|n| !declared.iter().any(|(d, _)| d == n))
            .collect(),
        dirs: std::fs::read_dir(base)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| e.path().is_dir())
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default(),
        ..LocalStacks::default()
    };
    out.dirs.sort();
    out
}

fn add_with_spec(out: &mut LocalStacks, name: String, dir: &Path) {
    let built = homelab_client::spec::build_spec(dir).and_then(|sp| {
        homelab_core::manifest::validate(&sp).map_err(|e| e.to_string())?;
        Ok(sp)
    });
    match built {
        Ok(sp) => {
            out.hashes
                .push((name.clone(), Ok(homelab_core::manifest::intent_hash(&sp))));
            out.component_digests
                .insert(name.clone(), homelab_core::manifest::component_digests(&sp));
            out.specs.insert(name, sp);
        }
        Err(e) => out.hashes.push((name, Err(e))),
    }
}

fn add_hash_only(out: &mut LocalStacks, name: String, dir: &Path) {
    let h = homelab_client::spec::local_intent_hash(dir).map(|(h, _)| h);
    out.hashes.push((name.clone(), h));
    if let Ok((digests, _)) = homelab_client::spec::local_component_digests(dir) {
        out.component_digests.insert(name, digests);
    }
}
