#![feature(rustc_private)]

extern crate rustc_ast;
extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_span;

use std::{
    collections::{HashMap, HashSet},
    env,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    process,
};

use rustc_ast::visit as ast_visit;
use rustc_driver::{Callbacks, Compilation};
use rustc_hir::{
    def::Res,
    def_id::{DefId, LocalDefId},
    intravisit::{self, Visitor},
    Arm, BinOpKind, Block, BodyOwnerKind, Expr, ExprKind, HirId, ItemKind, LetStmt, Pat,
    PatExprKind, PatKind, QPath, StmtKind,
};
use rustc_interface::interface::Compiler;
use rustc_middle::ty::{AliasTyKind, GenericArgKind, Ty, TyCtxt, TyKind};
use rustc_span::{hygiene::ExpnKind, source_map::SourceMap, Span, Symbol};
use serde::Serialize;
use syn::{
    parse::Parser,
    visit::{self, Visit as SynVisit},
    Expr as SynExpr, ExprCall as SynExprCall, ExprMethodCall as SynExprMethodCall,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct RustcDiagnostic {
    rule_id: &'static str,
    rule_name: &'static str,
    file: String,
    line: usize,
    column: usize,
    message: String,
}

#[derive(Default)]
struct PolkadotCallbacks {
    diagnostics: Vec<RustcDiagnostic>,
    continue_compilation: bool,
    enabled_rules: HashSet<String>,
    parsed_weight_attributes: Vec<ParsedWeightAttribute>,
    parsed_item_attributes: Vec<ParsedItemAttributes>,
    parsed_transactional_attributes: Vec<ParsedTransactionalAttribute>,
}

#[derive(Clone)]
struct ParsedWeightAttribute {
    function_name: String,
    file: String,
    line: usize,
    attribute_start_line: usize,
    attribute_end_line: usize,
    attribute_source: String,
    deprecated: bool,
    has_call_index: bool,
}

#[derive(Clone)]
struct ParsedItemAttributes {
    item_name: String,
    file: String,
    line: usize,
    storage: bool,
    unbounded: bool,
    event: bool,
    internal_numeric_layout: bool,
}

#[derive(Clone)]
struct ParsedTransactionalAttribute {
    function_name: String,
    file: String,
    line: usize,
}

struct EventFieldCandidate {
    span: Span,
    macro_consumed_event_marker: bool,
}

impl Callbacks for PolkadotCallbacks {
    fn after_crate_root_parsing(
        &mut self,
        compiler: &Compiler,
        krate: &mut rustc_ast::ast::Crate,
    ) -> Compilation {
        let mut visitor = ParsedWeightAttributeVisitor {
            source_map: compiler.sess.source_map(),
            parsed_attributes: &mut self.parsed_weight_attributes,
            parsed_item_attributes: &mut self.parsed_item_attributes,
            parsed_transactional_attributes: &mut self.parsed_transactional_attributes,
        };
        ast_visit::walk_crate(&mut visitor, krate);
        Compilation::Continue
    }

    fn after_analysis<'tcx>(
        &mut self,
        _compiler: &rustc_interface::interface::Compiler,
        tcx: TyCtxt<'tcx>,
    ) -> Compilation {
        if !crate_matches_source_filters(tcx, &output_file_filters()) {
            return if self.continue_compilation {
                Compilation::Continue
            } else {
                Compilation::Stop
            };
        }
        if self.rule_enabled("SEC013") {
            report_unbounded_storage_aliases(
                tcx,
                tcx.sess.source_map(),
                &self.parsed_item_attributes,
                &mut self.diagnostics,
            );
        }
        if self.rule_enabled("SEC014") {
            report_identity_hashers_on_common_keys(
                tcx,
                tcx.sess.source_map(),
                &self.parsed_item_attributes,
                &mut self.diagnostics,
            );
        }
        if self.rule_enabled("SEC004") {
            report_raw_weight_arithmetic(
                tcx,
                tcx.sess.source_map(),
                &self.parsed_weight_attributes,
                &mut self.diagnostics,
            );
        }
        if self.rule_enabled("SEC005") {
            report_expensive_weight_calculation(
                tcx,
                tcx.sess.source_map(),
                &self.parsed_weight_attributes,
                &mut self.diagnostics,
            );
        }
        if self.rule_enabled("SEC010") {
            report_missing_transactional_hooks(
                tcx,
                tcx.sess.source_map(),
                &self.parsed_transactional_attributes,
                &mut self.diagnostics,
            );
        }
        if self.rule_enabled("SEC016") {
            report_missing_storage_version_checks(
                tcx,
                tcx.sess.source_map(),
                &mut self.diagnostics,
            );
        }
        if self.rule_enabled("VAL003") {
            report_storage_write_before_validation(
                tcx,
                tcx.sess.source_map(),
                &mut self.diagnostics,
            );
        }
        if self.rule_enabled("SEM010") {
            report_xor_as_exponentiation(tcx, tcx.sess.source_map(), &mut self.diagnostics);
        }
        if self.rule_enabled("SEM009") {
            report_redundant_storage_contains_key(
                tcx,
                tcx.sess.source_map(),
                &mut self.diagnostics,
            );
        }
        if self.rule_enabled("SEM006") {
            report_dbweight_missing_pov(tcx, tcx.sess.source_map(), &mut self.diagnostics);
        }
        if self.rule_enabled("SEM016") {
            report_missing_authorize_call_in_create_authorized_transaction(
                tcx,
                tcx.sess.source_map(),
                &mut self.diagnostics,
            );
        }
        if self.rule_enabled("SEC006") {
            report_unchecked_repatriate_reserved(tcx, tcx.sess.source_map(), &mut self.diagnostics);
        }
        if self.rule_enabled("SEC007") {
            report_discarded_results(tcx, tcx.sess.source_map(), &mut self.diagnostics);
        }
        let sec017_tainted_bodies = self
            .rule_enabled("SEC017")
            .then(|| {
                unbounded_tainted_reachable_function_bodies(
                    tcx,
                    tcx.sess.source_map(),
                    &self.parsed_weight_attributes,
                )
            })
            .unwrap_or_default();
        let reachable_entry_point_bodies = (self.rule_enabled("SEC002")
            || self.rule_enabled("SEC008")
            || self.rule_enabled("SEC011")
            || self.rule_enabled("SEC012")
            || self.rule_enabled("SEC015"))
        .then(|| reachable_local_function_bodies(tcx, false))
        .unwrap_or_default();
        if self.rule_enabled("SEC017") {
            report_vec_event_fields(
                tcx,
                tcx.sess.source_map(),
                &self.parsed_item_attributes,
                &self.parsed_weight_attributes,
                &sec017_tainted_bodies,
                &mut self.diagnostics,
            );
        }
        let reachable_fallible_entry_point_bodies = self
            .rule_enabled("SEC009")
            .then(|| reachable_local_function_bodies(tcx, true))
            .unwrap_or_default();
        let sec003_tainted_bodies = self
            .rule_enabled("SEC003")
            .then(|| tainted_reachable_function_bodies(tcx))
            .unwrap_or_default();
        if self.rule_enabled("SEC002") {
            report_reachable_debug_assertions(
                tcx,
                tcx.sess.source_map(),
                &reachable_entry_point_bodies,
                &mut self.diagnostics,
            );
        }
        if self.rule_enabled("SEC008") {
            report_reachable_panic_calls(
                tcx,
                tcx.sess.source_map(),
                &reachable_entry_point_bodies,
                &mut self.diagnostics,
            );
        }
        if self.rule_enabled("SEC009") {
            report_reachable_raw_arithmetic(
                tcx,
                tcx.sess.source_map(),
                &reachable_fallible_entry_point_bodies,
                &mut self.diagnostics,
            );
        }
        if self.rule_enabled("SEC011") {
            report_reachable_storage_iteration(
                tcx,
                tcx.sess.source_map(),
                &reachable_entry_point_bodies,
                &mut self.diagnostics,
            );
        }
        if self.rule_enabled("SEC012") {
            report_reachable_clear_prefix(
                tcx,
                tcx.sess.source_map(),
                &reachable_entry_point_bodies,
                &mut self.diagnostics,
            );
        }
        if self.rule_enabled("SEC015") {
            report_reachable_dispatch_bypass_filters(
                tcx,
                tcx.sess.source_map(),
                &reachable_entry_point_bodies,
                &mut self.diagnostics,
            );
        }

        for def_id in tcx.hir_body_owners() {
            let body_owner_kind = tcx.hir_body_owner_kind(def_id);
            if !matches!(body_owner_kind, BodyOwnerKind::Fn | BodyOwnerKind::Closure) {
                continue;
            }
            let typeck = tcx.typeck(def_id);
            let body = tcx.hir_body_owned_by(def_id);

            if matches!(body_owner_kind, BodyOwnerKind::Fn) && self.rule_enabled("SEC018") {
                report_missing_weight_for_unbounded_inputs(
                    tcx,
                    def_id,
                    body,
                    tcx.sess.source_map(),
                    &self.parsed_weight_attributes,
                    &mut self.diagnostics,
                );
            }

            if matches!(body_owner_kind, BodyOwnerKind::Fn) && self.rule_enabled("SEC001") {
                report_unbounded_public_vec_inputs(
                    tcx,
                    def_id,
                    body,
                    tcx.sess.source_map(),
                    &self.parsed_weight_attributes,
                    &mut self.diagnostics,
                );
            }

            if !matches!(body_owner_kind, BodyOwnerKind::Fn) {
                continue;
            }
            if let Some(tainted_bindings) = sec003_tainted_bodies.get(&def_id) {
                let mut decode_visitor = Sec003Visitor {
                    source_map: tcx.sess.source_map(),
                    tcx,
                    typeck,
                    diagnostics: &mut self.diagnostics,
                    tainted_bindings: tainted_bindings.clone(),
                };
                decode_visitor.visit_body(body);
            }
        }

        if self.continue_compilation {
            Compilation::Continue
        } else {
            Compilation::Stop
        }
    }
}

fn crate_matches_source_filters(tcx: TyCtxt<'_>, filters: &[String]) -> bool {
    if filters.is_empty() {
        return true;
    }

    let source_map = tcx.sess.source_map();
    let location = source_map.lookup_char_pos(tcx.hir_root_module().spans.inner_span.lo());
    let crate_source = location.file.name.prefer_local().to_string();
    source_matches_filters(&crate_source, filters)
}

fn source_matches_filters(source: &str, filters: &[String]) -> bool {
    if filters.is_empty() {
        return true;
    }

    let manifest_root = env::var_os("POLKADOT_LINTER_DRIVER_MANIFEST_ROOT").map(PathBuf::from);
    filters.iter().any(|filter| {
        source.contains(filter)
            || filter.ends_with(source)
            || manifest_root.as_ref().is_some_and(|root| {
                Path::new(source).is_relative() && root.join(source).starts_with(Path::new(filter))
            })
    })
}

fn reachable_local_function_bodies(
    tcx: TyCtxt<'_>,
    fallible_entry_points_only: bool,
) -> Vec<LocalDefId> {
    let mut pending = tcx
        .hir_body_owners()
        .filter(|def_id| {
            matches!(tcx.hir_body_owner_kind(*def_id), BodyOwnerKind::Fn)
                && is_reachable_entry_point(tcx, *def_id)
                && (!fallible_entry_points_only || returns_fallible(tcx, *def_id))
        })
        .collect::<Vec<_>>();
    let mut visited = HashSet::new();
    let mut reachable = Vec::new();

    while let Some(def_id) = pending.pop() {
        if !visited.insert(def_id) || !matches!(tcx.hir_body_owner_kind(def_id), BodyOwnerKind::Fn)
        {
            continue;
        }

        let Some(body) = tcx.hir_maybe_body_owned_by(def_id) else {
            continue;
        };
        let typeck = tcx.typeck(def_id);
        let mut callee_visitor = LocalCalleeVisitor {
            typeck,
            callees: HashSet::new(),
            function_bindings: HashMap::new(),
        };
        callee_visitor.visit_body(body);
        pending.extend(callee_visitor.callees);
        reachable.push(def_id);
    }

    reachable
}

fn tainted_reachable_function_bodies<'tcx>(
    tcx: TyCtxt<'tcx>,
) -> HashMap<LocalDefId, HashSet<HirId>> {
    tainted_reachable_function_bodies_with_input_filter(tcx, false, None).unbounded_bindings
}

#[derive(Default)]
struct TaintedBodyEvidence {
    unbounded_bindings: HashMap<LocalDefId, HashSet<HirId>>,
    weight_accounted_bindings: HashMap<LocalDefId, HashSet<HirId>>,
}

fn unbounded_tainted_reachable_function_bodies<'tcx>(
    tcx: TyCtxt<'tcx>,
    source_map: &SourceMap,
    parsed_weight_attributes: &[ParsedWeightAttribute],
) -> TaintedBodyEvidence {
    tainted_reachable_function_bodies_with_input_filter(
        tcx,
        true,
        Some((source_map, parsed_weight_attributes)),
    )
}

fn tainted_reachable_function_bodies_with_input_filter<'tcx>(
    tcx: TyCtxt<'tcx>,
    unbounded_inputs_only: bool,
    parsed_dispatchables: Option<(&SourceMap, &[ParsedWeightAttribute])>,
) -> TaintedBodyEvidence {
    let mut tainted_parameter_indices = tcx
        .hir_body_owners()
        .filter(|def_id| {
            matches!(tcx.hir_body_owner_kind(*def_id), BodyOwnerKind::Fn)
                && (is_reachable_entry_point(tcx, *def_id)
                    || (unbounded_inputs_only
                        && parsed_dispatchables.is_some_and(|(source_map, attributes)| {
                            is_frame_dispatchable(tcx, *def_id, source_map, attributes)
                        })))
        })
        .filter_map(|def_id| {
            tcx.hir_maybe_body_owned_by(def_id).map(|body| {
                let typeck = tcx.typeck(def_id);
                let parameter_indices: HashSet<usize> = body
                    .params
                    .iter()
                    .enumerate()
                    .filter_map(|(index, param)| {
                        (!unbounded_inputs_only
                            || type_contains_unbounded_event_vec(
                                tcx,
                                typeck.node_type(param.pat.hir_id),
                            ))
                        .then_some(index)
                    })
                    .collect();
                (def_id, parameter_indices)
            })
        })
        .collect::<HashMap<_, _>>();
    let mut pending = tainted_parameter_indices
        .keys()
        .copied()
        .collect::<Vec<_>>();
    let mut unaccounted_parameter_indices = tainted_parameter_indices
        .iter()
        .map(|(def_id, parameter_indices)| {
            let accounted_indices = parsed_dispatchables
                .map(|(source_map, attributes)| {
                    weight_accounted_dispatchable_parameter_indices(
                        tcx, *def_id, source_map, attributes,
                    )
                })
                .unwrap_or_default();
            (
                *def_id,
                parameter_indices
                    .difference(&accounted_indices)
                    .copied()
                    .collect::<HashSet<_>>(),
            )
        })
        .collect::<HashMap<_, _>>();

    while let Some(def_id) = pending.pop() {
        let Some(body) = tcx.hir_maybe_body_owned_by(def_id) else {
            continue;
        };
        let Some(parameter_indices) = tainted_parameter_indices.get(&def_id) else {
            continue;
        };
        let tainted_bindings = parameter_indices
            .iter()
            .filter_map(|index| body.params.get(*index))
            .flat_map(|param| pattern_binding_ids(param.pat))
            .collect::<HashSet<_>>();
        let weight_accounted_bindings = parameter_indices
            .iter()
            .filter(|index| {
                !unaccounted_parameter_indices
                    .get(&def_id)
                    .is_some_and(|unaccounted| unaccounted.contains(index))
            })
            .filter_map(|index| body.params.get(*index))
            .flat_map(|param| pattern_binding_ids(param.pat))
            .collect::<HashSet<_>>();
        let mut visitor = TaintedLocalCallVisitor {
            typeck: tcx.typeck(def_id),
            tainted_bindings,
            weight_accounted_bindings,
            tainted_callee_parameters: Vec::new(),
            unaccounted_callee_parameters: Vec::new(),
            function_bindings: HashMap::new(),
        };
        visitor.visit_body(body);

        for (callee, parameter_index) in visitor.tainted_callee_parameters {
            let Some(callee_body) = tcx.hir_maybe_body_owned_by(callee) else {
                continue;
            };
            if parameter_index >= callee_body.params.len() {
                continue;
            }
            if tainted_parameter_indices
                .entry(callee)
                .or_default()
                .insert(parameter_index)
            {
                pending.push(callee);
            }
        }
        for (callee, parameter_index) in visitor.unaccounted_callee_parameters {
            if unaccounted_parameter_indices
                .entry(callee)
                .or_default()
                .insert(parameter_index)
            {
                pending.push(callee);
            }
        }
    }

    let mut unbounded_bindings = HashMap::new();
    let mut weight_accounted_bindings = HashMap::new();
    for (def_id, parameter_indices) in tainted_parameter_indices {
        let Some(body) = tcx.hir_maybe_body_owned_by(def_id) else {
            continue;
        };
        let unaccounted_indices = unaccounted_parameter_indices
            .get(&def_id)
            .cloned()
            .unwrap_or_default();
        unbounded_bindings.insert(
            def_id,
            parameter_indices
                .iter()
                .filter_map(|index| body.params.get(*index))
                .flat_map(|param| pattern_binding_ids(param.pat))
                .collect(),
        );
        weight_accounted_bindings.insert(
            def_id,
            parameter_indices
                .iter()
                .filter(|index| !unaccounted_indices.contains(index))
                .filter_map(|index| body.params.get(*index))
                .flat_map(|param| pattern_binding_ids(param.pat))
                .collect(),
        );
    }
    TaintedBodyEvidence {
        unbounded_bindings,
        weight_accounted_bindings,
    }
}

fn report_reachable_debug_assertions<'tcx>(
    tcx: TyCtxt<'tcx>,
    source_map: &SourceMap,
    reachable_bodies: &[LocalDefId],
    diagnostics: &mut Vec<RustcDiagnostic>,
) {
    let mut reported_lines = HashSet::new();

    for def_id in reachable_bodies {
        if let Some(body) = tcx.hir_maybe_body_owned_by(*def_id) {
            let mut debug_assert_visitor = Sec002Visitor {
                source_map,
                diagnostics,
                reported_lines: &mut reported_lines,
            };
            debug_assert_visitor.visit_body(body);
            report_source_debug_assertions(
                tcx,
                *def_id,
                source_map,
                diagnostics,
                &mut reported_lines,
            );
        }
    }
}

fn report_source_debug_assertions(
    tcx: TyCtxt<'_>,
    def_id: LocalDefId,
    source_map: &SourceMap,
    diagnostics: &mut Vec<RustcDiagnostic>,
    reported_lines: &mut HashSet<(String, usize)>,
) {
    let definition_span = tcx
        .def_ident_span(def_id.to_def_id())
        .unwrap_or_else(|| tcx.def_span(def_id));
    let location = source_map.lookup_char_pos(definition_span.lo());
    let source_path = location.file.name.prefer_local().to_string();
    let Ok(source) = std::fs::read_to_string(&source_path) else {
        return;
    };
    let function_name = tcx.item_name(def_id.to_def_id()).as_str().to_string();
    let Some((function_start, start_line)) =
        source_function_start(&source, location.line, &function_name)
    else {
        return;
    };

    for (line, column) in source_debug_assert_locations(&source[function_start..], start_line) {
        if reported_lines.insert((source_path.clone(), line)) {
            diagnostics.push(RustcDiagnostic {
                rule_id: "SEC002",
                rule_name: "debug-assert-in-production",
                file: source_path.clone(),
                line,
                column,
                message: "`debug_assert!` expands into a debug-only panic path".to_string(),
            });
        }
    }
}

fn source_debug_assert_locations(source: &str, start_line: usize) -> Vec<(usize, usize)> {
    #[derive(Clone, Copy)]
    enum ScanState {
        Code,
        LineComment,
        BlockComment { depth: usize },
        String { escaped: bool },
        Character { escaped: bool },
        RawString { hashes: usize },
    }

    let mut state = ScanState::Code;
    let mut locations = Vec::new();
    let mut brace_depth = 0usize;
    let mut entered_body = false;
    let mut line = start_line;
    let mut column = 1usize;
    let mut chars = source.chars().peekable();

    while let Some(ch) = chars.next() {
        let next = chars.peek().copied();
        match state {
            ScanState::Code => {
                if ch == '/' && next == Some('/') {
                    chars.next();
                    column += 1;
                    state = ScanState::LineComment;
                } else if ch == '/' && next == Some('*') {
                    chars.next();
                    column += 1;
                    state = ScanState::BlockComment { depth: 1 };
                } else if ch == 'r' && raw_string_hash_count(&chars).is_some() {
                    let hashes =
                        raw_string_hash_count(&chars).expect("raw string prefix was found");
                    for _ in 0..=hashes {
                        chars.next();
                        column += 1;
                    }
                    state = ScanState::RawString { hashes };
                } else if ch == '"' {
                    state = ScanState::String { escaped: false };
                } else if ch == '\'' {
                    state = ScanState::Character { escaped: false };
                } else if ch == '{' {
                    entered_body = true;
                    brace_depth += 1;
                } else if ch == '}' && entered_body {
                    brace_depth = brace_depth.saturating_sub(1);
                    if brace_depth == 0 {
                        break;
                    }
                } else if entered_body && ch == 'd' {
                    let mut candidate = String::from(ch);
                    for _ in 0..12 {
                        let Some(next_ch) = chars.peek().copied() else {
                            break;
                        };
                        candidate.push(next_ch);
                        if candidate == "debug_assert!" {
                            locations.push((line, column));
                            break;
                        }
                        if !"debug_assert!".starts_with(&candidate) {
                            break;
                        }
                        chars.next();
                        column += 1;
                    }
                }
            }
            ScanState::LineComment => {
                if ch == '\n' {
                    state = ScanState::Code;
                }
            }
            ScanState::BlockComment { mut depth } => {
                if ch == '/' && next == Some('*') {
                    chars.next();
                    column += 1;
                    depth += 1;
                    state = ScanState::BlockComment { depth };
                } else if ch == '*' && next == Some('/') {
                    chars.next();
                    column += 1;
                    depth -= 1;
                    state = (depth == 0)
                        .then_some(ScanState::Code)
                        .unwrap_or(ScanState::BlockComment { depth });
                }
            }
            ScanState::String { escaped } => {
                if escaped {
                    state = ScanState::String { escaped: false };
                } else if ch == '\\' {
                    state = ScanState::String { escaped: true };
                } else if ch == '"' {
                    state = ScanState::Code;
                }
            }
            ScanState::Character { escaped } => {
                if escaped {
                    state = ScanState::Character { escaped: false };
                } else if ch == '\\' {
                    state = ScanState::Character { escaped: true };
                } else if ch == '\'' {
                    state = ScanState::Code;
                }
            }
            ScanState::RawString { hashes } => {
                if ch == '"' && raw_string_terminator_matches(&chars, hashes) {
                    for _ in 0..hashes {
                        chars.next();
                        column += 1;
                    }
                    state = ScanState::Code;
                }
            }
        }

        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }

    locations
}

