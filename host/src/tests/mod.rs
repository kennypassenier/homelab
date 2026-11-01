//! Host tests, by part. They exercise the whole host through the crate root;
//! the helpers they share live in `support`.

mod commands;
mod config;
mod config_save;
mod doctor;
mod history;
mod night;
mod notices;
mod notify_kyu;
mod questions;
mod reads;
mod refusals;
mod session;
mod stack_commit;
mod supervision;
mod support;
mod tokens;
mod ui_relay;
