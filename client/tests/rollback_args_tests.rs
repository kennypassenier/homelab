//! fix-114 (native-rollback-copies-deleted, 2026-09-27):
//! `homelab rollback-native <stack>/<unit>`, or `<stack>` for a stack with
//! one unit.

use homelab_client::stack_and_unit;

#[test]
fn a_unit_is_named_after_the_stack() {
    assert_eq!(
        stack_and_unit("kyu/kyu-runner"),
        ("kyu".to_string(), Some("kyu-runner".to_string()))
    );
    assert_eq!(stack_and_unit("almanac"), ("almanac".to_string(), None));
    assert_eq!(
        stack_and_unit("stacks/kyu/kyu-runner"),
        ("kyu".to_string(), Some("kyu-runner".to_string())),
        "the path to the stack directory works too"
    );
    assert_eq!(
        stack_and_unit("stacks/almanac/"),
        ("almanac".to_string(), None)
    );
}