fn raw_string_hash_count(chars: &std::iter::Peekable<std::str::Chars<'_>>) -> Option<usize> {
    let mut preview = chars.clone();
    let mut hashes = 0;
    while preview.peek() == Some(&'#') {
        preview.next();
        hashes += 1;
    }
    (preview.next() == Some('"')).then_some(hashes)
}

fn raw_string_terminator_matches(
    chars: &std::iter::Peekable<std::str::Chars<'_>>,
    hashes: usize,
) -> bool {
    let mut preview = chars.clone();
    (0..hashes).all(|_| preview.next() == Some('#'))
}

fn source_function_start(
    source: &str,
    definition_line: usize,
    function_name: &str,
) -> Option<(usize, usize)> {
    let mut offset = 0;
    for (index, line) in source.split_inclusive('\n').enumerate() {
        let line_number = index + 1;
        if line_number >= definition_line && line.contains(&format!("fn {function_name}")) {
            return Some((offset, line_number));
        }
        offset += line.len();
    }
    None
}

fn report_reachable_panic_calls<'tcx>(
    tcx: TyCtxt<'tcx>,
    source_map: &SourceMap,
    reachable_bodies: &[LocalDefId],
    diagnostics: &mut Vec<RustcDiagnostic>,
) {
    let mut reported_lines = HashSet::new();

    for def_id in reachable_bodies {
        let Some(body) = tcx.hir_maybe_body_owned_by(*def_id) else {
            continue;
        };
        let mut panic_visitor = Sec008Visitor {
            source_map,
            tcx,
            typeck: tcx.typeck(*def_id),
            diagnostics,
            reported_lines: &mut reported_lines,
            known_unwrappable_bindings: HashSet::new(),
        };
        panic_visitor.visit_body(body);
    }
}

fn report_reachable_dispatch_bypass_filters<'tcx>(
    tcx: TyCtxt<'tcx>,
    source_map: &SourceMap,
    reachable_bodies: &[LocalDefId],
    diagnostics: &mut Vec<RustcDiagnostic>,
) {
    let mut reported_lines = HashSet::new();

    for def_id in reachable_bodies {
        let Some(body) = tcx.hir_maybe_body_owned_by(*def_id) else {
            continue;
        };
        let mut visitor = Sec015Visitor {
            source_map,
            tcx,
            typeck: tcx.typeck(*def_id),
            diagnostics,
            reported_lines: &mut reported_lines,
            root_guard_depth: 0,
        };
        visitor.visit_body(body);
    }
}

fn report_reachable_raw_arithmetic<'tcx>(
    tcx: TyCtxt<'tcx>,
    source_map: &SourceMap,
    reachable_bodies: &[LocalDefId],
    diagnostics: &mut Vec<RustcDiagnostic>,
) {
    let mut reported_lines = HashSet::new();

    for def_id in reachable_bodies {
        let Some(body) = tcx.hir_maybe_body_owned_by(*def_id) else {
            continue;
        };
        let mut visitor = Sec009Visitor {
            source_map,
            tcx,
            typeck: tcx.typeck(*def_id),
            diagnostics,
            reported_lines: &mut reported_lines,
            non_underflow_pairs: HashSet::new(),
            nonzero_bindings: HashSet::new(),
        };
        visitor.visit_body(body);
    }
}

fn report_reachable_storage_iteration<'tcx>(
    tcx: TyCtxt<'tcx>,
    source_map: &SourceMap,
    reachable_bodies: &[LocalDefId],
    diagnostics: &mut Vec<RustcDiagnostic>,
) {
    for def_id in reachable_bodies {
        let Some(body) = tcx.hir_maybe_body_owned_by(*def_id) else {
            continue;
        };
        let mut visitor = Sec011Visitor {
            source_map,
            tcx,
            typeck: tcx.typeck(*def_id),
            diagnostics,
            statically_bounded_bindings: HashSet::new(),
        };
        visitor.visit_body(body);
    }
}

fn report_reachable_clear_prefix<'tcx>(
    tcx: TyCtxt<'tcx>,
    source_map: &SourceMap,
    reachable_bodies: &[LocalDefId],
    diagnostics: &mut Vec<RustcDiagnostic>,
) {
    for def_id in reachable_bodies {
        let Some(body) = tcx.hir_maybe_body_owned_by(*def_id) else {
            continue;
        };
        let mut visitor = Sec012Visitor {
            source_map,
            tcx,
            typeck: tcx.typeck(*def_id),
            diagnostics,
            unbounded_limit_bindings: HashSet::new(),
        };
        visitor.visit_body(body);
    }
}

fn report_missing_transactional_hooks<'tcx>(
    tcx: TyCtxt<'tcx>,
    source_map: &SourceMap,
    parsed_transactional_attributes: &[ParsedTransactionalAttribute],
    diagnostics: &mut Vec<RustcDiagnostic>,
) {
    for def_id in tcx.hir_body_owners() {
        if !matches!(tcx.hir_body_owner_kind(def_id), BodyOwnerKind::Fn)
            || !is_frame_lifecycle_hook(tcx, def_id)
            || has_transactional_hook_attribute(
                tcx,
                def_id,
                source_map,
                parsed_transactional_attributes,
            )
        {
            continue;
        }
        let Some(body) = tcx.hir_maybe_body_owned_by(def_id) else {
            continue;
        };
        let mut visitor = Sec010Visitor {
            tcx,
            typeck: tcx.typeck(def_id),
            storage_writes: 0,
            has_fallible_path_after_write: false,
        };
        visitor.visit_body(body);
        if visitor.storage_writes < 2 || !visitor.has_fallible_path_after_write {
            continue;
        }
        let (file, line, column) = span_location(source_map, tcx.def_span(def_id));
        diagnostics.push(RustcDiagnostic {
            rule_id: "SEC010",
            rule_name: "missing-transactional-in-hook",
            file,
            line,
            column,
            message: format!(
                "FRAME hook has {} resolved storage writes before a fallible path without a transactional storage layer",
                visitor.storage_writes
            ),
        });
    }
}

fn report_missing_storage_version_checks<'tcx>(
    tcx: TyCtxt<'tcx>,
    source_map: &SourceMap,
    diagnostics: &mut Vec<RustcDiagnostic>,
) {
    for def_id in tcx.hir_body_owners() {
        if !matches!(tcx.hir_body_owner_kind(def_id), BodyOwnerKind::Fn)
            || !is_frame_runtime_upgrade(tcx, def_id)
        {
            continue;
        }
        let Some(body) = tcx.hir_maybe_body_owned_by(def_id) else {
            continue;
        };
        let mut visitor = Sec016Visitor {
            tcx,
            typeck: tcx.typeck(def_id),
            has_resolved_storage_version_check: false,
            has_resolved_storage_write: false,
        };
        visitor.visit_body(body);
        if !visitor.has_resolved_storage_write || visitor.has_resolved_storage_version_check {
            continue;
        }
        let (file, line, column) = span_location(source_map, tcx.def_span(def_id));
        diagnostics.push(RustcDiagnostic {
            rule_id: "SEC016",
            rule_name: "missing-storage-version-check-in-runtime-upgrade",
            file,
            line,
            column,
            message: "Resolved FRAME runtime upgrade writes storage without a StorageVersion check"
                .to_string(),
        });
    }
}

fn report_storage_write_before_validation<'tcx>(
    tcx: TyCtxt<'tcx>,
    source_map: &SourceMap,
    diagnostics: &mut Vec<RustcDiagnostic>,
) {
    for def_id in tcx.hir_body_owners() {
        if !matches!(tcx.hir_body_owner_kind(def_id), BodyOwnerKind::Fn) {
            continue;
        }
        let Some(body) = tcx.hir_maybe_body_owned_by(def_id) else {
            continue;
        };
        let mut visitor = Val003Visitor {
            tcx,
            typeck: tcx.typeck(def_id),
            source_map,
            diagnostics,
            first_storage_write: None,
            reported: false,
        };
        visitor.visit_body(body);
    }
}

fn has_transactional_hook_attribute(
    tcx: TyCtxt<'_>,
    def_id: LocalDefId,
    source_map: &SourceMap,
    parsed_transactional_attributes: &[ParsedTransactionalAttribute],
) -> bool {
    if has_hir_attr(tcx, tcx.local_def_id_to_hir_id(def_id), &["transactional"]) {
        return true;
    }
    let definition_span = tcx
        .def_ident_span(def_id.to_def_id())
        .unwrap_or_else(|| tcx.def_span(def_id));
    let location = source_map.lookup_char_pos(definition_span.lo());
    let function_name = tcx.item_name(def_id.to_def_id());
    parsed_transactional_attributes.iter().any(|attribute| {
        attribute.function_name == function_name.as_str()
            && attribute.file == location.file.name.prefer_local().to_string()
            && attribute.line == location.line
    })
}

struct Sec010Visitor<'a, 'tcx> {
    tcx: TyCtxt<'tcx>,
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    storage_writes: usize,
    has_fallible_path_after_write: bool,
}

struct Sec016Visitor<'a, 'tcx> {
    tcx: TyCtxt<'tcx>,
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    has_resolved_storage_version_check: bool,
    has_resolved_storage_write: bool,
}

struct Val003Visitor<'a, 'tcx> {
    tcx: TyCtxt<'tcx>,
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    source_map: &'a SourceMap,
    diagnostics: &'a mut Vec<RustcDiagnostic>,
    first_storage_write: Option<Span>,
    reported: bool,
}

impl<'tcx> Visitor<'tcx> for Val003Visitor<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if self.reported {
            return;
        }

        if let ExprKind::Call(callee, _) = expr.kind {
            if is_frame_storage_write_call(self.tcx, self.typeck, callee) {
                self.first_storage_write.get_or_insert(expr.span);
            }
        }

        if self.first_storage_write.is_some()
            && matches!(
                expr.kind,
                ExprKind::Match(_, _, rustc_hir::MatchSource::TryDesugar(_))
            )
        {
            let (file, line, column) = span_location(
                self.source_map,
                self.first_storage_write.expect("checked above"),
            );
            self.diagnostics.push(RustcDiagnostic {
                rule_id: "VAL003",
                rule_name: "storage-write-before-validation",
                file,
                line,
                column,
                message: "Resolved FRAME storage write occurs before a fallible validation edge"
                    .to_string(),
            });
            self.reported = true;
            return;
        }

        if matches!(expr.kind, ExprKind::Closure(_)) {
            return;
        }
        intravisit::walk_expr(self, expr);
    }
}

impl<'tcx> Visitor<'tcx> for Sec016Visitor<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if is_resolved_storage_version_check(self.tcx, self.typeck, expr) {
            self.has_resolved_storage_version_check = true;
        }
        if let ExprKind::Call(callee, _) = expr.kind {
            if is_frame_migration_storage_write_call(self.tcx, self.typeck, callee) {
                self.has_resolved_storage_write = true;
            }
        }
        if matches!(expr.kind, ExprKind::Closure(_)) {
            return;
        }
        intravisit::walk_expr(self, expr);
    }
}

impl<'tcx> Visitor<'tcx> for Sec010Visitor<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if let ExprKind::Call(callee, args) = expr.kind {
            if is_frame_storage_layer_call(self.tcx, self.typeck, callee) {
                // The closure executes in a storage transaction; evidence inside it cannot leave
                // partial state if its Result is an error.
                return;
            }
            if let ExprKind::Closure(closure) = callee.kind {
                for argument in args {
                    self.visit_expr(argument);
                }
                self.visit_body(self.tcx.hir_body(closure.body));
                return;
            }
            if is_frame_storage_write_call(self.tcx, self.typeck, callee) {
                self.storage_writes += 1;
            }
        }

        if matches!(
            expr.kind,
            ExprKind::Match(_, _, rustc_hir::MatchSource::TryDesugar(_))
        ) && self.storage_writes > 0
        {
            self.has_fallible_path_after_write = true;
        }

        if matches!(expr.kind, ExprKind::Closure(_)) {
            return;
        }
        intravisit::walk_expr(self, expr);
    }
}

fn is_frame_storage_write_call<'tcx>(
    tcx: TyCtxt<'tcx>,
    typeck: &rustc_middle::ty::TypeckResults<'tcx>,
    callee: &'tcx Expr<'tcx>,
) -> bool {
    associated_call_name(callee).is_some_and(|name| {
        matches!(
            name.as_str(),
            "put" | "insert" | "mutate" | "remove" | "kill" | "set"
        )
    }) && is_frame_storage_associated_call(tcx, typeck, callee)
}

fn is_frame_migration_storage_write_call<'tcx>(
    tcx: TyCtxt<'tcx>,
    typeck: &rustc_middle::ty::TypeckResults<'tcx>,
    callee: &'tcx Expr<'tcx>,
) -> bool {
    let is_write_name = associated_call_name(callee).is_some_and(|name| {
        matches!(
            name.as_str(),
            "put"
                | "insert"
                | "mutate"
                | "mutate_extant"
                | "remove"
                | "kill"
                | "set"
                | "append"
                | "take"
                | "clear_prefix"
                | "translate"
        )
    });
    if !is_write_name {
        return false;
    }

    is_frame_storage_associated_call(tcx, typeck, callee)
        || typeck
            .type_dependent_def_id(callee.hir_id)
            .is_some_and(|def_id| {
                matches_frame_storage_method_owner_path(&tcx.def_path_str(def_id))
            })
}

fn is_resolved_storage_version_check(
    tcx: TyCtxt<'_>,
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    expr: &Expr<'_>,
) -> bool {
    let def_id = match expr.kind {
        ExprKind::MethodCall(_, _, _, _) => typeck.type_dependent_def_id(expr.hir_id),
        ExprKind::Call(callee, _) => {
            let ExprKind::Path(qpath) = callee.kind else {
                return false;
            };
            typeck.qpath_res(&qpath, callee.hir_id).opt_def_id()
        }
        _ => return false,
    };
    def_id.is_some_and(|def_id| {
        let path = tcx.def_path_str(def_id);
        is_frame_support_path(&path)
            && (path.contains("::StorageVersion::")
                || path.ends_with("::on_chain_storage_version")
                || path.ends_with("::in_code_storage_version")
                || path.ends_with("::current_storage_version"))
    })
}

fn is_frame_storage_layer_call<'tcx>(
    tcx: TyCtxt<'tcx>,
    typeck: &rustc_middle::ty::TypeckResults<'tcx>,
    callee: &'tcx Expr<'tcx>,
) -> bool {
    matches!(callee.kind, ExprKind::Path(qpath) if matches!(typeck.qpath_res(&qpath, callee.hir_id), Res::Def(_, def_id) if {
        let path = tcx.def_path_str(def_id);
        is_frame_support_path(&path) && path.ends_with("::with_storage_layer")
    }))
}

fn report_unchecked_repatriate_reserved<'tcx>(
    tcx: TyCtxt<'tcx>,
    source_map: &SourceMap,
    diagnostics: &mut Vec<RustcDiagnostic>,
) {
    for def_id in tcx.hir_body_owners() {
        let Some(body) = tcx.hir_maybe_body_owned_by(def_id) else {
            continue;
        };
        let mut visitor = Sec006Visitor {
            tcx,
            typeck: tcx.typeck(def_id),
            unchecked_results: HashMap::new(),
            diagnostics,
        };
        visitor.visit_body(body);
        for span in visitor.unchecked_results.into_values() {
            let (file, line, column) = span_location(source_map, span);
            visitor.diagnostics.push(RustcDiagnostic {
                rule_id: "SEC006",
                rule_name: "unchecked-repatriate-reserved",
                file,
                line,
                column,
                message: "Resolved repatriate_reserved remaining balance is never checked"
                    .to_string(),
            });
        }
    }
}

fn report_discarded_results<'tcx>(
    tcx: TyCtxt<'tcx>,
    source_map: &SourceMap,
    diagnostics: &mut Vec<RustcDiagnostic>,
) {
    let mut reported_lines = HashSet::new();

    for def_id in tcx.hir_body_owners() {
        if !matches!(tcx.hir_body_owner_kind(def_id), BodyOwnerKind::Fn) {
            continue;
        }
        let Some(body) = tcx.hir_maybe_body_owned_by(def_id) else {
            continue;
        };
        let mut visitor = Sec007Visitor {
            source_map,
            tcx,
            typeck: tcx.typeck(def_id),
            diagnostics,
            reported_lines: &mut reported_lines,
        };
        visitor.visit_body(body);
    }
}

struct Sec006Visitor<'a, 'tcx> {
    tcx: TyCtxt<'tcx>,
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    unchecked_results: HashMap<HirId, Span>,
    diagnostics: &'a mut Vec<RustcDiagnostic>,
}

impl<'tcx> Visitor<'tcx> for Sec006Visitor<'_, 'tcx> {
    fn visit_stmt(&mut self, stmt: &'tcx rustc_hir::Stmt<'tcx>) {
        if matches!(stmt.kind, StmtKind::Semi(expr) | StmtKind::Expr(expr) if is_repatriate_reserved_call(self.tcx, self.typeck, expr))
        {
            let (file, line, column) = span_location(self.tcx.sess.source_map(), stmt.span);
            self.diagnostics.push(RustcDiagnostic {
                rule_id: "SEC006",
                rule_name: "unchecked-repatriate-reserved",
                file,
                line,
                column,
                message: "Resolved repatriate_reserved return value is discarded".to_string(),
            });
        }
        intravisit::walk_stmt(self, stmt);
    }

    fn visit_local(&mut self, local: &'tcx LetStmt<'tcx>) {
        if local
            .init
            .is_some_and(|init| is_repatriate_reserved_call(self.tcx, self.typeck, init))
        {
            let bindings = pattern_binding_ids(local.pat);
            if bindings.is_empty() {
                let (file, line, column) = span_location(self.tcx.sess.source_map(), local.span);
                self.diagnostics.push(RustcDiagnostic {
                    rule_id: "SEC006",
                    rule_name: "unchecked-repatriate-reserved",
                    file,
                    line,
                    column,
                    message: "Resolved repatriate_reserved return value is discarded".to_string(),
                });
            } else {
                self.unchecked_results
                    .extend(bindings.into_iter().map(|binding| (binding, local.span)));
            }
        }
        intravisit::walk_local(self, local);
    }

    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if let ExprKind::MethodCall(segment, receiver, args, _) = expr.kind {
            if matches!(
                segment.ident.name.as_str(),
                "is_zero" | "saturating_sub" | "defensive_saturating_sub" | "checked_sub"
            ) {
                self.mark_checked(receiver);
                for arg in args {
                    self.mark_checked(arg);
                }
            }
        }
        if let ExprKind::Binary(_, lhs, rhs) = expr.kind {
            self.mark_checked(lhs);
            self.mark_checked(rhs);
        }
        if matches!(expr.kind, ExprKind::Closure(_)) {
            return;
        }
        intravisit::walk_expr(self, expr);
    }
}

impl Sec006Visitor<'_, '_> {
    fn mark_checked(&mut self, expr: &Expr<'_>) {
        if let Some(binding) = local_binding_id(self.typeck, expr) {
            self.unchecked_results.remove(&binding);
        }
    }
}

struct Sec007Visitor<'a, 'tcx> {
    source_map: &'a SourceMap,
    tcx: TyCtxt<'tcx>,
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    diagnostics: &'a mut Vec<RustcDiagnostic>,
    reported_lines: &'a mut HashSet<(String, usize)>,
}

impl<'tcx> Visitor<'tcx> for Sec007Visitor<'_, 'tcx> {
    fn visit_local(&mut self, local: &'tcx LetStmt<'tcx>) {
        if matches!(local.pat.kind, PatKind::Wild)
            && local.init.is_some_and(|init| {
                result_error_type(self.tcx, self.typeck.expr_ty(init))
                    .is_some_and(|error_ty| !type_is_unit(self.tcx, error_ty))
            })
        {
            let (file, line, column) = span_location(self.source_map, local.span);
            if self.reported_lines.insert((file.clone(), line)) {
                self.diagnostics.push(RustcDiagnostic {
                    rule_id: "SEC007",
                    rule_name: "let-underscore-result",
                    file,
                    line,
                    column,
                    message: "Resolved Result value is discarded without handling its error"
                        .to_string(),
                });
            }
        }
        intravisit::walk_local(self, local);
    }
}

fn is_repatriate_reserved_call(
    tcx: TyCtxt<'_>,
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    expr: &Expr<'_>,
) -> bool {
    let expr = strip_drop_temps(expr);
    let callee = match expr.kind {
        ExprKind::Call(callee, _) => callee,
        ExprKind::Match(scrutinee, _, rustc_hir::MatchSource::TryDesugar(_)) => {
            return is_repatriate_reserved_call(tcx, typeck, scrutinee);
        }
        _ => return false,
    };
    let ExprKind::Path(qpath) = callee.kind else {
        return false;
    };
    let def_id = typeck
        .type_dependent_def_id(callee.hir_id)
        .or_else(|| typeck.qpath_res(&qpath, callee.hir_id).opt_def_id());
    def_id.is_some_and(|def_id| {
        let path = tcx.def_path_str(def_id);
        is_frame_support_path(&path) && path.ends_with("::repatriate_reserved")
    })
}

impl PolkadotCallbacks {
    fn rule_enabled(&self, rule_id: &str) -> bool {
        self.enabled_rules.is_empty()
            || self.enabled_rules.iter().any(|enabled| {
                enabled == "SEC" || rule_id == enabled || rule_id.starts_with(enabled)
            })
    }
}

fn report_raw_weight_arithmetic<'tcx>(
    tcx: TyCtxt<'tcx>,
    source_map: &SourceMap,
    parsed_weight_attributes: &[ParsedWeightAttribute],
    diagnostics: &mut Vec<RustcDiagnostic>,
) {
    let ranges = parsed_weight_attributes
        .iter()
        .map(|attribute| WeightAttributeRange {
            file: &attribute.file,
            start_line: attribute.attribute_start_line,
            end_line: attribute.attribute_end_line,
        })
        .collect::<Vec<_>>();
    if ranges.is_empty() {
        return;
    }

    let mut reported_lines = HashSet::new();
    for def_id in tcx.hir_body_owners() {
        if !matches!(tcx.hir_body_owner_kind(def_id), BodyOwnerKind::Fn) {
            continue;
        }
        let Some(body) = tcx.hir_maybe_body_owned_by(def_id) else {
            continue;
        };
        let mut visitor = Sec004Visitor {
            source_map,
            tcx,
            typeck: tcx.typeck(def_id),
            ranges: &ranges,
            diagnostics,
            reported_lines: &mut reported_lines,
        };
        visitor.visit_body(body);
    }
}

