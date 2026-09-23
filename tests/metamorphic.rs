// Copyright 2023 Greptime Team
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Metamorphic tests: one generated query is driven through every
//! representation the crate exposes, and all of them must agree.
//!
//! For every AST produced by the seeded generator the following oracles run:
//!
//! 1. AST -> `Display` string -> `parse` -> AST must be a structural
//!    round-trip. The comparison ignores nothing semantic: matcher order,
//!    vector matching cardinality, the `bool` modifier, `offset`/`@` and the
//!    subquery step are all preserved. (This AST carries no source spans, so
//!    there is nothing span-related to ignore; positions only exist in the
//!    lexer.)
//! 2. `prettify` may normalize whitespace, but prettifying the re-parsed AST
//!    must reproduce the first prettified output byte-for-byte (idempotent).
//! 3. `walk_expr` / `walk_expr_mut` must traverse exactly the nodes an
//!    independent recursive traversal finds, in the same pre-order.
//! 4. With the `ser` feature, the JSON serialization of the original and of
//!    the re-parsed AST must be identical, and serializing to a JSON string
//!    and reading it back must be a fixed point.
//!
//! The generator only emits ASTs whose `Display` form parses back to itself
//! (e.g. binary children that would re-associate are wrapped in parentheses,
//! set operators are materialized with `ManyToMany` exactly like `check_ast`
//! does). Single-point illegal variants (bad arity, forbidden vector
//! matching, duplicated group labels, broken matcher escapes, ...) are built
//! separately and must be rejected by `parse`/`check_ast` without serde or
//! the visitor panicking first.
//!
//! All randomness comes from a fixed-seed splitmix64; failures are shrunk by
//! descending into failing sub-expressions before being reported.

use std::fmt;
use std::sync::{Arc, Once};
use std::time::{Duration, SystemTime};

use promql_parser::label::{MatchOp, Matcher, Matchers, METRIC_NAME};
use promql_parser::parser::ast::ExtensionExpr;
use promql_parser::parser::token::{
    T_ADD, T_AVG, T_BOTTOMK, T_COUNT, T_COUNT_VALUES, T_DIV, T_EQL, T_EQLC, T_EQL_REGEX, T_GTE,
    T_GTR, T_GROUP, T_LAND, T_LOR, T_LSS, T_LTE, T_LUNLESS, T_MAX, T_MIN, T_MOD, T_MUL, T_NEQ,
    T_NEQ_REGEX, T_POW, T_QUANTILE, T_STDDEV, T_STDVAR, T_SUB, T_SUM, T_TOPK,
};
use promql_parser::parser::value::ValueType;
use promql_parser::parser::{
    parse, register_extra_functions, AggregateExpr, AtModifier, BinModifier, BinaryExpr, Call,
    Expr, Extension, Function, FunctionArgs, LabelModifier, MatrixSelector, NumberLiteral, Offset,
    ParenExpr, StringLiteral, SubqueryExpr, TokenType as _, UnaryExpr, VectorMatchCardinality,
    VectorMatchFillValues, VectorSelector,
};
use promql_parser::parser::{Prettier, TokenType};
use promql_parser::util::visitor::{walk_expr_mut, ExprVisitorMut};
use promql_parser::util::{walk_expr, ExprVisitor};
