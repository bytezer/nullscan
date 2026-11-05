//! TypeScript rule implementations, one module per rule.
//!
//! Every module exposes `pub fn run(&FileContext) -> Vec<Diagnostic>`. The metadata,
//! scope, and execution order live in `crate::rule::RULES`; adding a module here without
//! registering it there has no effect.

mod query;

pub mod cognitive_complexity;
pub mod eqeqeq;
pub mod no_collapsible_if;
pub mod no_console;
pub mod no_debugger;
pub mod no_duplicate_string;
pub mod no_empty_catch;
pub mod no_explicit_any;
pub mod no_identical_conditions;
pub mod no_redundant_boolean;
pub mod no_var;
pub mod prefer_immediate_return;

pub(crate) use query::query_rule;