fn report_xor_as_exponentiation<'tcx>(
    tcx: TyCtxt<'tcx>,
    source_map: &SourceMap,
    diagnostics: &mut Vec<RustcDiagnostic>,
) {
    let mut reported_lines = HashSet::new();

    for def_id in tcx.hir_body_owners() {
        let Some(body) = tcx.hir_maybe_body_owned_by(def_id) else {
            continue;
        };
        let mut visitor = Sem010Visitor {
            source_map,
            typeck: tcx.typeck(def_id),
            diagnostics,
            reported_lines: &mut reported_lines,
        };
        visitor.visit_body(body);
    }
}

fn report_redundant_storage_contains_key<'tcx>(
    tcx: TyCtxt<'tcx>,
    source_map: &SourceMap,
    diagnostics: &mut Vec<RustcDiagnostic>,
) {
    let mut reported_lines = HashSet::new();

    for def_id in tcx.hir_body_owners() {
        let Some(body) = tcx.hir_maybe_body_owned_by(def_id) else {
            continue;
        };
        let mut visitor = Sem009Visitor {
            source_map,
            tcx,
            typeck: tcx.typeck(def_id),
            diagnostics,
            reported_lines: &mut reported_lines,
        };
        visitor.visit_body(body);
    }
}

fn report_dbweight_missing_pov<'tcx>(
    tcx: TyCtxt<'tcx>,
    source_map: &SourceMap,
    diagnostics: &mut Vec<RustcDiagnostic>,
) {
    let mut reported_lines = HashSet::new();
    for def_id in tcx.hir_body_owners() {
        let Some(body) = tcx.hir_maybe_body_owned_by(def_id) else {
            continue;
        };
        let mut visitor = Sem006Visitor {
            source_map,
            tcx,
            typeck: tcx.typeck(def_id),
            diagnostics,
            reported_lines: &mut reported_lines,
        };
        visitor.visit_body(body);
    }
}

fn report_missing_authorize_call_in_create_authorized_transaction<'tcx>(
    tcx: TyCtxt<'tcx>,
    source_map: &SourceMap,
    diagnostics: &mut Vec<RustcDiagnostic>,
) {
    let mut reported_lines = HashSet::new();

    for def_id in tcx.hir_body_owners() {
        if !matches!(tcx.hir_body_owner_kind(def_id), BodyOwnerKind::Fn) {
            continue;
        }
        if tcx.item_name(def_id.to_def_id()).as_str() != "create_extension" {
            continue;
        }
        let Some(trait_id) = tcx
            .impl_of_method(def_id.to_def_id())
            .and_then(|impl_id| tcx.trait_id_of_impl(impl_id))
        else {
            continue;
        };
        let trait_path = tcx.def_path_str(trait_id);
        if !trait_path.contains("frame_system::offchain::CreateAuthorizedTransaction") {
            continue;
        }
        let Some(body) = tcx.hir_maybe_body_owned_by(def_id) else {
            continue;
        };

        let mut visitor = Sem016Visitor {
            tcx,
            typeck: tcx.typeck(def_id),
            found_authorize_call: false,
        };
        visitor.visit_body(body);
        if visitor.found_authorize_call {
            continue;
        }

        let (file, line, column) = span_location(source_map, tcx.def_span(def_id));
        if reported_lines.insert((file.clone(), line)) {
            diagnostics.push(RustcDiagnostic {
                rule_id: "SEM016",
                rule_name: "missing-authorizecall-in-create-authorized-transaction",
                file,
                line,
                column,
                message: "Resolved frame_system::offchain::CreateAuthorizedTransaction::create_extension() does not construct frame_system::AuthorizeCall::new()".to_string(),
            });
        }
    }
}

struct Sem016Visitor<'a, 'tcx> {
    tcx: TyCtxt<'tcx>,
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    found_authorize_call: bool,
}

impl<'tcx> Visitor<'tcx> for Sem016Visitor<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if let ExprKind::Call(callee, _) = expr.kind {
            let def_id = match callee.kind {
                ExprKind::Path(qpath) => self
                    .typeck
                    .type_dependent_def_id(callee.hir_id)
                    .or_else(|| self.typeck.qpath_res(&qpath, callee.hir_id).opt_def_id()),
                _ => None,
            };
            if def_id.is_some_and(|def_id| {
                let path = self.tcx.def_path_str(def_id);
                path.contains("frame_system::AuthorizeCall") && path.ends_with("::new")
            }) {
                self.found_authorize_call = true;
                return;
            }
        }
        intravisit::walk_expr(self, expr);
    }
}

struct Sem006Visitor<'a, 'tcx> {
    source_map: &'a SourceMap,
    tcx: TyCtxt<'tcx>,
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    diagnostics: &'a mut Vec<RustcDiagnostic>,
    reported_lines: &'a mut HashSet<(String, usize)>,
}

impl<'tcx> Visitor<'tcx> for Sem006Visitor<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if let ExprKind::MethodCall(segment, receiver, _, _) = expr.kind {
            let (file, line, column) = span_location(self.source_map, expr.span.source_callsite());
            if matches!(
                segment.ident.name.as_str(),
                "reads" | "writes" | "reads_writes"
            ) && !is_generated_weights_file(&file)
                && type_is_runtime_db_weight(self.tcx, self.typeck.expr_ty(receiver))
                && self.reported_lines.insert((file.clone(), line))
            {
                self.diagnostics.push(RustcDiagnostic {
                    rule_id: "SEM006",
                    rule_name: "dbweight-missing-pov",
                    file,
                    line,
                    column,
                    message: "Resolved RuntimeDbWeight reads/writes only accounts for ref-time, not proof size (PoV)".to_string(),
                });
            }
        }
        intravisit::walk_expr(self, expr);
    }
}

struct Sem009Visitor<'a, 'tcx> {
    source_map: &'a SourceMap,
    tcx: TyCtxt<'tcx>,
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    diagnostics: &'a mut Vec<RustcDiagnostic>,
    reported_lines: &'a mut HashSet<(String, usize)>,
}

impl<'tcx> Visitor<'tcx> for Sem009Visitor<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if let ExprKind::If(condition, then_branch, _) = expr.kind {
            if let Some((storage_owner, keys)) = resolved_storage_call(
                self.source_map,
                self.tcx,
                self.typeck,
                condition,
                "contains_key",
            ) {
                let mut finder = Sem009RemoveTakeFinder {
                    source_map: self.source_map,
                    tcx: self.tcx,
                    typeck: self.typeck,
                    storage_owner,
                    keys,
                    found: false,
                };
                finder.visit_expr(then_branch);

                if finder.found {
                    let (file, line, column) =
                        span_location(self.source_map, condition.span.source_callsite());
                    if self.reported_lines.insert((file.clone(), line)) {
                        self.diagnostics.push(RustcDiagnostic {
                            rule_id: "SEM009",
                            rule_name: "redundant-contains-key-before-remove",
                            file,
                            line,
                            column,
                            message: "Resolved FRAME contains_key() before remove()/take() on the same key is a wasted storage read".to_string(),
                        });
                    }
                }
            }
        }
        intravisit::walk_expr(self, expr);
    }
}

struct Sem009RemoveTakeFinder<'a, 'tcx> {
    source_map: &'a SourceMap,
    tcx: TyCtxt<'tcx>,
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    storage_owner: String,
    keys: Vec<HirId>,
    found: bool,
}

impl<'tcx> Visitor<'tcx> for Sem009RemoveTakeFinder<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if self.found || matches!(expr.kind, ExprKind::Closure(_)) {
            return;
        }
        if matches!(expr.kind, ExprKind::Call(..)) {
            for method in ["remove", "take"] {
                if let Some((storage_owner, keys)) =
                    resolved_storage_call(self.source_map, self.tcx, self.typeck, expr, method)
                {
                    if storage_owner == self.storage_owner && keys == self.keys {
                        self.found = true;
                        return;
                    }
                }
            }
        }
        intravisit::walk_expr(self, expr);
    }
}

fn resolved_storage_call<'tcx>(
    source_map: &SourceMap,
    tcx: TyCtxt<'tcx>,
    typeck: &rustc_middle::ty::TypeckResults<'tcx>,
    expr: &'tcx Expr<'tcx>,
    expected_method: &str,
) -> Option<(String, Vec<HirId>)> {
    let expr = strip_drop_temps(expr);
    let ExprKind::Call(callee, arguments) = expr.kind else {
        return None;
    };
    if associated_call_name(callee).as_deref() != Some(expected_method) {
        return None;
    }
    if !is_frame_storage_associated_call(tcx, typeck, callee) {
        return None;
    }
    let callee_source = source_map
        .span_to_snippet(callee.span.source_callsite())
        .ok()?;
    let (storage_owner, _) = callee_source.rsplit_once("::")?;
    let keys = arguments
        .iter()
        .map(|argument| storage_key_binding(typeck, argument))
        .collect::<Option<Vec<_>>>()?;
    (!keys.is_empty()).then_some((storage_owner.trim().to_string(), keys))
}

fn storage_key_binding(
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    expr: &Expr<'_>,
) -> Option<HirId> {
    let expr = strip_drop_temps(expr);
    match expr.kind {
        ExprKind::AddrOf(_, _, inner) => storage_key_binding(typeck, inner),
        _ => local_binding_id(typeck, expr),
    }
}

struct Sem010Visitor<'a, 'tcx> {
    source_map: &'a SourceMap,
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    diagnostics: &'a mut Vec<RustcDiagnostic>,
    reported_lines: &'a mut HashSet<(String, usize)>,
}

impl<'tcx> Visitor<'tcx> for Sem010Visitor<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if let ExprKind::Binary(operator, lhs, rhs) = expr.kind {
            if operator.node == BinOpKind::BitXor && is_integral(self.typeck.expr_ty(expr)) {
                if let (Some(base), Some(exponent)) = (
                    decimal_integer_literal_value(self.source_map, lhs),
                    decimal_integer_literal_value(self.source_map, rhs),
                ) {
                    if matches!(base, 2 | 10 | 100) && exponent > 3 {
                        let (file, line, column) =
                            span_location(self.source_map, expr.span.source_callsite());
                        if self.reported_lines.insert((file.clone(), line)) {
                            self.diagnostics.push(RustcDiagnostic {
                                rule_id: "SEM010",
                                rule_name: "xor-as-exponentiation",
                                file,
                                line,
                                column,
                                message: format!(
                                    "`{base} ^ {exponent}` is bitwise XOR (= {}), not exponentiation",
                                    base ^ exponent
                                ),
                            });
                        }
                    }
                }
            }
        }
        intravisit::walk_expr(self, expr);
    }
}

struct WeightAttributeRange<'a> {
    file: &'a str,
    start_line: usize,
    end_line: usize,
}

struct Sec004Visitor<'a, 'tcx> {
    source_map: &'a SourceMap,
    tcx: TyCtxt<'tcx>,
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    ranges: &'a [WeightAttributeRange<'a>],
    diagnostics: &'a mut Vec<RustcDiagnostic>,
    reported_lines: &'a mut HashSet<(String, usize)>,
}

impl<'tcx> Visitor<'tcx> for Sec004Visitor<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if matches!(expr.kind, ExprKind::Binary(op, _, _) if matches!(op.node, BinOpKind::Add | BinOpKind::Mul))
            && self.is_weight_attribute_span(expr.span)
            && self.is_raw_numeric_or_weight_binary(expr)
        {
            let (file, line, column) = span_location(self.source_map, expr.span);
            if self.reported_lines.insert((file.clone(), line)) {
                self.diagnostics.push(RustcDiagnostic {
                    rule_id: "SEC004",
                    rule_name: "unsafe-weight-arithmetic",
                    file,
                    line,
                    column,
                    message: "Resolved non-saturating arithmetic inside #[pallet::weight(...)]"
                        .to_string(),
                });
            }
        }
        intravisit::walk_expr(self, expr);
    }
}

impl Sec004Visitor<'_, '_> {
    fn is_weight_attribute_span(&self, span: Span) -> bool {
        let location = self.source_map.lookup_char_pos(span.source_callsite().lo());
        let file = location.file.name.prefer_local().to_string();
        self.ranges.iter().any(|range| {
            range.file == file && (range.start_line..=range.end_line).contains(&location.line)
        })
    }

    fn is_raw_numeric_or_weight_binary(&self, expr: &Expr<'_>) -> bool {
        let ty = self.typeck.expr_ty(expr);
        matches!(ty.kind(), TyKind::Int(_) | TyKind::Uint(_)) || type_is_weight(self.tcx, ty)
    }
}

fn report_expensive_weight_calculation<'tcx>(
    tcx: TyCtxt<'tcx>,
    source_map: &SourceMap,
    parsed_weight_attributes: &[ParsedWeightAttribute],
    diagnostics: &mut Vec<RustcDiagnostic>,
) {
    let ranges = parsed_weight_attributes
        .iter()
        .map(|attribute| WeightAttributeRange {
            file: &attribute.file,
            start_line: attribute.attribute_start_line,
            end_line: attribute.attribute_end_line,
        })
        .collect::<Vec<_>>();
    if ranges.is_empty() {
        return;
    }

    let mut reported_lines = HashSet::new();
    for def_id in tcx.hir_body_owners() {
        if !matches!(tcx.hir_body_owner_kind(def_id), BodyOwnerKind::Fn) {
            continue;
        }
        let Some(body) = tcx.hir_maybe_body_owned_by(def_id) else {
            continue;
        };
        let mut visitor = Sec005Visitor {
            source_map,
            tcx,
            typeck: tcx.typeck(def_id),
            ranges: &ranges,
            diagnostics,
            reported_lines: &mut reported_lines,
        };
        visitor.visit_body(body);
    }
}

struct Sec005Visitor<'a, 'tcx> {
    source_map: &'a SourceMap,
    tcx: TyCtxt<'tcx>,
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    ranges: &'a [WeightAttributeRange<'a>],
    diagnostics: &'a mut Vec<RustcDiagnostic>,
    reported_lines: &'a mut HashSet<(String, usize)>,
}

impl<'tcx> Visitor<'tcx> for Sec005Visitor<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        let expensive = match expr.kind {
            ExprKind::Call(callee, _) => {
                is_frame_storage_read_call(self.tcx, self.typeck, callee)
                    || is_resolved_weight_expense(self.tcx, self.typeck, callee)
            }
            ExprKind::MethodCall(_, _, _, _) => {
                is_resolved_weight_expense(self.tcx, self.typeck, expr)
            }
            _ => false,
        };
        if expensive && self.is_weight_attribute_span(expr.span) {
            let (file, line, column) = span_location(self.source_map, expr.span);
            if self.reported_lines.insert((file.clone(), line)) {
                self.diagnostics.push(RustcDiagnostic {
                    rule_id: "SEC005",
                    rule_name: "expensive-weight-calculation",
                    file,
                    line,
                    column,
                    message: "Resolved expensive operation inside #[pallet::weight(...)]"
                        .to_string(),
                });
            }
        }
        intravisit::walk_expr(self, expr);
    }
}

impl Sec005Visitor<'_, '_> {
    fn is_weight_attribute_span(&self, span: Span) -> bool {
        let location = self.source_map.lookup_char_pos(span.source_callsite().lo());
        let file = location.file.name.prefer_local().to_string();
        self.ranges.iter().any(|range| {
            range.file == file && (range.start_line..=range.end_line).contains(&location.line)
        })
    }
}

fn is_frame_storage_read_call<'tcx>(
    tcx: TyCtxt<'tcx>,
    typeck: &rustc_middle::ty::TypeckResults<'tcx>,
    callee: &'tcx Expr<'tcx>,
) -> bool {
    associated_call_name(callee).is_some_and(|name| name == "get")
        && is_frame_storage_associated_call(tcx, typeck, callee)
}

fn is_resolved_weight_expense(
    tcx: TyCtxt<'_>,
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    expr: &Expr<'_>,
) -> bool {
    if let ExprKind::MethodCall(segment, receiver, _, _) = expr.kind {
        return matches!(
            segment.ident.name.as_str(),
            "get_dispatch_info" | "encode" | "using_encoded" | "decode"
        ) && !matches!(typeck.expr_ty(receiver).kind(), TyKind::Error(_));
    }
    let def_id = typeck
        .type_dependent_def_id(expr.hir_id)
        .or_else(|| match expr.kind {
            ExprKind::MethodCall(segment, _, _, _) => typeck.type_dependent_def_id(segment.hir_id),
            _ => None,
        })
        .or_else(|| {
            let ExprKind::Path(qpath) = expr.kind else {
                return None;
            };
            typeck.qpath_res(&qpath, expr.hir_id).opt_def_id()
        });
    let Some(def_id) = def_id else {
        return false;
    };
    let path = tcx.def_path_str(def_id);
    matches!(
        path.as_str(),
        path if path.ends_with("::GetDispatchInfo::get_dispatch_info")
            || path.ends_with("::parity_scale_codec::Encode::encode")
            || path.ends_with("::parity_scale_codec::Encode::using_encoded")
            || path.ends_with("::parity_scale_codec::Decode::decode")
    )
}

fn report_unbounded_storage_aliases<'tcx>(
    tcx: TyCtxt<'tcx>,
    source_map: &SourceMap,
    parsed_item_attributes: &[ParsedItemAttributes],
    diagnostics: &mut Vec<RustcDiagnostic>,
) {
    for item_id in tcx.hir_free_items() {
        let item = tcx.hir_item(item_id);
        if !matches!(item.kind, ItemKind::TyAlias(..))
            || !is_frame_storage_alias(
                tcx,
                item.owner_id.def_id,
                item.hir_id(),
                source_map,
                parsed_item_attributes,
            )
            || is_explicitly_unbounded_storage(
                tcx,
                item.owner_id.def_id,
                item.hir_id(),
                source_map,
                parsed_item_attributes,
            )
        {
            continue;
        }

        let alias_ty = tcx.type_of(item.owner_id.def_id).instantiate_identity();
        if storage_alias_value_type(tcx, alias_ty)
            .is_some_and(|value_ty| type_contains_unbounded_storage_collection(tcx, value_ty))
        {
            let (file, line, column) = span_location(source_map, item.span);
            diagnostics.push(RustcDiagnostic {
                rule_id: "SEC013",
                rule_name: "unbounded-storage-collections",
                file,
                line,
                column,
                message: "Storage alias resolves to an unbounded collection value".to_string(),
            });
        }
    }
}

fn report_identity_hashers_on_common_keys<'tcx>(
    tcx: TyCtxt<'tcx>,
    source_map: &SourceMap,
    parsed_item_attributes: &[ParsedItemAttributes],
    diagnostics: &mut Vec<RustcDiagnostic>,
) {
    for item_id in tcx.hir_free_items() {
        let item = tcx.hir_item(item_id);
        if !matches!(item.kind, ItemKind::TyAlias(..))
            || !is_frame_storage_alias(
                tcx,
                item.owner_id.def_id,
                item.hir_id(),
                source_map,
                parsed_item_attributes,
            )
        {
            continue;
        }

        let alias_ty = tcx.type_of(item.owner_id.def_id).instantiate_identity();
        let Some((storage_kind, type_args)) = storage_alias_type_arguments(tcx, alias_ty) else {
            continue;
        };
        let has_internal_numeric_layout = parsed_item_attributes_match(
            tcx,
            item.owner_id.def_id,
            source_map,
            parsed_item_attributes,
            |attributes| attributes.internal_numeric_layout,
        );
        if !storage_uses_identity_on_common_key(
            tcx,
            storage_kind,
            &type_args,
            has_internal_numeric_layout,
        ) {
            continue;
        }

        let (file, line, column) = span_location(source_map, item.span);
        diagnostics.push(RustcDiagnostic {
            rule_id: "SEC014",
            rule_name: "identity-hasher-on-common-keys",
            file,
            line,
            column,
            message: "Resolved storage map uses Identity hasher on a common key type".to_string(),
        });
    }
}

fn storage_alias_type_arguments<'tcx>(
    tcx: TyCtxt<'tcx>,
    ty: Ty<'tcx>,
) -> Option<(&'static str, Vec<Ty<'tcx>>)> {
    match ty.kind() {
        TyKind::Adt(adt, args) => {
            let owner = storage_owner_name(&tcx.def_path_str(adt.did()))?;
            let type_args = args
                .iter()
                .filter_map(|arg| match arg.kind() {
                    GenericArgKind::Type(arg_ty) => Some(arg_ty),
                    _ => None,
                })
                .collect();
            Some((owner, type_args))
        }
        TyKind::Alias(kind, alias_ty) => expand_alias_type(tcx, *kind, *alias_ty)
            .and_then(|expanded| storage_alias_type_arguments(tcx, expanded)),
        _ => None,
    }
}

fn storage_uses_identity_on_common_key<'tcx>(
    tcx: TyCtxt<'tcx>,
    storage_kind: &str,
    type_args: &[Ty<'tcx>],
    has_internal_numeric_layout: bool,
) -> bool {
    match storage_kind {
        "StorageMap" | "CountedStorageMap" => {
            identity_common_key_pair(tcx, type_args, 1, 2, has_internal_numeric_layout)
        }
        "StorageDoubleMap" => {
            identity_common_key_pair(tcx, type_args, 1, 2, has_internal_numeric_layout)
                || identity_common_key_pair(tcx, type_args, 3, 4, has_internal_numeric_layout)
        }
        _ => false,
    }
}

fn identity_common_key_pair<'tcx>(
    tcx: TyCtxt<'tcx>,
    type_args: &[Ty<'tcx>],
    hasher_index: usize,
    key_index: usize,
    has_internal_numeric_layout: bool,
) -> bool {
    let Some(hasher_ty) = type_args.get(hasher_index).copied() else {
        return false;
    };
    let Some(key_ty) = type_args.get(key_index).copied() else {
        return false;
    };
    type_is_identity_hasher(tcx, hasher_ty)
        && type_is_common_identity_key(tcx, key_ty)
        && !(has_internal_numeric_layout && type_is_builtin_numeric_key(key_ty))
}

fn type_is_identity_hasher<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> bool {
    match ty.kind() {
        TyKind::Adt(adt, _) => {
            let path = tcx.def_path_str(adt.did());
            is_frame_support_path(&path) && path.ends_with("::Identity")
        }
        TyKind::Alias(kind, alias_ty) => expand_alias_type(tcx, *kind, *alias_ty)
            .is_some_and(|expanded| type_is_identity_hasher(tcx, expanded)),
        _ => false,
    }
}

fn type_is_common_identity_key<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> bool {
    type_is_builtin_numeric_key(ty)
        || matches!(ty.kind(), TyKind::Alias(_, alias_ty) if {
            let path = tcx.def_path_str(alias_ty.def_id);
            path.ends_with("::Balance") || path.ends_with("::BlockNumber")
        })
}

fn type_is_builtin_numeric_key(ty: Ty<'_>) -> bool {
    matches!(
        ty.kind(),
        TyKind::Uint(rustc_middle::ty::UintTy::U32 | rustc_middle::ty::UintTy::U64)
    )
}

