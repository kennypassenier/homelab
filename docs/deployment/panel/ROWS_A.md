# Register rows for panel fixes (branch panel-fixes-host-state)

Rows in the REGISTER.md fix-row format, to be moved into the register when
the branch lands.

| fix-50 | **An unreadable state.json loaded as an empty fleet** (expert panel, state-load-error-empty-fleet, predicted from code). `StateStore::load` answered every read error with `HostState::default()` (`core/src/state.rs:245-248`); an EACCES, EIO or EMFILE on the read meant the next save erased every managed stack | `CoreError::NotFound` separates absence from unreadability; `RealExecutor::read_file` returns it only for `ErrorKind::NotFound`, and `load` refuses any other read error with a hard error instead of an empty fleet. Test `fix_50_an_unreadable_state_file_is_an_error_not_an_empty_fleet` failed first (load returned an empty HostState) | doing: release |
