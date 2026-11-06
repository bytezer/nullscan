//! TypeScript rule implementations, one module per rule.
//!
//! Every module exposes `pub fn run(&FileContext) -> Vec<Diagnostic>`. The metadata,
//! scope, and execution order live in `crate::rule::RULES`; adding a module here without
//! registering it there has no effect.

mod branches;
mod query;

pub mod cognitive_complexity;
pub mod consistent_return;
pub mod eqeqeq;
pub mod hardcoded_secrets;
pub mod max_switch_cases;
pub mod no_all_duplicated_branches;
pub mod no_array_constructor;
pub mod no_collapsible_if;
pub mod no_console;
pub mod no_constant_condition;
pub mod no_debugger;
pub mod no_duplicate_imports;
pub mod no_duplicate_string;
pub mod no_duplicated_branches;
pub mod no_element_overwrite;
pub mod no_empty_catch;
pub mod no_eval;
pub mod no_explicit_any;
pub mod no_fallthrough;
pub mod no_gratuitous_expressions;
pub mod no_identical_conditions;
pub mod no_identical_expressions;
pub mod no_ignored_return;
pub mod no_implied_eval;
pub mod no_nested_switch;
pub mod no_nested_template_literals;
pub mod no_new_func;
pub mod no_new_wrappers;
pub mod no_one_iteration_loop;
pub mod no_redundant_boolean;
pub mod no_redundant_jump;
pub mod no_redeclare;
pub mod no_same_line_conditional;
pub mod no_script_url;
pub mod no_self_assign;
pub mod no_shadow;
pub mod no_small_switch;
pub mod no_unreachable;
pub mod no_unused_collection;
pub mod no_unused_vars;
pub mod no_use_before_define;
pub mod no_useless_catch;
pub mod no_var;
pub mod non_existent_operator;
pub mod prefer_const;
pub mod prefer_immediate_return;
pub mod prefer_single_boolean_return;
pub mod use_isnan;

pub(crate) use query::{
    named_children_without_comments, node_text, query_rule, report_at, report_at_range,
    static_string_text,
};