fn type_is_weight<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> bool {
    match ty.kind() {
        TyKind::Adt(adt, _) => tcx.def_path_str(adt.did()).ends_with("::Weight"),
        TyKind::Alias(kind, alias_ty) => expand_alias_type(tcx, *kind, *alias_ty)
            .is_some_and(|expanded| type_is_weight(tcx, expanded)),
        _ => false,
    }
}

fn storage_alias_value_type<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> Option<Ty<'tcx>> {
    match ty.kind() {
        TyKind::Adt(adt, args) => {
            let name = tcx.def_path_str(adt.did());
            let value_index = match storage_owner_name(&name)? {
                "StorageValue" => 1,
                "StorageMap" | "CountedStorageMap" => 3,
                "StorageDoubleMap" => 5,
                "StorageNMap" => 2,
                _ => return None,
            };
            args.iter()
                .filter_map(|arg| match arg.kind() {
                    GenericArgKind::Type(arg_ty) => Some(arg_ty),
                    _ => None,
                })
                .nth(value_index)
        }
        TyKind::Alias(kind, alias_ty) => expand_alias_type(tcx, *kind, *alias_ty)
            .and_then(|expanded| storage_alias_value_type(tcx, expanded)),
        _ => None,
    }
}

fn storage_owner_name(name: &str) -> Option<&'static str> {
    [
        "CountedStorageMap",
        "StorageDoubleMap",
        "StorageMap",
        "StorageNMap",
        "StorageValue",
    ]
    .iter()
    .copied()
    .find(|owner| name == *owner || name.ends_with(&format!("::{owner}")))
}

fn is_frame_storage_alias(
    tcx: TyCtxt<'_>,
    def_id: LocalDefId,
    hir_id: rustc_hir::HirId,
    source_map: &SourceMap,
    parsed_item_attributes: &[ParsedItemAttributes],
) -> bool {
    has_hir_attr(tcx, hir_id, &["pallet", "storage"])
        || parsed_item_attributes_match(tcx, def_id, source_map, parsed_item_attributes, |attrs| {
            attrs.storage
        })
        || source_prefix_before_definition(tcx, def_id, source_map).is_some_and(|prefix| {
            prefix
                .rfind("#[pallet::storage]")
                .is_some_and(|start| attribute_block_belongs_to_definition(&prefix[start..]))
        })
}

fn is_explicitly_unbounded_storage(
    tcx: TyCtxt<'_>,
    def_id: LocalDefId,
    hir_id: rustc_hir::HirId,
    source_map: &SourceMap,
    parsed_item_attributes: &[ParsedItemAttributes],
) -> bool {
    has_hir_attr(tcx, hir_id, &["pallet", "unbounded"])
        || parsed_item_attributes_match(tcx, def_id, source_map, parsed_item_attributes, |attrs| {
            attrs.unbounded
        })
        || source_prefix_before_definition(tcx, def_id, source_map).is_some_and(|prefix| {
            prefix
                .rfind("#[pallet::unbounded]")
                .is_some_and(|start| attribute_block_belongs_to_definition(&prefix[start..]))
        })
}

fn report_vec_event_fields<'tcx>(
    tcx: TyCtxt<'tcx>,
    source_map: &SourceMap,
    parsed_item_attributes: &[ParsedItemAttributes],
    parsed_weight_attributes: &[ParsedWeightAttribute],
    tainted_bodies: &TaintedBodyEvidence,
    diagnostics: &mut Vec<RustcDiagnostic>,
) {
    let mut candidates = Vec::new();
    let mut fields_by_variant = HashMap::new();

    for item_id in tcx.hir_free_items() {
        let item = tcx.hir_item(item_id);
        let ItemKind::Enum(_, _, enum_def) = item.kind else {
            continue;
        };
        let source_linked_event_marker = is_frame_event(
            tcx,
            item.owner_id.def_id,
            item.hir_id(),
            source_map,
            parsed_item_attributes,
        );
        if !source_linked_event_marker {
            continue;
        }

        for variant in enum_def.variants {
            for field in variant.data.fields() {
                let field_ty = tcx.type_of(field.def_id).instantiate_identity();
                if type_contains_unbounded_event_vec(tcx, field_ty) {
                    let index = candidates.len();
                    candidates.push(EventFieldCandidate {
                        span: field.span,
                        macro_consumed_event_marker: source_linked_event_marker
                            && source_has_generate_deposit_attribute(source_map, field.span),
                    });
                    fields_by_variant
                        .entry(tcx.parent(field.def_id.to_def_id()))
                        .or_insert_with(HashMap::new)
                        .insert(field.ident.name, index);
                }
            }
        }
    }

    if candidates.is_empty() {
        return;
    }

    let mut emitted_field_indices = HashSet::new();
    for (def_id, unbounded_bindings) in &tainted_bodies.unbounded_bindings {
        let Some(body) = tcx.hir_maybe_body_owned_by(*def_id) else {
            continue;
        };
        let typeck = tcx.typeck(*def_id);
        let mut visitor = Sec017Visitor {
            typeck,
            fields_by_variant: &fields_by_variant,
            unbounded_bindings: unbounded_bindings.clone(),
            weight_accounted_bindings: tainted_bodies
                .weight_accounted_bindings
                .get(def_id)
                .cloned()
                .unwrap_or_else(|| {
                    weight_accounted_dispatchable_bindings(
                        tcx,
                        *def_id,
                        source_map,
                        parsed_weight_attributes,
                    )
                }),
            emitted_field_indices: &mut emitted_field_indices,
        };
        visitor.visit_body(body);
    }

    for (index, candidate) in candidates.iter().enumerate() {
        if !candidate.macro_consumed_event_marker && !emitted_field_indices.contains(&index) {
            continue;
        }
        let (file, line, column) = span_location(source_map, candidate.span);
        diagnostics.push(RustcDiagnostic {
            rule_id: "SEC017",
            rule_name: "vec-in-events",
            file,
            line,
            column,
            message: "Event field resolves to an unbounded Vec payload".to_string(),
        });
    }
}

fn weight_accounted_dispatchable_bindings(
    tcx: TyCtxt<'_>,
    def_id: LocalDefId,
    source_map: &SourceMap,
    parsed_weight_attributes: &[ParsedWeightAttribute],
) -> HashSet<HirId> {
    if !is_frame_dispatchable(tcx, def_id, source_map, parsed_weight_attributes) {
        return HashSet::new();
    }
    let Some(weight_attributes) =
        pallet_weight_attributes(tcx, def_id, source_map, parsed_weight_attributes)
    else {
        return HashSet::new();
    };
    tcx.hir_maybe_body_owned_by(def_id)
        .into_iter()
        .flat_map(|body| {
            body.params.iter().filter_map(|param| {
                let name = body_param_name(param)?;
                weight_accounts_for_param(&weight_attributes.expression, &name)
                    .then(|| pattern_binding_ids(param.pat))
            })
        })
        .flatten()
        .collect()
}

fn weight_accounted_dispatchable_parameter_indices(
    tcx: TyCtxt<'_>,
    def_id: LocalDefId,
    source_map: &SourceMap,
    parsed_weight_attributes: &[ParsedWeightAttribute],
) -> HashSet<usize> {
    if !is_frame_dispatchable(tcx, def_id, source_map, parsed_weight_attributes) {
        return HashSet::new();
    }
    let Some(weight_attributes) =
        pallet_weight_attributes(tcx, def_id, source_map, parsed_weight_attributes)
    else {
        return HashSet::new();
    };
    tcx.hir_maybe_body_owned_by(def_id)
        .into_iter()
        .flat_map(|body| {
            body.params.iter().enumerate().filter_map(|(index, param)| {
                let name = body_param_name(param)?;
                weight_accounts_for_param(&weight_attributes.expression, &name).then_some(index)
            })
        })
        .collect()
}

fn source_has_generate_deposit_attribute(source_map: &SourceMap, span: Span) -> bool {
    let location = source_map.lookup_char_pos(span.lo());
    let Ok(source) = std::fs::read_to_string(location.file.name.prefer_local().to_string()) else {
        return false;
    };
    let lines = source.lines().collect::<Vec<_>>();
    let end = location.line.saturating_sub(1).min(lines.len());
    let start = end.saturating_sub(8);
    lines[start..end]
        .iter()
        .any(|line| line.contains("#[pallet::generate_deposit"))
}

struct Sec017Visitor<'a, 'tcx> {
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    fields_by_variant: &'a HashMap<DefId, HashMap<Symbol, usize>>,
    unbounded_bindings: HashSet<HirId>,
    weight_accounted_bindings: HashSet<HirId>,
    emitted_field_indices: &'a mut HashSet<usize>,
}

impl<'tcx> Visitor<'tcx> for Sec017Visitor<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if let ExprKind::Assign(target, value, _) = expr.kind {
            if let Some(binding) = local_binding_id(self.typeck, target) {
                if expr_references_tainted_binding(self.typeck, value, &self.unbounded_bindings) {
                    self.unbounded_bindings.insert(binding);
                } else {
                    self.unbounded_bindings.remove(&binding);
                }
                if local_binding_id(self.typeck, value).is_some_and(|value_binding| {
                    self.weight_accounted_bindings.contains(&value_binding)
                }) {
                    self.weight_accounted_bindings.insert(binding);
                } else {
                    self.weight_accounted_bindings.remove(&binding);
                }
            }
        }

        if let ExprKind::Struct(qpath, fields, _) = expr.kind {
            let variant = match self.typeck.qpath_res(&qpath, expr.hir_id) {
                Res::Def(_, def_id) => def_id,
                _ => return intravisit::walk_expr(self, expr),
            };
            if let Some(event_fields) = self.fields_by_variant.get(&variant) {
                for field in fields {
                    if let Some(index) = event_fields.get(&field.ident.name) {
                        if expr_references_tainted_binding(
                            self.typeck,
                            field.expr,
                            &self.unbounded_bindings,
                        ) && !expr_references_tainted_binding(
                            self.typeck,
                            field.expr,
                            &self.weight_accounted_bindings,
                        ) {
                            self.emitted_field_indices.insert(*index);
                        }
                    }
                }
            }
        }

        intravisit::walk_expr(self, expr);
    }

    fn visit_local(&mut self, local: &'tcx LetStmt<'tcx>) {
        let bindings = pattern_binding_ids(local.pat);
        if local.init.is_some_and(|init| {
            expr_references_tainted_binding(self.typeck, init, &self.unbounded_bindings)
        }) {
            self.unbounded_bindings.extend(bindings.iter().copied());
        } else {
            self.unbounded_bindings
                .retain(|binding| !bindings.contains(binding));
        }
        if local
            .init
            .and_then(|init| local_binding_id(self.typeck, init))
            .is_some_and(|value_binding| self.weight_accounted_bindings.contains(&value_binding))
        {
            self.weight_accounted_bindings
                .extend(bindings.iter().copied());
        } else {
            self.weight_accounted_bindings
                .retain(|binding| !bindings.contains(binding));
        }

        intravisit::walk_local(self, local);
    }
}

fn is_frame_event(
    tcx: TyCtxt<'_>,
    def_id: LocalDefId,
    hir_id: rustc_hir::HirId,
    source_map: &SourceMap,
    parsed_item_attributes: &[ParsedItemAttributes],
) -> bool {
    if has_hir_attr(tcx, hir_id, &["pallet", "event"]) {
        return true;
    }
    if parsed_item_attributes_match(tcx, def_id, source_map, parsed_item_attributes, |attrs| {
        attrs.event
    }) {
        return true;
    }

    let Some(prefix) = source_prefix_before_definition(tcx, def_id, source_map) else {
        return false;
    };
    let Some(event_start) = prefix.rfind("#[pallet::event]") else {
        return false;
    };

    attribute_block_belongs_to_definition(&prefix[event_start..])
        && tcx.def_path_str(def_id.to_def_id()).ends_with("::Event")
}

fn parsed_item_attributes_match(
    tcx: TyCtxt<'_>,
    def_id: LocalDefId,
    source_map: &SourceMap,
    parsed_item_attributes: &[ParsedItemAttributes],
    predicate: impl Fn(&ParsedItemAttributes) -> bool,
) -> bool {
    let definition_span = tcx
        .def_ident_span(def_id.to_def_id())
        .unwrap_or_else(|| tcx.def_span(def_id));
    let location = source_map.lookup_char_pos(definition_span.lo());
    let file = location.file.name.prefer_local().to_string();
    let name = tcx.item_name(def_id.to_def_id());
    parsed_item_attributes.iter().any(|attributes| {
        attributes.item_name == name.as_str()
            && attributes.file == file
            && attributes.line == location.line
            && predicate(attributes)
    })
}

fn report_unbounded_public_vec_inputs<'tcx>(
    tcx: TyCtxt<'tcx>,
    def_id: LocalDefId,
    body: &'tcx rustc_hir::Body<'tcx>,
    source_map: &SourceMap,
    parsed_weight_attributes: &[ParsedWeightAttribute],
    diagnostics: &mut Vec<RustcDiagnostic>,
) {
    if !tcx.local_visibility(def_id).is_public()
        || !is_frame_dispatchable(tcx, def_id, source_map, parsed_weight_attributes)
    {
        return;
    }
    if has_privileged_origin_guard(tcx, tcx.typeck(def_id), body) {
        return;
    }

    let sig = tcx.fn_sig(def_id).instantiate_identity().skip_binder();
    for (idx, ty) in sig.inputs().iter().enumerate() {
        if !type_contains_unbounded_vec(tcx, *ty) {
            continue;
        }
        let Some(param) = body.params.get(idx) else {
            continue;
        };
        if has_initial_terminating_vec_length_bound(tcx.typeck(def_id), body, param) {
            continue;
        }
        let (file, line, column) = span_location(source_map, param.span);
        diagnostics.push(RustcDiagnostic {
            rule_id: "SEC001",
            rule_name: "unbounded-vec-in-extrinsic",
            file,
            line,
            column,
            message: "Public callable accepts an unbounded Vec parameter after type resolution"
                .to_string(),
        });
    }
}

fn has_initial_terminating_vec_length_bound(
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    body: &rustc_hir::Body<'_>,
    param: &rustc_hir::Param<'_>,
) -> bool {
    let Some(binding) = pattern_binding_ids(param.pat).into_iter().next() else {
        return false;
    };
    let ExprKind::Block(block, _) = body.value.kind else {
        return false;
    };
    let mut literal_bindings = HashSet::new();
    for statement in block.stmts {
        match statement.kind {
            StmtKind::Let(local) if local.init.is_some_and(integer_literal) => {
                literal_bindings.extend(pattern_binding_ids(local.pat));
            }
            StmtKind::Expr(first) | StmtKind::Semi(first) => {
                let ExprKind::If(condition, then_branch, None) = strip_drop_temps(first).kind
                else {
                    return false;
                };
                return expression_exits_current_function(then_branch)
                    && condition_rejects_oversized_vec(
                        typeck,
                        condition,
                        binding,
                        &literal_bindings,
                    );
            }
            StmtKind::Let(_) | StmtKind::Item(_) => return false,
        }
    }
    false
}

fn condition_rejects_oversized_vec(
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    condition: &Expr<'_>,
    binding: HirId,
    literal_bindings: &HashSet<HirId>,
) -> bool {
    let condition = strip_drop_temps(condition);
    if let ExprKind::Unary(rustc_hir::UnOp::Not, inner) = condition.kind {
        return vec_length_is_within_bound(typeck, inner, binding, literal_bindings);
    }
    vec_length_exceeds_bound(typeck, condition, binding, literal_bindings)
}

fn vec_length_is_within_bound(
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    condition: &Expr<'_>,
    binding: HirId,
    literal_bindings: &HashSet<HirId>,
) -> bool {
    let ExprKind::Binary(operator, lhs, rhs) = strip_drop_temps(condition).kind else {
        return false;
    };
    (matches!(operator.node, BinOpKind::Lt | BinOpKind::Le)
        && expr_is_vec_length_of_binding(typeck, lhs, binding)
        && integer_literal_or_bound_binding(typeck, rhs, literal_bindings))
        || (matches!(operator.node, BinOpKind::Gt | BinOpKind::Ge)
            && expr_is_vec_length_of_binding(typeck, rhs, binding)
            && integer_literal_or_bound_binding(typeck, lhs, literal_bindings))
}

fn vec_length_exceeds_bound(
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    condition: &Expr<'_>,
    binding: HirId,
    literal_bindings: &HashSet<HirId>,
) -> bool {
    let ExprKind::Binary(operator, lhs, rhs) = strip_drop_temps(condition).kind else {
        return false;
    };
    (matches!(operator.node, BinOpKind::Gt | BinOpKind::Ge)
        && expr_is_vec_length_of_binding(typeck, lhs, binding)
        && integer_literal_or_bound_binding(typeck, rhs, literal_bindings))
        || (matches!(operator.node, BinOpKind::Lt | BinOpKind::Le)
            && expr_is_vec_length_of_binding(typeck, rhs, binding)
            && integer_literal_or_bound_binding(typeck, lhs, literal_bindings))
}

fn integer_literal_or_bound_binding(
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    expr: &Expr<'_>,
    literal_bindings: &HashSet<HirId>,
) -> bool {
    integer_literal(expr)
        || local_binding_id(typeck, expr).is_some_and(|binding| literal_bindings.contains(&binding))
}

fn expr_is_vec_length_of_binding(
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    expr: &Expr<'_>,
    binding: HirId,
) -> bool {
    let ExprKind::MethodCall(segment, receiver, _, _) = strip_drop_temps(expr).kind else {
        return false;
    };
    segment.ident.name.as_str() == "len" && local_binding_id(typeck, receiver) == Some(binding)
}

fn has_privileged_origin_guard<'tcx>(
    tcx: TyCtxt<'tcx>,
    typeck: &'tcx rustc_middle::ty::TypeckResults<'tcx>,
    body: &'tcx rustc_hir::Body<'tcx>,
) -> bool {
    let mut visitor = PrivilegedOriginVisitor {
        tcx,
        typeck,
        has_privileged_guard: false,
        inside_try_desugar: false,
        conditional_depth: 0,
    };
    visitor.visit_body(body);
    visitor.has_privileged_guard
}

struct PrivilegedOriginVisitor<'a, 'tcx> {
    tcx: TyCtxt<'tcx>,
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    has_privileged_guard: bool,
    inside_try_desugar: bool,
    conditional_depth: usize,
}

impl<'tcx> Visitor<'tcx> for PrivilegedOriginVisitor<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        let was_inside_try_desugar = self.inside_try_desugar;
        let was_conditional_depth = self.conditional_depth;
        self.inside_try_desugar |= matches!(
            expr.kind,
            ExprKind::Match(_, _, rustc_hir::MatchSource::TryDesugar(_))
        );
        if matches!(expr.kind, ExprKind::If(..) | ExprKind::Loop(..))
            || matches!(expr.kind, ExprKind::Match(_, _, source) if !matches!(source, rustc_hir::MatchSource::TryDesugar(_)))
        {
            self.conditional_depth += 1;
        }
        if self.inside_try_desugar && self.conditional_depth == 0 {
            if let ExprKind::Call(callee, _) = expr.kind {
                let def_id = match callee.kind {
                    ExprKind::Path(qpath) => {
                        self.typeck.qpath_res(&qpath, callee.hir_id).opt_def_id()
                    }
                    _ => None,
                };
                if let Some(def_id) = def_id {
                    let path = self.tcx.def_path_str(def_id);
                    self.has_privileged_guard |= is_frame_system_root_check(&path)
                        || (is_frame_ensure_origin_check(&path)
                            && callee_uses_named_privileged_origin(self.tcx, self.typeck, callee));
                }
            }
        }

        intravisit::walk_expr(self, expr);
        self.inside_try_desugar = was_inside_try_desugar;
        self.conditional_depth = was_conditional_depth;
    }
}

fn is_frame_system_root_check(path: &str) -> bool {
    path.ends_with("::frame_system::ensure_root") || path.contains("frame_system::ensure_root")
}

fn is_frame_ensure_origin_check(path: &str) -> bool {
    is_frame_support_path(path)
        && matches!(
            path.rsplit("::").next(),
            Some("ensure_origin" | "ensure_origin_or_root")
        )
}

fn callee_uses_named_privileged_origin<'tcx>(
    tcx: TyCtxt<'tcx>,
    typeck: &rustc_middle::ty::TypeckResults<'tcx>,
    callee: &'tcx Expr<'tcx>,
) -> bool {
    associated_call_receiver_type(typeck, callee)
        .is_some_and(|origin_ty| type_is_named_privileged_origin(tcx, origin_ty))
}

fn type_is_named_privileged_origin<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> bool {
    let def_id = match ty.kind() {
        TyKind::Alias(_, alias_ty) => alias_ty.def_id,
        TyKind::Adt(adt, _) => adt.did(),
        _ => return false,
    };
    matches!(
        tcx.item_name(def_id).as_str(),
        "AdminOrigin"
            | "ForceOrigin"
            | "FounderSetOrigin"
            | "GovernanceOrigin"
            | "RelayChainOrigin"
            | "RootOrigin"
    )
}

fn report_missing_weight_for_unbounded_inputs<'tcx>(
    tcx: TyCtxt<'tcx>,
    def_id: LocalDefId,
    body: &'tcx rustc_hir::Body<'tcx>,
    source_map: &SourceMap,
    parsed_weight_attributes: &[ParsedWeightAttribute],
    diagnostics: &mut Vec<RustcDiagnostic>,
) {
    if !tcx.local_visibility(def_id).is_public() {
        return;
    }
    if !is_frame_dispatchable(tcx, def_id, source_map, parsed_weight_attributes) {
        return;
    }

    let Some(weight_attributes) =
        pallet_weight_attributes(tcx, def_id, source_map, parsed_weight_attributes)
    else {
        return;
    };
    if has_hir_attr(tcx, tcx.local_def_id_to_hir_id(def_id), &["deprecated"])
        || weight_attributes.deprecated
    {
        return;
    }

    let sig = tcx.fn_sig(def_id).instantiate_identity().skip_binder();
    for (idx, ty) in sig.inputs().iter().enumerate() {
        if !type_contains_unbounded_vec(tcx, *ty) {
            continue;
        }
        let Some(param) = body.params.get(idx) else {
            continue;
        };
        let Some(param_name) = body_param_name(param) else {
            continue;
        };
        if has_initial_terminating_vec_length_bound(tcx.typeck(def_id), body, param) {
            continue;
        }
        if weight_accounts_for_param(&weight_attributes.expression, &param_name) {
            continue;
        }

        let (file, line, column) = span_location(source_map, param.span);
        diagnostics.push(RustcDiagnostic {
            rule_id: "SEC018",
            rule_name: "missing-weight-for-unbounded-input",
            file,
            line,
            column,
            message: format!(
                "Weight attribute does not account for resolved unbounded `{param_name}` input"
            ),
        });
    }
}

