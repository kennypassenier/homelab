//! Small helpers the form machine shares.

use super::*;

pub(super) fn plain() -> Applied {
    Applied {
        effect: Effect::None,
        held: None,
    }
}

pub(super) fn held_for(form: &OpenForm, what: &str, errors: &BTreeMap<String, String>) -> Refusal {
    let all: Vec<Field> = form
        .desc
        .fields()
        .cloned()
        .chain(
            form.edit
                .as_ref()
                .and_then(|e| e.sub.as_ref())
                .map(|s| s.fields.clone())
                .unwrap_or_default(),
        )
        .collect();
    let list: Vec<String> = errors
        .iter()
        .map(|(name, e)| {
            let id = all
                .iter()
                .find(|f| &f.name == name)
                .map_or(name.as_str(), |f| f.id.as_str());
            format!("{id}: {e}")
        })
        .collect();
    Refusal::new(
        format!("ui press {what}"),
        format!("the form holds: {}", list.join("; ")),
        "correct the named fields and press again",
    )
}

pub(super) fn held_msg(what: &str, why: &str) -> Applied {
    Applied {
        effect: Effect::None,
        held: Some(Refusal::new(
            format!("ui press {what}"),
            format!("the form holds: {why}"),
            "change something first, or homelab ui close",
        )),
    }
}

pub(super) fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| x.as_str().map(str::to_string))
        .collect()
}

pub(super) fn edit(form: &mut OpenForm) -> &mut EditState {
    form.edit.as_mut().expect("an edit form has its edit state")
}
