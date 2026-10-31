//! Choosing one night across several repositories for a restore.

use super::snapshot_parse::SnapRun;

/// fix-112: `(repository, snapshot id)` per repository, and a note to log
/// when the choice could not keep to one night.
pub type NightChoice = (Vec<(String, String)>, Option<String>);

type Listings<'a> = &'a [(String, Vec<SnapRun>)];

/// fix-112: the snapshot of each repository that a restore of `wanted` takes.
///
/// `latest` is the newest night present in EVERY repository. When no night
/// is (history from before the tags), each repository's own newest is taken
/// and the note says so. An explicit id brings the other repositories of its
/// night along, and is refused when one of them does not have that night.
pub fn resolve_night(
    listings: &[(String, Vec<SnapRun>)],
    wanted: &str,
) -> Result<NightChoice, String> {
    for (owner, snaps) in listings {
        if snaps.is_empty() {
            return Err(format!(
                "repository for '{}' holds no snapshots at all — nothing has been stopped",
                owner
            ));
        }
    }
    if wanted == "latest" {
        return resolve_latest(listings);
    }
    resolve_explicit(listings, wanted)
}

/// The snapshot each repository holds for night `run`, or the owner of the
/// first repository that has none.
fn of_night(listings: Listings<'_>, run: u64) -> Result<Vec<(String, String)>, String> {
    listings
        .iter()
        .map(|(owner, snaps)| {
            snaps
                .iter()
                .filter(|s| s.run == Some(run))
                .max_by_key(|s| s.time)
                .map(|s| (owner.clone(), s.id.clone()))
                .ok_or_else(|| owner.clone())
        })
        .collect()
}

/// The newest night that every repository holds a tagged snapshot of.
fn newest_common_night(listings: Listings<'_>) -> Option<u64> {
    let mut common: Option<std::collections::BTreeSet<u64>> = None;
    for (_, snaps) in listings {
        let runs: std::collections::BTreeSet<u64> = snaps.iter().filter_map(|s| s.run).collect();
        common = Some(match common {
            None => runs,
            Some(c) => c.intersection(&runs).cloned().collect(),
        });
    }
    common.and_then(|c| c.into_iter().max())
}

fn resolve_latest(listings: Listings<'_>) -> Result<NightChoice, String> {
    if let Some(run) = newest_common_night(listings) {
        return of_night(listings, run).map(|ids| (ids, None));
    }
    let ids = listings
        .iter()
        .map(|(owner, snaps)| {
            let newest = snaps.iter().max_by_key(|s| s.time).expect("checked above");
            (owner.clone(), newest.id.clone())
        })
        .collect();
    Ok((
        ids,
        Some(
            "no night is present in every repository (snapshots from before the night \
             tags), so each repository's own newest is restored — they may be from \
             different nights"
                .into(),
        ),
    ))
}

fn resolve_explicit(listings: Listings<'_>, wanted: &str) -> Result<NightChoice, String> {
    let found = listings.iter().find_map(|(owner, snaps)| {
        snaps
            .iter()
            .find(|s| s.id.starts_with(wanted) || s.short_id.starts_with(wanted))
            .map(|s| (owner, s))
    });
    let Some((owner, snap)) = found else {
        return Err(format!(
            "snapshot '{}' is in none of this stack's repositories — nothing has been stopped",
            wanted
        ));
    };
    let Some(run) = snap.run else {
        return Err(format!(
            "snapshot '{}' of '{}' carries no night tag (it was taken before snapshots carried one), so the \
             other repositories cannot be matched to it — restore that app alone with `--app \
             {}`, or restore 'latest'. Nothing has been stopped",
            wanted, owner, owner
        ));
    };
    of_night(listings, run)
        .map(|ids| (ids, None))
        .map_err(|missing| {
            format!(
                "snapshot '{}' is from the night run-{}, which the repository for '{}' does not \
                 have — restoring it would pair two nights. Nothing has been stopped",
                wanted, run, missing
            )
        })
}