struct PalletWeightAttributes {
    expression: SynExpr,
    deprecated: bool,
}

fn pallet_weight_attributes(
    tcx: TyCtxt<'_>,
    def_id: LocalDefId,
    source_map: &SourceMap,
    parsed_weight_attributes: &[ParsedWeightAttribute],
) -> Option<PalletWeightAttributes> {
    let definition_span = tcx
        .def_ident_span(def_id.to_def_id())
        .unwrap_or_else(|| tcx.def_span(def_id));
    let definition_location = source_map.lookup_char_pos(definition_span.lo());
    let definition_file = definition_location.file.name.prefer_local().to_string();
    let definition_line = definition_location.line;
    let function_name = tcx.item_name(def_id.to_def_id());
    if let Some(attributes) = parsed_weight_attributes
        .iter()
        .find(|attribute| {
            attribute.function_name == function_name.as_str()
                && attribute.file == definition_file
                && attribute.line == definition_line
        })
        .and_then(|attribute| {
            parse_pallet_weight_attributes(&attribute.attribute_source, attribute.deprecated)
        })
    {
        return Some(attributes);
    }

    let prefix = source_prefix_before_definition(tcx, def_id, source_map)?;
    let start = prefix.rfind("#[pallet::weight")?;
    let attribute_block = &prefix[start..];
    if !attribute_block_belongs_to_definition(attribute_block) {
        return None;
    }
    let attribute = balanced_attribute(attribute_block)?;

    parse_pallet_weight_attributes(attribute, attribute_block.contains("#[deprecated"))
}

fn parse_pallet_weight_attributes(
    attribute: &str,
    deprecated: bool,
) -> Option<PalletWeightAttributes> {
    let attributes = syn::Attribute::parse_outer.parse_str(attribute).ok()?;
    let weight_attribute = attributes
        .iter()
        .find(|attr| syn_path_matches(attr.path(), &["pallet", "weight"]))?;
    Some(PalletWeightAttributes {
        expression: weight_attribute.parse_args().ok()?,
        deprecated,
    })
}

struct ParsedWeightAttributeVisitor<'a> {
    source_map: &'a SourceMap,
    parsed_attributes: &'a mut Vec<ParsedWeightAttribute>,
    parsed_item_attributes: &'a mut Vec<ParsedItemAttributes>,
    parsed_transactional_attributes: &'a mut Vec<ParsedTransactionalAttribute>,
}

impl ParsedWeightAttributeVisitor<'_> {
    fn collect_function(
        &mut self,
        function_name: &str,
        span: Span,
        attributes: &[rustc_ast::ast::Attribute],
    ) {
        if attributes.iter().any(|attribute| {
            matches!(&attribute.kind, rustc_ast::ast::AttrKind::Normal(normal) if normal.item.path.segments.last().is_some_and(|segment| segment.ident.name.as_str() == "transactional"))
        }) {
            let location = self.source_map.lookup_char_pos(span.lo());
            self.parsed_transactional_attributes
                .push(ParsedTransactionalAttribute {
                    function_name: function_name.to_string(),
                    file: location.file.name.prefer_local().to_string(),
                    line: location.line,
                });
        }
        let Some(weight_attribute) = attributes.iter().find(|attribute| {
            matches!(&attribute.kind, rustc_ast::ast::AttrKind::Normal(normal) if ast_path_matches(&normal.item.path, &["pallet", "weight"]))
        }) else {
            return;
        };
        let Ok(attribute_source) = self.source_map.span_to_snippet(weight_attribute.span) else {
            return;
        };
        let location = self.source_map.lookup_char_pos(span.lo());
        let attribute_start = self.source_map.lookup_char_pos(weight_attribute.span.lo());
        let attribute_end = self.source_map.lookup_char_pos(weight_attribute.span.hi());
        self.parsed_attributes.push(ParsedWeightAttribute {
            function_name: function_name.to_string(),
            file: location.file.name.prefer_local().to_string(),
            line: location.line,
            attribute_start_line: attribute_start.line,
            attribute_end_line: attribute_end.line,
            attribute_source,
            deprecated: attributes.iter().any(|attribute| {
                matches!(&attribute.kind, rustc_ast::ast::AttrKind::Normal(normal) if ast_path_matches(&normal.item.path, &["deprecated"]))
            }),
            has_call_index: attributes.iter().any(|attribute| {
                matches!(&attribute.kind, rustc_ast::ast::AttrKind::Normal(normal) if ast_path_matches(&normal.item.path, &["pallet", "call_index"]))
            }),
        });
    }

    fn collect_item_attributes(&mut self, item: &rustc_ast::ast::Item) {
        let item_name = match &item.kind {
            rustc_ast::ast::ItemKind::TyAlias(alias) => alias.ident.name.as_str(),
            rustc_ast::ast::ItemKind::Enum(ident, ..) => ident.name.as_str(),
            _ => return,
        };
        let storage = item.attrs.iter().any(|attribute| {
            matches!(&attribute.kind, rustc_ast::ast::AttrKind::Normal(normal) if ast_path_matches(&normal.item.path, &["pallet", "storage"]))
        });
        let unbounded = item.attrs.iter().any(|attribute| {
            matches!(&attribute.kind, rustc_ast::ast::AttrKind::Normal(normal) if ast_path_matches(&normal.item.path, &["pallet", "unbounded"]))
        });
        let event = item.attrs.iter().any(|attribute| {
            matches!(&attribute.kind, rustc_ast::ast::AttrKind::Normal(normal) if ast_path_matches(&normal.item.path, &["pallet", "event"]))
        });
        if !(storage || unbounded || event) {
            return;
        }
        let internal_numeric_layout = item
            .attrs
            .iter()
            .filter_map(|attribute| self.source_map.span_to_snippet(attribute.span).ok())
            .map(|source| source.to_ascii_lowercase())
            .any(|source| {
                ["ring buffer", "index", "indices", "segment", "position"]
                    .iter()
                    .any(|marker| source.contains(marker))
            });
        let location = self.source_map.lookup_char_pos(item.span.lo());
        self.parsed_item_attributes.push(ParsedItemAttributes {
            item_name: item_name.to_string(),
            file: location.file.name.prefer_local().to_string(),
            line: location.line,
            storage,
            unbounded,
            event,
            internal_numeric_layout,
        });
    }
}

impl<'ast> ast_visit::Visitor<'ast> for ParsedWeightAttributeVisitor<'_> {
    type Result = ();

    fn visit_item(&mut self, item: &'ast rustc_ast::ast::Item) -> Self::Result {
        if matches!(
            item.kind,
            rustc_ast::ast::ItemKind::TyAlias(..) | rustc_ast::ast::ItemKind::Enum(..)
        ) {
            self.collect_item_attributes(item);
        }
        if let rustc_ast::ast::ItemKind::Fn(function) = &item.kind {
            self.collect_function(function.ident.name.as_str(), item.span, &item.attrs);
        }
        ast_visit::walk_item(self, item)
    }

    fn visit_assoc_item(
        &mut self,
        item: &'ast rustc_ast::ast::AssocItem,
        context: ast_visit::AssocCtxt,
    ) -> Self::Result {
        if let rustc_ast::ast::AssocItemKind::Fn(function) = &item.kind {
            self.collect_function(function.ident.name.as_str(), item.span, &item.attrs);
        }
        ast_visit::walk_assoc_item(self, item, context)
    }
}

fn ast_path_matches(path: &rustc_ast::Path, expected: &[&str]) -> bool {
    path.segments
        .iter()
        .map(|segment| segment.ident.name.as_str())
        .eq(expected.iter().copied())
}

fn attribute_block_belongs_to_definition(attribute_block: &str) -> bool {
    !attribute_block
        .lines()
        .skip(1)
        .map(str::trim_start)
        .any(starts_rust_item)
}

fn starts_rust_item(line: &str) -> bool {
    let line = line.strip_prefix("pub ").unwrap_or(line);
    [
        "fn ", "impl ", "struct ", "enum ", "trait ", "type ", "mod ",
    ]
    .iter()
    .any(|item| line.starts_with(item))
}

fn syn_path_matches(path: &syn::Path, expected: &[&str]) -> bool {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .eq(expected.iter().map(|segment| (*segment).to_string()))
}

fn is_frame_dispatchable(
    tcx: TyCtxt<'_>,
    def_id: LocalDefId,
    source_map: &SourceMap,
    parsed_weight_attributes: &[ParsedWeightAttribute],
) -> bool {
    let definition_span = tcx
        .def_ident_span(def_id.to_def_id())
        .unwrap_or_else(|| tcx.def_span(def_id));
    let definition_location = source_map.lookup_char_pos(definition_span.lo());
    let definition_file = definition_location.file.name.prefer_local().to_string();
    let definition_line = definition_location.line;
    let function_name = tcx.item_name(def_id.to_def_id());
    if parsed_weight_attributes.iter().any(|attribute| {
        attribute.has_call_index
            && attribute.function_name == function_name.as_str()
            && attribute.file == definition_file
            && attribute.line == definition_line
    }) {
        return true;
    }
    let matching_captured_dispatchables = parsed_weight_attributes
        .iter()
        .filter(|attribute| {
            attribute.has_call_index
                && attribute.function_name == function_name.as_str()
                && attribute.file == definition_file
        })
        .count();
    if matching_captured_dispatchables == 1 {
        return true;
    }

    let Some(prefix) = source_prefix_before_definition(tcx, def_id, source_map) else {
        return false;
    };
    let Some(call_index_start) = prefix.rfind("#[pallet::call_index") else {
        return false;
    };
    let attribute_block = &prefix[call_index_start..];

    attribute_block.contains("#[pallet::weight")
        && attribute_block_belongs_to_definition(attribute_block)
}

fn source_prefix_before_definition(
    tcx: TyCtxt<'_>,
    def_id: LocalDefId,
    source_map: &SourceMap,
) -> Option<String> {
    let definition_span = tcx
        .def_ident_span(def_id.to_def_id())
        .unwrap_or_else(|| tcx.def_span(def_id));
    let location = source_map.lookup_char_pos(definition_span.lo());
    let source_path = location.file.name.prefer_local().to_string();
    let source = std::fs::read_to_string(source_path).ok()?;
    let lines = source.lines().collect::<Vec<_>>();
    let function_line = location.line.saturating_sub(1).min(lines.len());
    let start_line = function_line.saturating_sub(64);
    Some(lines[start_line..function_line].join("\n"))
}

fn balanced_attribute(source: &str) -> Option<&str> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for (index, ch) in source.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if ch == '"' {
            in_string = true;
            continue;
        }
        if ch == '[' {
            depth += 1;
        } else if ch == ']' {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(&source[..=index]);
            }
        }
    }

    None
}

struct Sec002Visitor<'a> {
    source_map: &'a SourceMap,
    diagnostics: &'a mut Vec<RustcDiagnostic>,
    reported_lines: &'a mut HashSet<(String, usize)>,
}

impl<'tcx> Visitor<'tcx> for Sec002Visitor<'_> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if let Some(call_site) = debug_assert_call_site(self.source_map, expr.span) {
            let (file, line, column) = span_location(self.source_map, call_site);
            if self.reported_lines.insert((file.clone(), line)) {
                self.diagnostics.push(RustcDiagnostic {
                    rule_id: "SEC002",
                    rule_name: "debug-assert-in-production",
                    file,
                    line,
                    column,
                    message: "`debug_assert!` expands into a debug-only panic path".to_string(),
                });
            }
        }

        intravisit::walk_expr(self, expr);
    }
}

struct Sec003Visitor<'a, 'tcx> {
    source_map: &'a SourceMap,
    tcx: TyCtxt<'tcx>,
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    diagnostics: &'a mut Vec<RustcDiagnostic>,
    tainted_bindings: HashSet<HirId>,
}

impl<'tcx> Visitor<'tcx> for Sec003Visitor<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if let ExprKind::If(condition, then_branch, else_branch) = expr.kind {
            self.visit_expr(condition);
            let incoming_taint = self.tainted_bindings.clone();

            self.visit_expr(then_branch);
            let then_taint = self.tainted_bindings.clone();

            self.tainted_bindings = incoming_taint.clone();
            if let Some(else_branch) = else_branch {
                self.visit_expr(else_branch);
            }
            self.tainted_bindings.extend(then_taint);
            return;
        }

        if let ExprKind::Assign(target, value, _) = expr.kind {
            if let Some(binding) = local_binding_id(self.typeck, target) {
                if expr_references_tainted_binding(self.typeck, value, &self.tainted_bindings) {
                    self.tainted_bindings.insert(binding);
                } else {
                    self.tainted_bindings.remove(&binding);
                }
            }
        }

        if is_unlimited_decode_call(expr)
            && (type_contains_recursive_decode_target(self.tcx, self.typeck.expr_ty(expr))
                || decode_receiver_contains_recursive_target(self.tcx, self.typeck, expr))
            && decode_call_uses_tainted_input(self.typeck, expr, &self.tainted_bindings)
        {
            let (file, line, column) = span_location(self.source_map, expr.span);
            if !span_line_starts_with_attribute(self.source_map, expr.span) {
                self.diagnostics.push(RustcDiagnostic {
                    rule_id: "SEC003",
                    rule_name: "missing-decode-depth-limit",
                    file,
                    line,
                    column,
                    message: "Recursive runtime type is decoded without a depth limit".to_string(),
                });
            }
        }

        if let ExprKind::MethodCall(segment, receiver, args, _) = expr.kind {
            if segment.ident.name.as_str() == "using_encoded"
                && expr_references_tainted_binding(self.typeck, receiver, &self.tainted_bindings)
            {
                self.visit_expr(receiver);
                for arg in args {
                    let ExprKind::Closure(closure) = arg.kind else {
                        self.visit_expr(arg);
                        continue;
                    };
                    let body = self.tcx.hir_body(closure.body);
                    let mut closure_visitor = Sec003Visitor {
                        source_map: self.source_map,
                        tcx: self.tcx,
                        // The enclosing body carries the resolved associated projection for
                        // closures passed to generic encoding helpers.
                        typeck: self.typeck,
                        diagnostics: self.diagnostics,
                        tainted_bindings: body
                            .params
                            .iter()
                            .flat_map(|param| pattern_binding_ids(param.pat))
                            .collect(),
                    };
                    closure_visitor.visit_body(body);
                }
                return;
            }
        }

        if let ExprKind::Match(scrutinee, arms, _) = expr.kind {
            self.visit_expr(scrutinee);
            let incoming_taint = self.tainted_bindings.clone();
            let mut arm_taint = HashSet::new();
            for arm in arms {
                self.tainted_bindings = incoming_taint.clone();
                let tainted_arm_bindings = tainted_pattern_binding_ids(
                    self.typeck,
                    scrutinee,
                    arm,
                    &self.tainted_bindings,
                );
                self.tainted_bindings
                    .extend(tainted_arm_bindings.iter().copied());
                self.visit_arm(arm);
                for binding in tainted_arm_bindings {
                    self.tainted_bindings.remove(&binding);
                }
                arm_taint.extend(self.tainted_bindings.iter().copied());
            }
            self.tainted_bindings = arm_taint;
            return;
        }

        intravisit::walk_expr(self, expr);
    }

    fn visit_local(&mut self, local: &'tcx LetStmt<'tcx>) {
        if local.init.is_some_and(|init| {
            expr_references_tainted_binding(self.typeck, init, &self.tainted_bindings)
        }) {
            self.tainted_bindings.extend(pattern_binding_ids(local.pat));
        }

        intravisit::walk_local(self, local);
    }
}

struct Sec015Visitor<'a, 'tcx> {
    source_map: &'a SourceMap,
    tcx: TyCtxt<'tcx>,
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    diagnostics: &'a mut Vec<RustcDiagnostic>,
    reported_lines: &'a mut HashSet<(String, usize)>,
    root_guard_depth: usize,
}

impl<'tcx> Visitor<'tcx> for Sec015Visitor<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if let ExprKind::If(condition, then_branch, else_branch) = expr.kind {
            self.visit_expr(condition);
            if is_resolved_root_is_ok(self.tcx, self.typeck, condition) {
                self.root_guard_depth += 1;
                self.visit_expr(then_branch);
                self.root_guard_depth -= 1;
            } else {
                self.visit_expr(then_branch);
            }
            if let Some(else_branch) = else_branch {
                self.visit_expr(else_branch);
            }
            return;
        }

        if matches!(expr.kind, ExprKind::MethodCall(_, _, _, _) if is_resolved_dispatch_bypass_filter(self.tcx, self.typeck, expr))
            && self.root_guard_depth == 0
        {
            let (file, line, column) = span_location(self.source_map, expr.span);
            if self.reported_lines.insert((file.clone(), line)) {
                self.diagnostics.push(RustcDiagnostic {
                    rule_id: "SEC015",
                    rule_name: "dispatch-bypass-filter-in-production",
                    file,
                    line,
                    column,
                    message: "Resolved dispatch_bypass_filter call lacks a root guard".to_string(),
                });
            }
        }

        intravisit::walk_expr(self, expr);
    }
}

fn is_resolved_dispatch_bypass_filter(
    tcx: TyCtxt<'_>,
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    expr: &Expr<'_>,
) -> bool {
    typeck
        .type_dependent_def_id(expr.hir_id)
        .is_some_and(|def_id| {
            let path = tcx.def_path_str(def_id);
            is_frame_support_path(&path) && path.ends_with("::dispatch_bypass_filter")
        })
}

fn is_resolved_root_is_ok(
    tcx: TyCtxt<'_>,
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    expr: &Expr<'_>,
) -> bool {
    let expr = strip_drop_temps(expr);
    let ExprKind::MethodCall(segment, receiver, _, _) = expr.kind else {
        return false;
    };
    if segment.ident.name.as_str() != "is_ok" {
        return false;
    }
    let receiver = strip_drop_temps(receiver);
    let ExprKind::Call(callee, _) = receiver.kind else {
        return false;
    };
    let ExprKind::Path(qpath) = callee.kind else {
        return false;
    };
    typeck
        .qpath_res(&qpath, callee.hir_id)
        .opt_def_id()
        .is_some_and(|def_id| is_frame_system_root_check(&tcx.def_path_str(def_id)))
}

struct Sec008Visitor<'a, 'tcx> {
    source_map: &'a SourceMap,
    tcx: TyCtxt<'tcx>,
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    diagnostics: &'a mut Vec<RustcDiagnostic>,
    reported_lines: &'a mut HashSet<(String, usize)>,
    known_unwrappable_bindings: HashSet<HirId>,
}

impl<'tcx> Visitor<'tcx> for Sec008Visitor<'_, 'tcx> {
    fn visit_block(&mut self, block: &'tcx Block<'tcx>) {
        let mut exit_guards = Vec::new();

        for statement in block.stmts {
            self.visit_stmt(statement);
            if let Some(binding) =
                exiting_unwrappable_guard_binding(self.tcx, self.typeck, statement)
            {
                if self.known_unwrappable_bindings.insert(binding) {
                    exit_guards.push(binding);
                }
            }
            if let Some(binding) = exiting_unwrappable_let_binding(self.tcx, self.typeck, statement)
            {
                if self.known_unwrappable_bindings.insert(binding) {
                    exit_guards.push(binding);
                }
            }
        }
        if let Some(expr) = block.expr {
            self.visit_expr(expr);
        }

        for binding in exit_guards {
            self.known_unwrappable_bindings.remove(&binding);
        }
    }

    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if let ExprKind::If(condition, then_branch, else_branch) = expr.kind {
            self.visit_expr(condition);
            let incoming_bindings = self.known_unwrappable_bindings.clone();

            self.known_unwrappable_bindings = incoming_bindings.clone();
            if let Some(binding) = unwrappable_then_guard_binding(self.tcx, self.typeck, condition)
                .or_else(|| unwrappable_then_let_binding(self.tcx, self.typeck, condition))
            {
                let newly_known = self.known_unwrappable_bindings.insert(binding);
                self.visit_expr(then_branch);
                if newly_known {
                    self.known_unwrappable_bindings.remove(&binding);
                }
                if let Some(else_branch) = else_branch {
                    self.visit_expr(else_branch);
                }
            } else {
                self.visit_expr(then_branch);
            }
            let then_bindings = self.known_unwrappable_bindings.clone();

            self.known_unwrappable_bindings = incoming_bindings;
            if let Some(else_branch) = else_branch {
                self.visit_expr(else_branch);
            }
            self.known_unwrappable_bindings = intersect_known_unwrappable_bindings(
                then_bindings,
                &self.known_unwrappable_bindings,
            );
            return;
        }

        if let ExprKind::Match(scrutinee, arms, _) = expr.kind {
            self.visit_expr(scrutinee);
            let incoming_bindings = self.known_unwrappable_bindings.clone();
            let known_binding = local_binding_id(self.typeck, scrutinee)
                .filter(|_| type_is_option_or_result(self.tcx, self.typeck.expr_ty(scrutinee)));
            let mut arm_bindings = None;
            for arm in arms {
                self.known_unwrappable_bindings = incoming_bindings.clone();
                if let Some(binding) =
                    known_binding.filter(|_| pattern_is_unwrappable_success(arm.pat))
                {
                    let newly_known = self.known_unwrappable_bindings.insert(binding);
                    self.visit_arm(arm);
                    if newly_known {
                        self.known_unwrappable_bindings.remove(&binding);
                    }
                } else {
                    self.visit_arm(arm);
                }
                let current_bindings = self.known_unwrappable_bindings.clone();
                arm_bindings = Some(match arm_bindings {
                    Some(bindings) => {
                        intersect_known_unwrappable_bindings(bindings, &current_bindings)
                    }
                    None => current_bindings,
                });
            }
            self.known_unwrappable_bindings = arm_bindings.unwrap_or(incoming_bindings);
            return;
        }

        if let ExprKind::Assign(target, value, _) = expr.kind {
            if let Some(binding) = local_binding_id(self.typeck, target) {
                if expression_constructs_known_unwrappable(self.tcx, self.typeck, value) {
                    self.known_unwrappable_bindings.insert(binding);
                } else {
                    self.known_unwrappable_bindings.remove(&binding);
                }
            }
        }

        if let ExprKind::MethodCall(segment, receiver, _, _) = expr.kind {
            let method = segment.ident.name.as_str();
            if matches!(method, "unwrap" | "expect")
                && !receiver_is_result_with_uninhabited_error(
                    self.tcx,
                    self.typeck.expr_ty(receiver),
                )
                && !expression_is_known_unwrappable(
                    self.tcx,
                    self.typeck,
                    receiver,
                    &self.known_unwrappable_bindings,
                )
            {
                let (file, line, column) = span_location(self.source_map, expr.span);
                if self.reported_lines.insert((file.clone(), line)) {
                    self.diagnostics.push(RustcDiagnostic {
                        rule_id: "SEC008",
                        rule_name: "panic-in-production",
                        file,
                        line,
                        column,
                        message: format!("`.{method}()` can panic on a reachable error path"),
                    });
                }
            }
        }

