//! fix-199: `formspec.json`'s `forms` list is what a Live view tab reports
//! back as what it can draw a dialog for (`TabCaps.forms`). It is a hand
//! list, like `pages` and `stack_tabs` already are, so this test holds it
//! equal to the two enums it is meant to mirror — a kind added to
//! `EditKind` or `ActionKind` and forgotten here would otherwise silently
//! under-report what this version knows, which would make a driven step
//! refused that this version actually supports.

use std::collections::BTreeSet;

use homelab_admin::core::actions::ActionKind;
use homelab_admin::core::drive::spec;
use homelab_admin::core::driveedit::EditKind;

#[test]
fn fix_199_formspec_forms_equals_every_edit_and_action_slug() {
    let want: BTreeSet<&str> = EditKind::ALL
        .iter()
        .map(|k| k.slug())
        .chain(ActionKind::ALL.iter().map(|k| k.slug()))
        .collect();
    let got: BTreeSet<&str> = spec().forms.iter().map(String::as_str).collect();
    let missing: Vec<&&str> = want.difference(&got).collect();
    let extra: Vec<&&str> = got.difference(&want).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "formspec.json's \"forms\" drifted from EditKind/ActionKind: \
         missing {missing:?}, extra {extra:?}"
    );
}
