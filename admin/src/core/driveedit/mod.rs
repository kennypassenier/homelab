//! feat-platform-10 (milestone follow), the edit forms: Claude drives the
//! stack settings form and its raw editor, "add an app", the firewall with
//! its rule dialog, the new-stack wizard, the host settings, the batch
//! dialog and the roll back dialog, the way `drive` drives an action's
//! dialog.
//!
//! The fields' words come from `formspec.json`'s `edit` section, the same
//! description `editforms.js` draws the browser's forms from; the checks
//! here are the browser's (`checkFields`, `ruleProblems`, `checkNewStep`,
//! `parseKey`), held equal by the cases file both suites read. The final
//! press (the commit and push, host.toml, the batch) is an `EditCall` the

mod after_set;
mod apps;
mod check_rows;
mod checks;
mod done;
mod firewall;
mod helpers;
mod keys;
mod kind;
mod latch;
mod native;
mod newstack;
mod open;
mod open_batch;
mod open_misc;
mod open_native;
mod open_stack;
mod press;
mod press_other;
mod press_plan;
mod press_preset;
mod press_sub;
mod refresh;
mod row;
mod row_host;
mod row_list;
mod settings;
mod settings_ext;
mod spec;
mod state;
mod storage;
mod tiles;

pub use self::after_set::*;
pub use self::apps::*;
pub use self::check_rows::*;
pub use self::checks::*;
pub use self::done::*;
pub use self::firewall::*;
use self::helpers::*;
pub use self::keys::*;
pub use self::kind::*;
pub use self::latch::*;
pub use self::native::*;
pub use self::newstack::*;
pub use self::open::*;
use self::open_batch::*;
use self::open_misc::*;
use self::open_native::*;
use self::open_stack::*;
pub use self::press::*;
use self::press_other::*;
use self::press_plan::*;
use self::press_preset::*;
use self::press_sub::*;
pub use self::refresh::*;
pub use self::row::*;
use self::row_host::*;
use self::row_list::*;
pub use self::settings::*;
pub use self::settings_ext::*;
pub use self::spec::*;
pub use self::state::*;
pub use self::storage::*;
pub use self::tiles::*;

use std::collections::{BTreeMap, BTreeSet};

use homelab_proto::{Scope, UiStep};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::core::actions::{ActionKind, Arg, Refusal, SELF_STACK};
use crate::core::drive::{
    self, Applied, Choice, Ctx, DriveState, Effect, Family, Field, FieldKind, FormStep, OpenForm,
    Sources, Values, fill, refused, spec,
};
use crate::core::presetedit;