        intravisit::walk_expr(self, expr);
    }

    fn visit_local(&mut self, local: &'tcx LetStmt<'tcx>) {
        if local.init.is_some_and(|init| {
            expression_constructs_known_unwrappable(self.tcx, self.typeck, init)
        }) {
            self.known_unwrappable_bindings
                .extend(pattern_binding_ids(local.pat));
        }

        intravisit::walk_local(self, local);
    }
}

fn intersect_known_unwrappable_bindings(
    mut first: HashSet<HirId>,
    second: &HashSet<HirId>,
) -> HashSet<HirId> {
    first.retain(|binding| second.contains(binding));
    first
}

fn pattern_is_unwrappable_success(pattern: &Pat<'_>) -> bool {
    match pattern.kind {
        PatKind::TupleStruct(qpath, _, _) => {
            matches!(
                qpath_last_segment(qpath).map(|segment| segment.ident.name.as_str()),
                Some("Some" | "Ok")
            )
        }
        PatKind::Or(patterns) => patterns
            .iter()
            .all(|pattern| pattern_is_unwrappable_success(pattern)),
        _ => false,
    }
}

fn unwrappable_then_guard_binding(
    tcx: TyCtxt<'_>,
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    condition: &Expr<'_>,
) -> Option<HirId> {
    let condition = strip_drop_temps(condition);
    let (negated, condition) = match condition.kind {
        ExprKind::Unary(rustc_hir::UnOp::Not, inner) => (true, strip_drop_temps(inner)),
        _ => (false, condition),
    };
    let ExprKind::MethodCall(segment, receiver, _, _) = condition.kind else {
        return None;
    };
    let receiver_ty = typeck.expr_ty(receiver);
    let method = segment.ident.name.as_str();
    let guard_accepts_success = (method == "is_some" && !negated)
        || (method == "is_none" && negated)
        || (method == "is_ok" && !negated)
        || (method == "is_err" && negated);
    (guard_accepts_success && type_is_option_or_result(tcx, receiver_ty))
        .then(|| local_binding_id(typeck, receiver))
        .flatten()
}

fn unwrappable_then_let_binding(
    tcx: TyCtxt<'_>,
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    condition: &Expr<'_>,
) -> Option<HirId> {
    let ExprKind::Let(let_expr) = strip_drop_temps(condition).kind else {
        return None;
    };
    (pattern_is_unwrappable_success(let_expr.pat)
        && type_is_option_or_result(tcx, typeck.expr_ty(let_expr.init)))
    .then(|| local_binding_id(typeck, let_expr.init))
    .flatten()
}

fn exiting_unwrappable_guard_binding(
    tcx: TyCtxt<'_>,
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    statement: &rustc_hir::Stmt<'_>,
) -> Option<HirId> {
    let (StmtKind::Expr(expr) | StmtKind::Semi(expr)) = statement.kind else {
        return None;
    };
    let ExprKind::If(condition, then_branch, None) = strip_drop_temps(expr).kind else {
        return None;
    };
    if !expression_exits_current_function(then_branch) {
        return None;
    }

    let condition = strip_drop_temps(condition);
    let (negated, condition) = match condition.kind {
        ExprKind::Unary(rustc_hir::UnOp::Not, inner) => (true, strip_drop_temps(inner)),
        _ => (false, condition),
    };
    let ExprKind::MethodCall(segment, receiver, _, _) = condition.kind else {
        return None;
    };
    let receiver_ty = typeck.expr_ty(receiver);
    let method = segment.ident.name.as_str();
    let guard_rejects_failure = (method == "is_none" && !negated)
        || (method == "is_some" && negated)
        || (method == "is_err" && !negated)
        || (method == "is_ok" && negated);
    if !guard_rejects_failure || !type_is_option_or_result(tcx, receiver_ty) {
        return None;
    }
    local_binding_id(typeck, receiver)
}

fn exiting_unwrappable_let_binding(
    tcx: TyCtxt<'_>,
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    statement: &rustc_hir::Stmt<'_>,
) -> Option<HirId> {
    let StmtKind::Let(local) = statement.kind else {
        return None;
    };
    let init = local.init?;
    let else_block = local.els?;
    (pattern_is_unwrappable_success(local.pat)
        && type_is_option_or_result(tcx, typeck.expr_ty(init))
        && block_exits_current_function(else_block))
    .then(|| local_binding_id(typeck, init))
    .flatten()
}

struct LocalCalleeVisitor<'a, 'tcx> {
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    callees: HashSet<LocalDefId>,
    function_bindings: HashMap<HirId, HashSet<LocalDefId>>,
}

impl<'tcx> Visitor<'tcx> for LocalCalleeVisitor<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if let ExprKind::If(condition, then_branch, else_branch) = expr.kind {
            self.visit_expr(condition);
            let incoming_bindings = self.function_bindings.clone();

            self.visit_expr(then_branch);
            let then_bindings = self.function_bindings.clone();

            self.function_bindings = incoming_bindings;
            if let Some(else_branch) = else_branch {
                self.visit_expr(else_branch);
            }
            self.function_bindings =
                merge_function_bindings(then_bindings, &self.function_bindings);
            return;
        }

        if let ExprKind::Match(scrutinee, arms, _) = expr.kind {
            self.visit_expr(scrutinee);
            let incoming_bindings = self.function_bindings.clone();
            let mut arm_bindings = HashMap::new();

            for arm in arms {
                self.function_bindings = incoming_bindings.clone();
                self.visit_arm(arm);
                arm_bindings = merge_function_bindings(arm_bindings, &self.function_bindings);
            }
            self.function_bindings = arm_bindings;
            return;
        }

        match expr.kind {
            ExprKind::Call(callee, _) => self.record_callee(callee),
            ExprKind::Assign(target, value, _) => {
                if let Some(binding) = local_binding_id(self.typeck, target) {
                    self.assign_function_binding(binding, value);
                }
            }
            ExprKind::MethodCall(..) => {
                if let Some(def_id) = self.typeck.type_dependent_def_id(expr.hir_id) {
                    if let Some(local_def_id) = def_id.as_local() {
                        self.callees.insert(local_def_id);
                    }
                }
            }
            _ => {}
        }

        intravisit::walk_expr(self, expr);
    }

    fn visit_local(&mut self, local: &'tcx LetStmt<'tcx>) {
        let bindings = pattern_binding_ids(local.pat);
        if let Some(init) = local.init {
            let callees = self.callees_for_value(init);
            for binding in bindings {
                if callees.is_empty() {
                    self.function_bindings.remove(&binding);
                } else {
                    self.function_bindings.insert(binding, callees.clone());
                }
            }
        }
        intravisit::walk_local(self, local);
    }
}

impl LocalCalleeVisitor<'_, '_> {
    fn record_callee(&mut self, callee: &Expr<'_>) {
        self.callees.extend(self.callees_for_value(callee));
    }

    fn assign_function_binding(&mut self, binding: HirId, value: &Expr<'_>) {
        let callees = self.callees_for_value(value);
        if callees.is_empty() {
            self.function_bindings.remove(&binding);
        } else {
            self.function_bindings.insert(binding, callees);
        }
    }

    fn callees_for_value(&self, value: &Expr<'_>) -> HashSet<LocalDefId> {
        if let Some(callee) = direct_local_callee(self.typeck, value) {
            return HashSet::from([callee]);
        }
        local_binding_id(self.typeck, value)
            .and_then(|binding| self.function_bindings.get(&binding).cloned())
            .unwrap_or_default()
    }
}

fn merge_function_bindings(
    mut first: HashMap<HirId, HashSet<LocalDefId>>,
    second: &HashMap<HirId, HashSet<LocalDefId>>,
) -> HashMap<HirId, HashSet<LocalDefId>> {
    for (binding, callees) in second {
        first
            .entry(*binding)
            .or_default()
            .extend(callees.iter().copied());
    }
    first
}

fn direct_local_callee(
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    expr: &Expr<'_>,
) -> Option<LocalDefId> {
    let ExprKind::Path(qpath) = expr.kind else {
        return None;
    };
    typeck
        .qpath_res(&qpath, expr.hir_id)
        .opt_def_id()?
        .as_local()
}

struct TaintedLocalCallVisitor<'a, 'tcx> {
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    tainted_bindings: HashSet<HirId>,
    weight_accounted_bindings: HashSet<HirId>,
    tainted_callee_parameters: Vec<(LocalDefId, usize)>,
    unaccounted_callee_parameters: Vec<(LocalDefId, usize)>,
    function_bindings: HashMap<HirId, HashSet<LocalDefId>>,
}

impl<'tcx> Visitor<'tcx> for TaintedLocalCallVisitor<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if let ExprKind::If(condition, then_branch, else_branch) = expr.kind {
            self.visit_expr(condition);
            let incoming_taint = self.tainted_bindings.clone();
            let incoming_function_bindings = self.function_bindings.clone();

            self.visit_expr(then_branch);
            let then_taint = self.tainted_bindings.clone();
            let then_function_bindings = self.function_bindings.clone();

            self.tainted_bindings = incoming_taint.clone();
            self.function_bindings = incoming_function_bindings;
            if let Some(else_branch) = else_branch {
                self.visit_expr(else_branch);
            }
            self.tainted_bindings.extend(then_taint);
            self.function_bindings =
                merge_function_bindings(then_function_bindings, &self.function_bindings);
            return;
        }

        if let ExprKind::Match(scrutinee, arms, _) = expr.kind {
            self.visit_expr(scrutinee);
            let incoming_taint = self.tainted_bindings.clone();
            let incoming_function_bindings = self.function_bindings.clone();
            let mut arm_taint = HashSet::new();
            let mut arm_function_bindings = HashMap::new();
            for arm in arms {
                self.tainted_bindings = incoming_taint.clone();
                self.function_bindings = incoming_function_bindings.clone();
                let tainted_arm_bindings = tainted_pattern_binding_ids(
                    self.typeck,
                    scrutinee,
                    arm,
                    &self.tainted_bindings,
                );
                self.tainted_bindings
                    .extend(tainted_arm_bindings.iter().copied());
                self.visit_arm(arm);
                for binding in tainted_arm_bindings {
                    self.tainted_bindings.remove(&binding);
                }
                arm_taint.extend(self.tainted_bindings.iter().copied());
                arm_function_bindings =
                    merge_function_bindings(arm_function_bindings, &self.function_bindings);
            }
            self.tainted_bindings = arm_taint;
            self.function_bindings = arm_function_bindings;
            return;
        }

        match expr.kind {
            ExprKind::Assign(target, value, _) => {
                if let Some(binding) = local_binding_id(self.typeck, target) {
                    self.assign_function_binding(binding, value);
                    if expr_references_tainted_binding(self.typeck, value, &self.tainted_bindings) {
                        self.tainted_bindings.insert(binding);
                    } else {
                        self.tainted_bindings.remove(&binding);
                    }
                    if expr_references_tainted_binding(
                        self.typeck,
                        value,
                        &self.weight_accounted_bindings,
                    ) {
                        self.weight_accounted_bindings.insert(binding);
                    } else {
                        self.weight_accounted_bindings.remove(&binding);
                    }
                }
            }
            ExprKind::Call(callee, args) => {
                for local_def_id in self.callees_for_value(callee) {
                    self.record_tainted_arguments(Some(local_def_id), args.iter());
                }
            }
            ExprKind::MethodCall(_, receiver, args, _) => {
                let local_def_id = self
                    .typeck
                    .type_dependent_def_id(expr.hir_id)
                    .and_then(|def_id| def_id.as_local());
                self.record_tainted_arguments(
                    local_def_id,
                    std::iter::once(receiver).chain(args.iter()),
                );
            }
            _ => {}
        }

        intravisit::walk_expr(self, expr);
    }

    fn visit_local(&mut self, local: &'tcx LetStmt<'tcx>) {
        if let Some(init) = local.init {
            let callees = self.callees_for_value(init);
            for binding in pattern_binding_ids(local.pat) {
                if callees.is_empty() {
                    self.function_bindings.remove(&binding);
                } else {
                    self.function_bindings.insert(binding, callees.clone());
                }
            }
        }
        if local.init.is_some_and(|init| {
            expr_references_tainted_binding(self.typeck, init, &self.tainted_bindings)
        }) {
            self.tainted_bindings.extend(pattern_binding_ids(local.pat));
        }
        if local.init.is_some_and(|init| {
            expr_references_tainted_binding(self.typeck, init, &self.weight_accounted_bindings)
        }) {
            self.weight_accounted_bindings
                .extend(pattern_binding_ids(local.pat));
        }

        intravisit::walk_local(self, local);
    }
}

impl TaintedLocalCallVisitor<'_, '_> {
    fn assign_function_binding(&mut self, binding: HirId, value: &Expr<'_>) {
        let callees = self.callees_for_value(value);
        if callees.is_empty() {
            self.function_bindings.remove(&binding);
        } else {
            self.function_bindings.insert(binding, callees);
        }
    }

    fn callees_for_value(&self, value: &Expr<'_>) -> HashSet<LocalDefId> {
        if let Some(callee) = direct_local_callee(self.typeck, value) {
            return HashSet::from([callee]);
        }
        local_binding_id(self.typeck, value)
            .and_then(|binding| self.function_bindings.get(&binding).cloned())
            .unwrap_or_default()
    }

    fn record_tainted_arguments<'tcx>(
        &mut self,
        local_def_id: Option<LocalDefId>,
        args: impl Iterator<Item = &'tcx Expr<'tcx>>,
    ) {
        let Some(local_def_id) = local_def_id else {
            return;
        };
        let args = args.collect::<Vec<_>>();
        self.tainted_callee_parameters
            .extend(args.iter().enumerate().filter_map(|(index, arg)| {
                expr_references_tainted_binding(self.typeck, arg, &self.tainted_bindings)
                    .then_some((local_def_id, index))
            }));
        self.unaccounted_callee_parameters
            .extend(args.iter().enumerate().filter_map(|(index, arg)| {
                (expr_references_tainted_binding(self.typeck, arg, &self.tainted_bindings)
                    && !expr_references_tainted_binding(
                        self.typeck,
                        arg,
                        &self.weight_accounted_bindings,
                    ))
                .then_some((local_def_id, index))
            }));
    }
}

struct Sec009Visitor<'a, 'tcx> {
    source_map: &'a SourceMap,
    tcx: TyCtxt<'tcx>,
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    diagnostics: &'a mut Vec<RustcDiagnostic>,
    reported_lines: &'a mut HashSet<(String, usize)>,
    non_underflow_pairs: HashSet<(HirId, HirId)>,
    nonzero_bindings: HashSet<HirId>,
}

impl<'tcx> Visitor<'tcx> for Sec009Visitor<'_, 'tcx> {
    fn visit_block(&mut self, block: &'tcx Block<'tcx>) {
        let mut exiting_underflow_guards = Vec::new();
        let mut exiting_nonzero_guards = Vec::new();

        for statement in block.stmts {
            self.visit_stmt(statement);
            if let Some(pair) = exiting_non_underflow_guard_pair(self.typeck, statement) {
                if self.non_underflow_pairs.insert(pair) {
                    exiting_underflow_guards.push(pair);
                }
            }
            if let Some(binding) = exiting_nonzero_guard_binding(self.typeck, statement) {
                if self.nonzero_bindings.insert(binding) {
                    exiting_nonzero_guards.push(binding);
                }
            }
        }
        if let Some(expr) = block.expr {
            self.visit_expr(expr);
        }

        for pair in exiting_underflow_guards {
            self.non_underflow_pairs.remove(&pair);
        }
        for binding in exiting_nonzero_guards {
            self.nonzero_bindings.remove(&binding);
        }
    }

    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if let ExprKind::If(condition, then_branch, else_branch) = expr.kind {
            self.visit_expr(condition);
            let underflow_guards = non_underflow_guard_pairs(self.typeck, condition);
            let nonzero_guards = nonzero_guard_bindings(self.typeck, condition);
            let else_underflow_guard =
                else_branch.and_then(|_| failed_non_underflow_guard_pair(self.typeck, condition));
            let else_nonzero_guard =
                else_branch.and_then(|_| failed_nonzero_guard_binding(self.typeck, condition));
            if !underflow_guards.is_empty()
                || !nonzero_guards.is_empty()
                || else_underflow_guard.is_some()
                || else_nonzero_guard.is_some()
            {
                let inserted_underflow_guards = underflow_guards
                    .into_iter()
                    .filter(|pair| self.non_underflow_pairs.insert(*pair))
                    .collect::<Vec<_>>();
                let inserted_nonzero_guards = nonzero_guards
                    .into_iter()
                    .filter(|binding| self.nonzero_bindings.insert(*binding))
                    .collect::<Vec<_>>();
                self.visit_expr(then_branch);
                for pair in inserted_underflow_guards {
                    self.non_underflow_pairs.remove(&pair);
                }
                for binding in inserted_nonzero_guards {
                    self.nonzero_bindings.remove(&binding);
                }
                if let Some(else_branch) = else_branch {
                    if let Some(pair) = else_underflow_guard {
                        self.non_underflow_pairs.insert(pair);
                    }
                    if let Some(binding) = else_nonzero_guard {
                        self.nonzero_bindings.insert(binding);
                    }
                    self.visit_expr(else_branch);
                    if let Some(pair) = else_underflow_guard {
                        self.non_underflow_pairs.remove(&pair);
                    }
                    if let Some(binding) = else_nonzero_guard {
                        self.nonzero_bindings.remove(&binding);
                    }
                }
                return;
            }
        }

        if let ExprKind::Match(scrutinee, arms, _) = expr.kind {
            if let Some(nonzero_binding) = local_binding_id(self.typeck, scrutinee).filter(|_| {
                arms.iter()
                    .any(|arm| pattern_is_positive_integer_literal(arm.pat))
            }) {
                self.visit_expr(scrutinee);
                for arm in arms {
                    let inserted_nonzero_binding = pattern_is_positive_integer_literal(arm.pat)
                        .then(|| nonzero_binding)
                        .filter(|binding| self.nonzero_bindings.insert(*binding));
                    self.visit_arm(arm);
                    if let Some(binding) = inserted_nonzero_binding {
                        self.nonzero_bindings.remove(&binding);
                    }
                }
                return;
            }
        }

        if let ExprKind::Binary(op, lhs, rhs) = expr.kind {
            let (file, line, column) = span_location(self.source_map, expr.span);
            if !span_line_starts_with_attribute(self.source_map, expr.span)
                && is_raw_arithmetic(op.node)
                && is_integral(self.typeck.expr_ty(lhs))
                && is_integral(self.typeck.expr_ty(rhs))
                && !(matches!(op.node, BinOpKind::Sub)
                    && subtraction_is_guarded(self.typeck, lhs, rhs, &self.non_underflow_pairs))
                && !(matches!(op.node, BinOpKind::Div | BinOpKind::Rem)
                    && divisor_is_proven_nonzero(
                        self.tcx,
                        self.typeck,
                        rhs,
                        &self.nonzero_bindings,
                    ))
                && self.reported_lines.insert((file.clone(), line))
            {
                self.diagnostics.push(RustcDiagnostic {
                    rule_id: "SEC009",
                    rule_name: "raw-arithmetic-in-fallible",
                    file,
                    line,
                    column,
                    message: "Fallible function uses raw arithmetic on resolved integer operands"
                        .to_string(),
                });
            }
        }

        intravisit::walk_expr(self, expr);
    }
}

fn pattern_is_positive_integer_literal(pattern: &Pat<'_>) -> bool {
    matches!(
        pattern.kind,
        PatKind::Expr(pattern_expression)
            if matches!(
                pattern_expression.kind,
                PatExprKind::Lit { lit, negated: false }
                    if matches!(lit.node, rustc_ast::LitKind::Int(value, _) if value.get() > 0)
            )
    )
}

fn nonzero_guard_bindings(
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    condition: &Expr<'_>,
) -> Vec<HirId> {
    let condition = strip_drop_temps(condition);
    if let ExprKind::Binary(operator, lhs, rhs) = condition.kind {
        if operator.node == BinOpKind::And {
            let mut bindings = nonzero_guard_bindings(typeck, lhs);
            bindings.extend(nonzero_guard_bindings(typeck, rhs));
            return bindings;
        }
    }
    nonzero_guard_binding(typeck, condition)
        .into_iter()
        .collect()
}

fn nonzero_guard_binding(
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    condition: &Expr<'_>,
) -> Option<HirId> {
    let ExprKind::Binary(operator, lhs, rhs) = strip_drop_temps(condition).kind else {
        return None;
    };
    match operator.node {
        BinOpKind::Ne if zero_integer_literal(rhs) => local_binding_id(typeck, lhs),
        BinOpKind::Ne if zero_integer_literal(lhs) => local_binding_id(typeck, rhs),
        BinOpKind::Gt if zero_integer_literal(rhs) => local_binding_id(typeck, lhs),
        BinOpKind::Lt if zero_integer_literal(lhs) => local_binding_id(typeck, rhs),
        BinOpKind::Ge if positive_integer_literal(rhs) => local_binding_id(typeck, lhs),
        BinOpKind::Le if positive_integer_literal(lhs) => local_binding_id(typeck, rhs),
        _ => None,
    }
}

fn non_underflow_guard_pairs(
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    condition: &Expr<'_>,
) -> Vec<(HirId, HirId)> {
    let condition = strip_drop_temps(condition);
    if let ExprKind::Binary(operator, lhs, rhs) = condition.kind {
        if operator.node == BinOpKind::And {
            let mut pairs = non_underflow_guard_pairs(typeck, lhs);
            pairs.extend(non_underflow_guard_pairs(typeck, rhs));
            return pairs;
        }
    }
    non_underflow_guard_pair(typeck, condition)
        .into_iter()
        .collect()
}

fn non_underflow_guard_pair(
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    condition: &Expr<'_>,
) -> Option<(HirId, HirId)> {
    let ExprKind::Binary(operator, lhs, rhs) = strip_drop_temps(condition).kind else {
        return None;
    };
    let lhs_binding = local_binding_id(typeck, lhs)?;
    let rhs_binding = local_binding_id(typeck, rhs)?;
    match operator.node {
        BinOpKind::Ge => Some((lhs_binding, rhs_binding)),
        BinOpKind::Le => Some((rhs_binding, lhs_binding)),
        _ => None,
    }
}

fn exiting_non_underflow_guard_pair(
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    statement: &rustc_hir::Stmt<'_>,
) -> Option<(HirId, HirId)> {
    let (StmtKind::Expr(expr) | StmtKind::Semi(expr)) = statement.kind else {
        return None;
    };
    let ExprKind::If(condition, then_branch, None) = strip_drop_temps(expr).kind else {
        return None;
    };
    if !expression_exits_current_function(then_branch) {
        return None;
    }
    failed_non_underflow_guard_pair(typeck, condition)
}

fn exiting_nonzero_guard_binding(
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    statement: &rustc_hir::Stmt<'_>,
) -> Option<HirId> {
    let (StmtKind::Expr(expr) | StmtKind::Semi(expr)) = statement.kind else {
        return None;
    };
    let ExprKind::If(condition, then_branch, None) = strip_drop_temps(expr).kind else {
        return None;
    };
    expression_exits_current_function(then_branch)
        .then(|| failed_nonzero_guard_binding(typeck, condition))
        .flatten()
}

fn failed_non_underflow_guard_pair(
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    condition: &Expr<'_>,
) -> Option<(HirId, HirId)> {
    let condition = strip_drop_temps(condition);
    if let ExprKind::Unary(rustc_hir::UnOp::Not, inner) = condition.kind {
        return non_underflow_guard_pair(typeck, inner);
    }

    let ExprKind::Binary(operator, lhs, rhs) = condition.kind else {
        return None;
    };
    let lhs_binding = local_binding_id(typeck, lhs)?;
    let rhs_binding = local_binding_id(typeck, rhs)?;
    match operator.node {
        BinOpKind::Lt => Some((lhs_binding, rhs_binding)),
        BinOpKind::Gt => Some((rhs_binding, lhs_binding)),
        _ => None,
    }
}

fn failed_nonzero_guard_binding(
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    condition: &Expr<'_>,
) -> Option<HirId> {
    let condition = strip_drop_temps(condition);
    if let ExprKind::Unary(rustc_hir::UnOp::Not, inner) = condition.kind {
        return nonzero_guard_binding(typeck, inner);
    }

    let ExprKind::Binary(operator, lhs, rhs) = condition.kind else {
        return None;
    };
    if operator.node != BinOpKind::Eq {
        return None;
    }
    if zero_integer_literal(rhs) {
        return local_binding_id(typeck, lhs);
    }
    zero_integer_literal(lhs)
        .then(|| local_binding_id(typeck, rhs))
        .flatten()
}

fn expression_exits_current_function(expr: &Expr<'_>) -> bool {
    match strip_drop_temps(expr).kind {
        ExprKind::Ret(_) => true,
        ExprKind::Block(block, _) => block_exits_current_function(block),
        _ => false,
    }
}

fn block_exits_current_function(block: &Block<'_>) -> bool {
    block.expr.is_some_and(expression_exits_current_function)
        || block
            .stmts
            .last()
            .and_then(statement_expression)
            .is_some_and(expression_exits_current_function)
}

fn statement_expression<'hir>(statement: &'hir rustc_hir::Stmt<'hir>) -> Option<&'hir Expr<'hir>> {
    match statement.kind {
        StmtKind::Expr(expr) | StmtKind::Semi(expr) => Some(expr),
        StmtKind::Let(_) | StmtKind::Item(_) => None,
    }
}

fn subtraction_is_guarded(
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    lhs: &Expr<'_>,
    rhs: &Expr<'_>,
    non_underflow_pairs: &HashSet<(HirId, HirId)>,
) -> bool {
    let Some(lhs_binding) = local_binding_id(typeck, lhs) else {
        return false;
    };
    let Some(rhs_binding) = local_binding_id(typeck, rhs) else {
        return false;
    };
    non_underflow_pairs.contains(&(lhs_binding, rhs_binding))
}

fn divisor_is_proven_nonzero(
    tcx: TyCtxt<'_>,
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    divisor: &Expr<'_>,
    nonzero_bindings: &HashSet<HirId>,
) -> bool {
    local_binding_id(typeck, divisor).is_some_and(|binding| nonzero_bindings.contains(&binding))
        || divisor_is_nonzero_integer_get(tcx, typeck, divisor)
}

fn divisor_is_nonzero_integer_get(
    tcx: TyCtxt<'_>,
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    divisor: &Expr<'_>,
) -> bool {
    let ExprKind::MethodCall(segment, receiver, args, _) = strip_drop_temps(divisor).kind else {
        return false;
    };
    args.is_empty()
        && segment.ident.name.as_str() == "get"
        && typeck
            .expr_ty(receiver)
            .ty_adt_def()
            .is_some_and(|adt| tcx.def_path_str(adt.did()).ends_with("::NonZero"))
}

fn integer_literal(expr: &Expr<'_>) -> bool {
    matches!(
        strip_drop_temps(expr).kind,
        ExprKind::Lit(literal) if matches!(literal.node, rustc_ast::LitKind::Int(..))
    )
}

fn decimal_integer_literal_value(source_map: &SourceMap, expr: &Expr<'_>) -> Option<u128> {
    let expr = strip_drop_temps(expr);
    let snippet = source_map
        .span_to_snippet(expr.span.source_callsite())
        .ok()?;
    let literal = snippet.trim_start();
    if literal.starts_with("0b") || literal.starts_with("0o") || literal.starts_with("0x") {
        return None;
    }

    match expr.kind {
        ExprKind::Lit(literal) => match literal.node {
            rustc_ast::LitKind::Int(value, _) => Some(value.get()),
            _ => None,
        },
        _ => None,
    }
}

fn zero_integer_literal(expr: &Expr<'_>) -> bool {
    matches!(
        strip_drop_temps(expr).kind,
        ExprKind::Lit(literal) if matches!(literal.node, rustc_ast::LitKind::Int(value, _) if value.get() == 0)
    )
}

fn positive_integer_literal(expr: &Expr<'_>) -> bool {
    matches!(
        strip_drop_temps(expr).kind,
        ExprKind::Lit(literal) if matches!(literal.node, rustc_ast::LitKind::Int(value, _) if value.get() > 0)
    )
}

fn span_line_starts_with_attribute(source_map: &SourceMap, span: Span) -> bool {
    let location = source_map.lookup_char_pos(span.lo());
    location
        .file
        .get_line(location.line.saturating_sub(1))
        .is_some_and(|line| line.trim_start().starts_with("#["))
}

struct Sec011Visitor<'a, 'tcx> {
    source_map: &'a SourceMap,
    tcx: TyCtxt<'tcx>,
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    diagnostics: &'a mut Vec<RustcDiagnostic>,
    statically_bounded_bindings: HashSet<HirId>,
}

impl<'tcx> Visitor<'tcx> for Sec011Visitor<'_, 'tcx> {
    fn visit_local(&mut self, local: &'tcx LetStmt<'tcx>) {
        if local.init.is_some_and(is_literal_iteration_limit) {
            self.statically_bounded_bindings
                .extend(pattern_binding_ids(local.pat));
        }

        intravisit::walk_local(self, local);
    }

    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if let ExprKind::If(condition, then_branch, else_branch) = expr.kind {
            self.visit_expr(condition);
            let incoming = self.statically_bounded_bindings.clone();
            self.visit_expr(then_branch);
            let then_bindings = self.statically_bounded_bindings.clone();
            self.statically_bounded_bindings = incoming;
            if let Some(else_branch) = else_branch {
                self.visit_expr(else_branch);
            }
            self.statically_bounded_bindings
                .retain(|binding| then_bindings.contains(binding));
            return;
        }

        if let ExprKind::Match(scrutinee, arms, _) = expr.kind {
            self.visit_expr(scrutinee);
            let incoming = self.statically_bounded_bindings.clone();
            let mut arm_bindings: Option<HashSet<HirId>> = None;
            for arm in arms {
                self.statically_bounded_bindings = incoming.clone();
                self.visit_arm(arm);
                let current_bindings = self.statically_bounded_bindings.clone();
                arm_bindings = Some(match arm_bindings {
                    Some(bindings) => bindings.intersection(&current_bindings).copied().collect(),
                    None => current_bindings,
                });
            }
            self.statically_bounded_bindings = arm_bindings.unwrap_or(incoming);
            return;
        }

        if let ExprKind::Assign(target, value, _) = expr.kind {
            if let Some(binding) = local_binding_id(self.typeck, target) {
                if is_literal_iteration_limit(value) {
                    self.statically_bounded_bindings.insert(binding);
                } else {
                    self.statically_bounded_bindings.remove(&binding);
                }
            }
        }

        if let ExprKind::MethodCall(segment, receiver, args, _) = expr.kind {
            if segment.ident.name.as_str() == "take"
                && args.len() == 1
                && is_statically_bounded_iteration_limit(
                    self.typeck,
                    &args[0],
                    &self.statically_bounded_bindings,
                )
                && is_frame_storage_iteration_call(self.tcx, self.typeck, receiver)
            {
                self.visit_expr(&args[0]);
                return;
            }
        }

        if is_frame_storage_iteration_call(self.tcx, self.typeck, expr) {
            let (file, line, column) = span_location(self.source_map, expr.span);
            self.diagnostics.push(RustcDiagnostic {
                rule_id: "SEC011",
                rule_name: "storage-iteration-in-dispatchables",
                file,
                line,
                column,
                message: "Callable iterates a resolved FRAME storage collection".to_string(),
            });
        }

        intravisit::walk_expr(self, expr);
    }
}

fn is_statically_bounded_iteration_limit(
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    expr: &Expr<'_>,
    statically_bounded_bindings: &HashSet<HirId>,
) -> bool {
    is_literal_iteration_limit(expr)
        || local_binding_id(typeck, expr)
            .is_some_and(|binding| statically_bounded_bindings.contains(&binding))
}

fn is_frame_storage_iteration_call<'tcx>(
    tcx: TyCtxt<'tcx>,
    typeck: &rustc_middle::ty::TypeckResults<'tcx>,
    expr: &Expr<'tcx>,
) -> bool {
    let ExprKind::Call(callee, _) = expr.kind else {
        return false;
    };
    associated_call_name(callee).is_some_and(|name| name == "iter" || name == "drain")
        && is_frame_storage_associated_call(tcx, typeck, callee)
}

fn is_literal_iteration_limit(expr: &Expr<'_>) -> bool {
    matches!(strip_drop_temps(expr).kind, ExprKind::Lit(_))
}

struct Sec012Visitor<'a, 'tcx> {
    source_map: &'a SourceMap,
    tcx: TyCtxt<'tcx>,
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    diagnostics: &'a mut Vec<RustcDiagnostic>,
    unbounded_limit_bindings: HashSet<HirId>,
}

impl<'tcx> Visitor<'tcx> for Sec012Visitor<'_, 'tcx> {
    fn visit_local(&mut self, local: &'tcx LetStmt<'tcx>) {
        if local.init.is_some_and(is_unbounded_clear_prefix_limit) {
            self.unbounded_limit_bindings
                .extend(pattern_binding_ids(local.pat));
        }

        intravisit::walk_local(self, local);
    }

    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if let ExprKind::If(condition, then_branch, else_branch) = expr.kind {
            self.visit_expr(condition);
            let incoming = self.unbounded_limit_bindings.clone();
            self.visit_expr(then_branch);
            let then_bindings = self.unbounded_limit_bindings.clone();
            self.unbounded_limit_bindings = incoming;
            if let Some(else_branch) = else_branch {
                self.visit_expr(else_branch);
            }
            self.unbounded_limit_bindings.extend(then_bindings);
            return;
        }

        if let ExprKind::Match(scrutinee, arms, _) = expr.kind {
            self.visit_expr(scrutinee);
            let incoming = self.unbounded_limit_bindings.clone();
            let mut arm_bindings = HashSet::new();
            for arm in arms {
                self.unbounded_limit_bindings = incoming.clone();
                self.visit_arm(arm);
                arm_bindings.extend(self.unbounded_limit_bindings.iter().copied());
            }
            self.unbounded_limit_bindings = arm_bindings;
            return;
        }

        if let ExprKind::Assign(target, value, _) = expr.kind {
            if let Some(binding) = local_binding_id(self.typeck, target) {
                if is_unbounded_clear_prefix_limit(value) {
                    self.unbounded_limit_bindings.insert(binding);
                } else {
                    self.unbounded_limit_bindings.remove(&binding);
                }
            }
        }

        if let ExprKind::Call(callee, args) = expr.kind {
            if associated_call_name(callee).is_some_and(|name| name == "clear_prefix")
                && is_frame_storage_associated_call(self.tcx, self.typeck, callee)
                && args.get(1).is_some_and(|limit| {
                    is_unbounded_clear_prefix_limit(limit)
                        || local_binding_id(self.typeck, limit)
                            .is_some_and(|binding| self.unbounded_limit_bindings.contains(&binding))
                })
            {
                let (file, line, column) = span_location(self.source_map, expr.span);
                self.diagnostics.push(RustcDiagnostic {
                    rule_id: "SEC012",
                    rule_name: "unbounded-clear-prefix",
                    file,
                    line,
                    column,
                    message: "Resolved FRAME storage clear_prefix uses an unbounded deletion limit"
                        .to_string(),
                });
            }
        }

        intravisit::walk_expr(self, expr);
    }
}

fn returns_fallible<'tcx>(tcx: TyCtxt<'tcx>, def_id: LocalDefId) -> bool {
    let sig = tcx.fn_sig(def_id).instantiate_identity().skip_binder();
    type_is_fallible(tcx, sig.output())
}

fn is_reachable_entry_point(tcx: TyCtxt<'_>, def_id: LocalDefId) -> bool {
    let method_name = tcx.item_name(def_id.to_def_id());
    let name = method_name.as_str();
    if let Some(trait_id) = tcx
        .impl_of_method(def_id.to_def_id())
        .and_then(|impl_id| tcx.trait_id_of_impl(impl_id))
    {
        let trait_path = tcx.def_path_str(trait_id);
        return (is_frame_support_path(&trait_path)
            && trait_path.ends_with("::Hooks")
            && matches!(
                name,
                "on_initialize" | "on_finalize" | "on_idle" | "on_poll" | "on_runtime_upgrade"
            ))
            || (is_frame_support_path(&trait_path)
                && matches!(
                    trait_path.rsplit("::").next(),
                    Some("OnRuntimeUpgrade" | "UncheckedOnRuntimeUpgrade")
                )
                && name == "on_runtime_upgrade")
            || (is_frame_support_path(&trait_path)
                && trait_path.ends_with("::ChangeMembers")
                && name == "change_members_sorted")
            || (trait_path.contains("xcm_executor::traits")
                && trait_path.ends_with("::OnResponse")
                && name == "on_response");
    }

    tcx.local_visibility(def_id).is_public()
}

fn is_frame_lifecycle_hook(tcx: TyCtxt<'_>, def_id: LocalDefId) -> bool {
    let Some(trait_id) = tcx
        .impl_of_method(def_id.to_def_id())
        .and_then(|impl_id| tcx.trait_id_of_impl(impl_id))
    else {
        return false;
    };
    let trait_path = tcx.def_path_str(trait_id);
    is_frame_support_path(&trait_path)
        && trait_path.ends_with("::Hooks")
        && matches!(
            tcx.item_name(def_id.to_def_id()).as_str(),
            "on_initialize" | "on_finalize" | "on_idle" | "on_poll"
        )
}

fn is_frame_runtime_upgrade(tcx: TyCtxt<'_>, def_id: LocalDefId) -> bool {
    if tcx.item_name(def_id.to_def_id()).as_str() != "on_runtime_upgrade" {
        return false;
    }
    let Some(trait_id) = tcx
        .impl_of_method(def_id.to_def_id())
        .and_then(|impl_id| tcx.trait_id_of_impl(impl_id))
    else {
        return false;
    };
    let trait_path = tcx.def_path_str(trait_id);
    is_frame_support_path(&trait_path)
        && (trait_path.ends_with("::Hooks")
            || (trait_path.ends_with("::OnRuntimeUpgrade")
                && !trait_path.ends_with("::UncheckedOnRuntimeUpgrade")))
}

fn type_is_fallible(tcx: TyCtxt<'_>, ty: Ty<'_>) -> bool {
    match ty.kind() {
        TyKind::Adt(adt, _) => {
            let name = tcx.def_path_str(adt.did());
            name.ends_with("::Result") || name.contains("result::Result")
        }
        _ => false,
    }
}

fn debug_assert_call_site(source_map: &SourceMap, span: Span) -> Option<Span> {
    let mut call_site = span;
    for _ in 0..32 {
        if call_site.ctxt().is_root() {
            return None;
        }
        let expn_data = call_site.ctxt().outer_expn_data();
        let source_call_site = expn_data.call_site.source_callsite();
        if matches!(expn_data.kind, ExpnKind::Macro(_, name) if name.as_str() == "debug_assert")
            || source_map
                .span_to_snippet(source_call_site)
                .is_ok_and(|snippet| snippet.contains("debug_assert!"))
        {
            return Some(source_call_site);
        }
        call_site = expn_data.call_site;
    }
    None
}

fn is_unlimited_decode_call(expr: &Expr<'_>) -> bool {
    match expr.kind {
        ExprKind::Call(callee, _) => qpath_call_name(callee).is_some_and(|name| name == "decode"),
        ExprKind::MethodCall(segment, _, _, _) => segment.ident.name.as_str() == "decode",
        _ => false,
    }
}

fn decode_call_uses_tainted_input(
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    expr: &Expr<'_>,
    tainted_bindings: &HashSet<HirId>,
) -> bool {
    match expr.kind {
        ExprKind::Call(_, args) => args
            .iter()
            .any(|arg| expr_references_tainted_binding(typeck, arg, tainted_bindings)),
        ExprKind::MethodCall(_, receiver, args, _) => {
            expr_references_tainted_binding(typeck, receiver, tainted_bindings)
                || args
                    .iter()
                    .any(|arg| expr_references_tainted_binding(typeck, arg, tainted_bindings))
        }
        _ => false,
    }
}

fn expr_references_tainted_binding(
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    expr: &Expr<'_>,
    tainted_bindings: &HashSet<HirId>,
) -> bool {
    match expr.kind {
        ExprKind::Path(qpath) => {
            matches!(typeck.qpath_res(&qpath, expr.hir_id), Res::Local(hir_id) if tainted_bindings.contains(&hir_id))
        }
        ExprKind::AddrOf(_, _, inner)
        | ExprKind::DropTemps(inner)
        | ExprKind::Unary(_, inner)
        | ExprKind::Cast(inner, _) => {
            expr_references_tainted_binding(typeck, inner, tainted_bindings)
        }
        ExprKind::Field(inner, _) => {
            expr_references_tainted_binding(typeck, inner, tainted_bindings)
        }
        ExprKind::Index(receiver, index, _) => {
            expr_references_tainted_binding(typeck, receiver, tainted_bindings)
                || expr_references_tainted_binding(typeck, index, tainted_bindings)
        }
        ExprKind::Call(callee, args) => {
            expr_references_tainted_binding(typeck, callee, tainted_bindings)
                || args
                    .iter()
                    .any(|arg| expr_references_tainted_binding(typeck, arg, tainted_bindings))
        }
        ExprKind::MethodCall(_, receiver, args, _) => {
            expr_references_tainted_binding(typeck, receiver, tainted_bindings)
                || args
                    .iter()
                    .any(|arg| expr_references_tainted_binding(typeck, arg, tainted_bindings))
        }
        ExprKind::If(_, then_branch, else_branch) => {
            expr_references_tainted_binding(typeck, then_branch, tainted_bindings)
                || else_branch.is_some_and(|else_branch| {
                    expr_references_tainted_binding(typeck, else_branch, tainted_bindings)
                })
        }
        ExprKind::Match(scrutinee, arms, _) => arms.iter().any(|arm| {
            let mut arm_tainted_bindings = tainted_bindings.clone();
            arm_tainted_bindings.extend(tainted_pattern_binding_ids(
                typeck,
                scrutinee,
                arm,
                tainted_bindings,
            ));
            expr_references_tainted_binding(typeck, arm.body, &arm_tainted_bindings)
        }),
        ExprKind::Block(block, _) => block
            .expr
            .is_some_and(|tail| expr_references_tainted_binding(typeck, tail, tainted_bindings)),
        ExprKind::Tup(values) | ExprKind::Array(values) => values
            .iter()
            .any(|value| expr_references_tainted_binding(typeck, value, tainted_bindings)),
        _ => false,
    }
}

fn local_binding_id(
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    expr: &Expr<'_>,
) -> Option<HirId> {
    let expr = strip_drop_temps(expr);
    let ExprKind::Path(qpath) = expr.kind else {
        return None;
    };
    match typeck.qpath_res(&qpath, expr.hir_id) {
        Res::Local(hir_id) => Some(hir_id),
        _ => None,
    }
}

fn strip_drop_temps<'hir>(mut expr: &'hir Expr<'hir>) -> &'hir Expr<'hir> {
    while let ExprKind::DropTemps(inner) = expr.kind {
        expr = inner;
    }
    expr
}

fn expression_is_known_unwrappable(
    tcx: TyCtxt<'_>,
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    expr: &Expr<'_>,
    known_bindings: &HashSet<HirId>,
) -> bool {
    local_binding_id(typeck, expr).is_some_and(|binding| known_bindings.contains(&binding))
        || expression_constructs_known_unwrappable(tcx, typeck, expr)
}

fn expression_constructs_known_unwrappable(
    tcx: TyCtxt<'_>,
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    expr: &Expr<'_>,
) -> bool {
    let ExprKind::Call(callee, _) = expr.kind else {
        return false;
    };
    let Some(constructor) = qpath_call_name(callee) else {
        return false;
    };
    let TyKind::Adt(adt, _) = typeck.expr_ty(expr).kind() else {
        return false;
    };
    let type_path = tcx.def_path_str(adt.did());
    matches!(constructor.as_str(), "Some" | "Ok")
        && ((constructor == "Some" && type_path.ends_with("::Option"))
            || (constructor == "Ok"
                && (type_path.ends_with("::Result") || type_path.contains("result::Result"))))
}

fn type_is_option_or_result(tcx: TyCtxt<'_>, ty: Ty<'_>) -> bool {
    let TyKind::Adt(adt, _) = ty.kind() else {
        return false;
    };
    let type_path = tcx.def_path_str(adt.did());
    type_path.ends_with("::Option")
        || type_path.ends_with("::Result")
        || type_path.contains("option::Option")
        || type_path.contains("result::Result")
}

fn result_error_type<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> Option<Ty<'tcx>> {
    let TyKind::Adt(adt, args) = ty.kind() else {
        return None;
    };
    let type_path = tcx.def_path_str(adt.did());
    if !(type_path.ends_with("::Result") || type_path.contains("result::Result")) {
        return None;
    }
    args.iter()
        .filter_map(|arg| match arg.kind() {
            GenericArgKind::Type(arg_ty) => Some(arg_ty),
            _ => None,
        })
        .nth(1)
}

fn type_is_unit<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> bool {
    match ty.kind() {
        TyKind::Tuple(values) => values.is_empty(),
        TyKind::Alias(kind, alias_ty) => expand_alias_type(tcx, *kind, *alias_ty)
            .is_some_and(|expanded| type_is_unit(tcx, expanded)),
        _ => false,
    }
}

fn pattern_binding_ids(pattern: &Pat<'_>) -> Vec<HirId> {
    match pattern.kind {
        PatKind::Binding(_, hir_id, _, subpattern) => std::iter::once(hir_id)
            .chain(subpattern.into_iter().flat_map(pattern_binding_ids))
            .collect(),
        PatKind::Tuple(patterns, _) | PatKind::Or(patterns) => patterns
            .iter()
            .flat_map(|pattern| pattern_binding_ids(pattern))
            .collect(),
        PatKind::TupleStruct(_, patterns, _) => patterns
            .iter()
            .flat_map(|pattern| pattern_binding_ids(pattern))
            .collect(),
        PatKind::Struct(_, fields, _) => fields
            .iter()
            .flat_map(|field| pattern_binding_ids(field.pat))
            .collect(),
        PatKind::Box(inner) | PatKind::Deref(inner) | PatKind::Ref(inner, _) => {
            pattern_binding_ids(inner)
        }
        PatKind::Slice(before, middle, after) => before
            .iter()
            .chain(middle)
            .chain(after)
            .flat_map(|pattern| pattern_binding_ids(pattern))
            .collect(),
        _ => Vec::new(),
    }
}

fn tainted_pattern_binding_ids(
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    source: &Expr<'_>,
    arm: &Arm<'_>,
    tainted_bindings: &HashSet<HirId>,
) -> Vec<HirId> {
    match (source.kind, arm.pat.kind) {
        (ExprKind::Tup(values), PatKind::Tuple(patterns, _)) => values
            .iter()
            .zip(patterns.iter())
            .flat_map(|(value, pattern)| {
                tainted_pattern_binding_ids_for_value(typeck, value, pattern, tainted_bindings)
            })
            .collect(),
        _ => tainted_pattern_binding_ids_for_value(typeck, source, arm.pat, tainted_bindings),
    }
}

fn tainted_pattern_binding_ids_for_value(
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    value: &Expr<'_>,
    pattern: &Pat<'_>,
    tainted_bindings: &HashSet<HirId>,
) -> Vec<HirId> {
    expr_references_tainted_binding(typeck, value, tainted_bindings)
        .then(|| pattern_binding_ids(pattern))
        .unwrap_or_default()
}

fn decode_receiver_contains_recursive_target<'tcx>(
    tcx: TyCtxt<'tcx>,
    typeck: &rustc_middle::ty::TypeckResults<'tcx>,
    expr: &'tcx Expr<'tcx>,
) -> bool {
    decode_receiver_type(typeck, expr)
        .is_some_and(|ty| type_contains_recursive_decode_target(tcx, ty))
}

fn decode_receiver_type<'tcx>(
    typeck: &rustc_middle::ty::TypeckResults<'tcx>,
    expr: &'tcx Expr<'tcx>,
) -> Option<Ty<'tcx>> {
    match expr.kind {
        ExprKind::Call(callee, _) => {
            let ExprKind::Path(QPath::TypeRelative(ty, _)) = callee.kind else {
                return None;
            };
            Some(typeck.node_type(ty.hir_id))
        }
        ExprKind::MethodCall(_, receiver, _, _) => Some(typeck.expr_ty(receiver)),
        _ => None,
    }
}

fn qpath_call_name(expr: &Expr<'_>) -> Option<String> {
    let ExprKind::Path(qpath) = expr.kind else {
        return None;
    };
    qpath_last_segment(qpath).map(|segment| segment.ident.name.to_string())
}

fn qpath_last_segment<'tcx>(qpath: QPath<'tcx>) -> Option<&'tcx rustc_hir::PathSegment<'tcx>> {
    match qpath {
        QPath::Resolved(_, path) => path.segments.last(),
        QPath::TypeRelative(_, segment) => Some(segment),
        QPath::LangItem(_, _) => None,
    }
}

fn associated_call_name(expr: &Expr<'_>) -> Option<String> {
    let ExprKind::Path(qpath) = expr.kind else {
        return None;
    };
    match qpath {
        QPath::TypeRelative(_, segment) => Some(segment.ident.name.to_string()),
        QPath::Resolved(_, path) => path
            .segments
            .last()
            .map(|segment| segment.ident.name.to_string()),
        QPath::LangItem(_, _) => None,
    }
}

fn is_frame_storage_associated_call<'tcx>(
    tcx: TyCtxt<'tcx>,
    typeck: &rustc_middle::ty::TypeckResults<'tcx>,
    callee: &'tcx Expr<'tcx>,
) -> bool {
    associated_call_receiver_type(typeck, callee)
        .is_some_and(|ty| type_is_frame_storage_owner(tcx, ty))
        || matches!(callee.kind, ExprKind::Path(qpath) if matches!(typeck.qpath_res(&qpath, callee.hir_id), Res::Def(_, def_id) if matches_frame_storage_method_owner_path(&tcx.def_path_str(def_id))))
}

fn associated_call_receiver_type<'tcx>(
    typeck: &rustc_middle::ty::TypeckResults<'tcx>,
    expr: &'tcx Expr<'tcx>,
) -> Option<Ty<'tcx>> {
    let ExprKind::Path(QPath::TypeRelative(ty, _)) = expr.kind else {
        return None;
    };
    Some(typeck.node_type(ty.hir_id))
}

fn expand_alias_type<'tcx>(
    tcx: TyCtxt<'tcx>,
    kind: AliasTyKind,
    alias_ty: rustc_middle::ty::AliasTy<'tcx>,
) -> Option<Ty<'tcx>> {
    matches!(kind, AliasTyKind::Free | AliasTyKind::Opaque)
        .then(|| tcx.type_of(alias_ty.def_id).instantiate(tcx, alias_ty.args))
}

fn type_contains_recursive_decode_target<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> bool {
    type_contains_recursive_decode_target_inner(tcx, ty, &mut HashSet::new())
}

fn type_contains_recursive_decode_target_inner<'tcx>(
    tcx: TyCtxt<'tcx>,
    ty: Ty<'tcx>,
    adt_stack: &mut HashSet<DefId>,
) -> bool {
    match ty.kind() {
        TyKind::Adt(adt, args) => {
            let name = tcx.def_path_str(adt.did());
            if is_recursive_decode_target_name(&name) || !adt_stack.insert(adt.did()) {
                return true;
            }
            let recursive = args.iter().any(|arg| match arg.kind() {
                GenericArgKind::Type(arg_ty) => {
                    type_contains_recursive_decode_target_inner(tcx, arg_ty, adt_stack)
                }
                _ => false,
            }) || adt.all_fields().any(|field| {
                type_contains_recursive_decode_target_inner(tcx, field.ty(tcx, args), adt_stack)
            });
            adt_stack.remove(&adt.did());
            recursive
        }
        TyKind::Alias(kind, alias_ty) => {
            is_recursive_decode_target_name(&tcx.def_path_str(alias_ty.def_id))
                || expand_alias_type(tcx, *kind, *alias_ty).is_some_and(|expanded| {
                    type_contains_recursive_decode_target_inner(tcx, expanded, adt_stack)
                })
        }
        TyKind::Ref(_, inner, _) => {
            type_contains_recursive_decode_target_inner(tcx, *inner, adt_stack)
        }
        TyKind::Array(inner, _) | TyKind::Slice(inner) => {
            type_contains_recursive_decode_target_inner(tcx, *inner, adt_stack)
        }
        TyKind::Tuple(types) => types
            .iter()
            .any(|inner| type_contains_recursive_decode_target_inner(tcx, inner, adt_stack)),
        _ => false,
    }
}

fn type_contains_unbounded_vec<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> bool {
    match ty.kind() {
        TyKind::Adt(adt, args) => {
            let name = tcx.def_path_str(adt.did());
            is_vec_type_name(&name)
                || args.iter().any(|arg| match arg.kind() {
                    GenericArgKind::Type(arg_ty) => type_contains_unbounded_vec(tcx, arg_ty),
                    _ => false,
                })
        }
        TyKind::Alias(kind, alias_ty) => expand_alias_type(tcx, *kind, *alias_ty)
            .is_some_and(|expanded| type_contains_unbounded_vec(tcx, expanded)),
        TyKind::Ref(_, inner, _) => type_contains_unbounded_vec(tcx, *inner),
        TyKind::Slice(_) => false,
        TyKind::Array(inner, _) => type_contains_unbounded_vec(tcx, *inner),
        TyKind::Tuple(types) => types
            .iter()
            .any(|inner| type_contains_unbounded_vec(tcx, inner)),
        _ => false,
    }
}

fn type_contains_unbounded_event_vec<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> bool {
    match ty.kind() {
        TyKind::Adt(adt, args) => {
            let name = tcx.def_path_str(adt.did());
            if is_bounded_vec_type_name(&name) {
                return false;
            }
            is_vec_type_name(&name)
                || args.iter().any(|arg| match arg.kind() {
                    GenericArgKind::Type(arg_ty) => type_contains_unbounded_event_vec(tcx, arg_ty),
                    _ => false,
                })
        }
        TyKind::Alias(kind, alias_ty) => expand_alias_type(tcx, *kind, *alias_ty)
            .is_some_and(|expanded| type_contains_unbounded_event_vec(tcx, expanded)),
        TyKind::Ref(_, inner, _) => type_contains_unbounded_event_vec(tcx, *inner),
        TyKind::Slice(_) => false,
        TyKind::Array(inner, _) => type_contains_unbounded_event_vec(tcx, *inner),
        TyKind::Tuple(types) => types
            .iter()
            .any(|inner| type_contains_unbounded_event_vec(tcx, inner)),
        _ => false,
    }
}

fn type_contains_unbounded_storage_collection<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> bool {
    match ty.kind() {
        TyKind::Adt(adt, args) => {
            let name = tcx.def_path_str(adt.did());
            if is_bounded_storage_collection_type_name(&name) {
                return false;
            }
            is_unbounded_storage_collection_type_name(&name)
                || args.iter().any(|arg| match arg.kind() {
                    GenericArgKind::Type(arg_ty) => {
                        type_contains_unbounded_storage_collection(tcx, arg_ty)
                    }
                    _ => false,
                })
        }
        TyKind::Alias(kind, alias_ty) => expand_alias_type(tcx, *kind, *alias_ty)
            .is_some_and(|expanded| type_contains_unbounded_storage_collection(tcx, expanded)),
        TyKind::Ref(_, inner, _) => type_contains_unbounded_storage_collection(tcx, *inner),
        TyKind::Slice(_) => false,
        TyKind::Array(inner, _) => type_contains_unbounded_storage_collection(tcx, *inner),
        TyKind::Tuple(types) => types
            .iter()
            .any(|inner| type_contains_unbounded_storage_collection(tcx, inner)),
        _ => false,
    }
}

fn is_vec_type_name(name: &str) -> bool {
    matches!(name, "Vec") || name.ends_with("::Vec")
}

fn is_bounded_vec_type_name(name: &str) -> bool {
    matches!(name, "BoundedVec" | "WeakBoundedVec")
        || name.ends_with("::BoundedVec")
        || name.ends_with("::WeakBoundedVec")
}

fn is_unbounded_storage_collection_type_name(name: &str) -> bool {
    is_vec_type_name(name)
        || matches!(name, "BTreeMap" | "BTreeSet" | "VecDeque")
        || name.ends_with("::BTreeMap")
        || name.ends_with("::BTreeSet")
        || name.ends_with("::VecDeque")
}

fn is_bounded_storage_collection_type_name(name: &str) -> bool {
    is_bounded_vec_type_name(name)
        || matches!(name, "BoundedBTreeMap" | "BoundedBTreeSet")
        || name.ends_with("::BoundedBTreeMap")
        || name.ends_with("::BoundedBTreeSet")
}

fn type_is_frame_storage_owner<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> bool {
    match ty.kind() {
        TyKind::Adt(adt, _) => {
            let name = tcx.def_path_str(adt.did());
            matches_frame_storage_owner_name(&name)
        }
        TyKind::Alias(kind, alias_ty) => expand_alias_type(tcx, *kind, *alias_ty)
            .is_some_and(|expanded| type_is_frame_storage_owner(tcx, expanded)),
        _ => false,
    }
}

fn matches_frame_storage_owner_name(name: &str) -> bool {
    if !is_frame_support_path(name) {
        return false;
    }

    [
        "CountedStorageMap",
        "StorageDoubleMap",
        "StorageMap",
        "StorageNMap",
        "StorageValue",
    ]
    .iter()
    .any(|owner| name == *owner || name.ends_with(&format!("::{owner}")))
}

fn matches_frame_storage_method_owner_path(name: &str) -> bool {
    is_frame_support_path(name)
        && [
            "CountedStorageMap",
            "IterableStorageDoubleMap",
            "IterableStorageMap",
            "IterableStorageNMap",
            "StorageDoubleMap",
            "StorageList",
            "StorageMap",
            "StorageNMap",
            "StorageValue",
        ]
        .iter()
        .any(|owner| name.contains(&format!("::{owner}::")))
}

fn is_frame_support_path(name: &str) -> bool {
    name.contains("frame_support::")
        || name.contains("polkadot_sdk_frame::")
        || name.starts_with("frame::")
        || name.contains("::frame::")
}

fn has_hir_attr(tcx: TyCtxt<'_>, hir_id: rustc_hir::HirId, path: &[&str]) -> bool {
    let symbols = path
        .iter()
        .map(|segment| Symbol::intern(segment))
        .collect::<Vec<_>>();
    tcx.hir_attrs(hir_id)
        .iter()
        .any(|attr| attr.path_matches(&symbols))
}

fn body_param_name(param: &rustc_hir::Param<'_>) -> Option<String> {
    match param.pat.kind {
        PatKind::Binding(_, _, ident, _) => Some(ident.name.to_string()),
        PatKind::Ref(inner, _) => match inner.kind {
            PatKind::Binding(_, _, ident, _) => Some(ident.name.to_string()),
            _ => None,
        },
        _ => None,
    }
}

fn weight_accounts_for_param(weight_expression: &SynExpr, param_name: &str) -> bool {
    struct WeightParamVisitor<'a> {
        param_name: &'a str,
        found: bool,
    }

    impl<'ast> SynVisit<'ast> for WeightParamVisitor<'_> {
        fn visit_expr_method_call(&mut self, node: &'ast SynExprMethodCall) {
            if matches!(
                node.method.to_string().as_str(),
                "len" | "encoded_size" | "using_encoded"
            ) && syn_expr_root_ident(&node.receiver).as_deref() == Some(self.param_name)
            {
                self.found = true;
            }
            visit::visit_expr_method_call(self, node);
        }

        fn visit_expr_call(&mut self, node: &'ast SynExprCall) {
            if syn_expr_last_path_segment(&node.func).as_deref() == Some("encoded_size")
                && node.args.iter().any(|argument| {
                    syn_expr_root_ident(argument).as_deref() == Some(self.param_name)
                })
            {
                self.found = true;
            }
            visit::visit_expr_call(self, node);
        }
    }

    let mut visitor = WeightParamVisitor {
        param_name,
        found: false,
    };
    visitor.visit_expr(weight_expression);
    visitor.found
}

fn syn_expr_root_ident(expr: &SynExpr) -> Option<String> {
    match expr {
        SynExpr::Path(path) if path.qself.is_none() && path.path.segments.len() == 1 => path
            .path
            .segments
            .first()
            .map(|segment| segment.ident.to_string()),
        SynExpr::Reference(reference) => syn_expr_root_ident(&reference.expr),
        SynExpr::Paren(paren) => syn_expr_root_ident(&paren.expr),
        SynExpr::Group(group) => syn_expr_root_ident(&group.expr),
        SynExpr::Field(field) => syn_expr_root_ident(&field.base),
        SynExpr::MethodCall(method_call) => syn_expr_root_ident(&method_call.receiver),
        SynExpr::Cast(cast) => syn_expr_root_ident(&cast.expr),
        SynExpr::Index(index) => syn_expr_root_ident(&index.expr),
        _ => None,
    }
}

fn syn_expr_last_path_segment(expr: &SynExpr) -> Option<String> {
    match expr {
        SynExpr::Path(path) if path.qself.is_none() => path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string()),
        SynExpr::Paren(paren) => syn_expr_last_path_segment(&paren.expr),
        SynExpr::Group(group) => syn_expr_last_path_segment(&group.expr),
        _ => None,
    }
}

fn is_unbounded_clear_prefix_limit(expr: &Expr<'_>) -> bool {
    match expr.kind {
        ExprKind::Path(qpath) => qpath_last_segment(qpath).is_some_and(|segment| {
            let name = segment.ident.name.as_str();
            name == "None" || name == "MAX"
        }),
        ExprKind::Call(callee, args) => {
            associated_call_name(callee)
                .or_else(|| qpath_call_name(callee))
                .is_some_and(|name| name == "Some")
                && args
                    .first()
                    .is_some_and(|inner| is_unbounded_clear_prefix_limit(inner))
        }
        _ => false,
    }
}

fn receiver_is_result_with_uninhabited_error<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> bool {
    let TyKind::Adt(adt, args) = ty.kind() else {
        return false;
    };
    let name = tcx.def_path_str(adt.did());
    if !(name.ends_with("::Result") || name.contains("result::Result")) {
        return false;
    }
    args.iter()
        .filter_map(|arg| match arg.kind() {
            GenericArgKind::Type(arg_ty) => Some(arg_ty),
            _ => None,
        })
        .nth(1)
        .is_some_and(|err_ty| type_is_uninhabited(tcx, err_ty))
}

fn type_is_uninhabited<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> bool {
    match ty.kind() {
        TyKind::Never => true,
        TyKind::Adt(adt, _) => {
            let name = tcx.def_path_str(adt.did());
            matches!(name.as_str(), "Infallible")
                || name.ends_with("::Infallible")
                || adt.variants().is_empty()
        }
        TyKind::Alias(kind, alias_ty) => expand_alias_type(tcx, *kind, *alias_ty)
            .is_some_and(|expanded| type_is_uninhabited(tcx, expanded)),
        _ => false,
    }
}

fn is_recursive_decode_target_name(name: &str) -> bool {
    matches!(
        name,
        "RuntimeCall" | "UncheckedExtrinsic" | "OpaqueExtrinsic"
    ) || name.ends_with("::RuntimeCall")
        || name.ends_with("::UncheckedExtrinsic")
        || name.ends_with("::OpaqueExtrinsic")
}

fn is_raw_arithmetic(op: BinOpKind) -> bool {
    matches!(
        op,
        BinOpKind::Add | BinOpKind::Sub | BinOpKind::Mul | BinOpKind::Div | BinOpKind::Rem
    )
}

fn is_integral(ty: Ty<'_>) -> bool {
    matches!(
        ty.kind(),
        TyKind::Int(_) | TyKind::Uint(_) | TyKind::Infer(rustc_middle::ty::IntVar(_))
    )
}

fn type_is_runtime_db_weight<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> bool {
    match ty.kind() {
        TyKind::Adt(adt, _) => {
            let path = tcx.def_path_str(adt.did());
            path.ends_with("::RuntimeDbWeight")
                && (path.contains("sp_weights::") || is_frame_support_path(&path))
        }
        TyKind::Alias(kind, alias_ty) => expand_alias_type(tcx, *kind, *alias_ty)
            .is_some_and(|expanded| type_is_runtime_db_weight(tcx, expanded)),
        _ => false,
    }
}

fn is_generated_weights_file(path: &str) -> bool {
    path.ends_with("weights.rs") || path.contains("/weights/") || path.contains("\\weights\\")
}

fn span_location(source_map: &SourceMap, span: Span) -> (String, usize, usize) {
    let location = source_map.lookup_char_pos(span.lo());
    (
        location.file.name.prefer_local().to_string(),
        location.line,
        location.col_display + 1,
    )
}

fn first_arg_is_rustc(args: &[String]) -> bool {
    args.first()
        .and_then(|arg| Path::new(arg).file_name())
        .and_then(|file_name| file_name.to_str())
        .is_some_and(|file_name| file_name == "rustc" || file_name.starts_with("rustc-"))
}

fn append_jsonl_diagnostics(path: &str, diagnostics: &[RustcDiagnostic]) {
    if diagnostics.is_empty() {
        return;
    }

    let mut output = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("rustc diagnostics output should be writable");

    for diagnostic in diagnostics {
        writeln!(
            output,
            "{}",
            serde_json::to_string(diagnostic).expect("rustc diagnostic should serialize")
        )
        .expect("rustc diagnostic should be written");
    }
}

fn output_file_filters() -> Vec<String> {
    env::var("POLKADOT_LINTER_DRIVER_FILE_CONTAINS")
        .ok()
        .into_iter()
        .flat_map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect()
}

fn enabled_rule_filters() -> HashSet<String> {
    env::var("POLKADOT_LINTER_DRIVER_RULES")
        .ok()
        .into_iter()
        .flat_map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect()
}

fn filtered_unique_diagnostics(
    diagnostics: &[RustcDiagnostic],
    file_filters: &[String],
) -> Vec<RustcDiagnostic> {
    let mut filtered = diagnostics
        .iter()
        .filter(|diagnostic| source_matches_filters(&diagnostic.file, file_filters))
        .cloned()
        .collect::<Vec<_>>();

    filtered.sort_by(|a, b| {
        a.rule_id
            .cmp(b.rule_id)
            .then(a.file.cmp(&b.file))
            .then(a.line.cmp(&b.line))
            .then(a.column.cmp(&b.column))
            .then(a.message.cmp(&b.message))
    });
    filtered.dedup_by(|a, b| {
        a.rule_id == b.rule_id
            && a.file == b.file
            && a.line == b.line
            && a.column == b.column
            && a.message == b.message
    });
    filtered
}

fn main() {
    let mut rustc_args = env::args().skip(1).collect::<Vec<_>>();
    if rustc_args.is_empty() {
        eprintln!("usage: polkadot-linter-driver <rustc args>");
        process::exit(2);
    }
    let wrapper_mode = first_arg_is_rustc(&rustc_args);
    if !wrapper_mode {
        rustc_args.insert(0, "rustc".to_string());
    }
    if !rustc_args.iter().any(|arg| arg == "--crate-name") {
        rustc_args.push("--crate-name".to_string());
        rustc_args.push("lint_target".to_string());
    }
    if !rustc_args.iter().any(|arg| arg == "--error-format=json") {
        rustc_args.push("--error-format=json".to_string());
    }

    let mut callbacks = PolkadotCallbacks {
        continue_compilation: wrapper_mode,
        enabled_rules: enabled_rule_filters(),
        ..PolkadotCallbacks::default()
    };
    let result = rustc_driver::catch_with_exit_code(|| {
        rustc_driver::run_compiler(&rustc_args, &mut callbacks);
    });
    let diagnostics = filtered_unique_diagnostics(&callbacks.diagnostics, &output_file_filters());

    if let Ok(path) = env::var("POLKADOT_LINTER_DRIVER_JSONL") {
        append_jsonl_diagnostics(&path, &diagnostics);
    } else if !wrapper_mode {
        println!(
            "{}",
            serde_json::to_string_pretty(&diagnostics).expect("rustc diagnostics should serialize")
        );
    }
    process::exit(result);
}
