#![feature(rustc_private)]

extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_span;

use std::{collections::HashSet, env, fs::OpenOptions, io::Write, path::Path, process};

use rustc_driver::{Callbacks, Compilation};
use rustc_hir::{
    def::Res,
    def_id::LocalDefId,
    intravisit::{self, Visitor},
    Arm, BinOpKind, BodyOwnerKind, Expr, ExprKind, HirId, ItemKind, LetStmt, Pat, PatKind, QPath,
};
use rustc_middle::ty::{AliasTyKind, GenericArgKind, Ty, TyCtxt, TyKind};
use rustc_span::{hygiene::ExpnKind, source_map::SourceMap, Span, Symbol};
use serde::Serialize;

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
}

impl Callbacks for PolkadotCallbacks {
    fn after_analysis<'tcx>(
        &mut self,
        _compiler: &rustc_interface::interface::Compiler,
        tcx: TyCtxt<'tcx>,
    ) -> Compilation {
        if self.rule_enabled("SEC013") {
            report_unbounded_storage_aliases(tcx, tcx.sess.source_map(), &mut self.diagnostics);
        }
        if self.rule_enabled("SEC017") {
            report_vec_event_fields(tcx, tcx.sess.source_map(), &mut self.diagnostics);
        }
        let reachable_entry_point_bodies = (self.rule_enabled("SEC002")
            || self.rule_enabled("SEC008"))
        .then(|| reachable_local_function_bodies(tcx, false))
        .unwrap_or_default();
        let reachable_fallible_entry_point_bodies = self
            .rule_enabled("SEC009")
            .then(|| reachable_local_function_bodies(tcx, true))
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
                    &mut self.diagnostics,
                );
            }

            if matches!(body_owner_kind, BodyOwnerKind::Fn) && self.rule_enabled("SEC001") {
                report_unbounded_public_vec_inputs(
                    tcx,
                    def_id,
                    body,
                    tcx.sess.source_map(),
                    &mut self.diagnostics,
                );
            }

            if matches!(body_owner_kind, BodyOwnerKind::Fn) && self.rule_enabled("SEC003") {
                let mut decode_visitor = Sec003Visitor {
                    source_map: tcx.sess.source_map(),
                    tcx,
                    typeck,
                    diagnostics: &mut self.diagnostics,
                    tainted_bindings: body
                        .params
                        .iter()
                        .flat_map(|param| pattern_binding_ids(param.pat))
                        .collect(),
                };
                decode_visitor.visit_body(body);
            }

            if matches!(body_owner_kind, BodyOwnerKind::Fn)
                && self.rule_enabled("SEC011")
                && is_public_or_hook(tcx, def_id)
            {
                let mut storage_iteration_visitor = Sec011Visitor {
                    source_map: tcx.sess.source_map(),
                    tcx,
                    typeck,
                    diagnostics: &mut self.diagnostics,
                };
                storage_iteration_visitor.visit_body(body);
            }

            if matches!(body_owner_kind, BodyOwnerKind::Fn) && self.rule_enabled("SEC012") {
                let mut clear_prefix_visitor = Sec012Visitor {
                    source_map: tcx.sess.source_map(),
                    tcx,
                    typeck,
                    diagnostics: &mut self.diagnostics,
                };
                clear_prefix_visitor.visit_body(body);
            }
        }

        if self.continue_compilation {
            Compilation::Continue
        } else {
            Compilation::Stop
        }
    }
}

fn reachable_local_function_bodies(
    tcx: TyCtxt<'_>,
    fallible_entry_points_only: bool,
) -> Vec<LocalDefId> {
    let mut pending = tcx
        .hir_body_owners()
        .filter(|def_id| {
            matches!(tcx.hir_body_owner_kind(*def_id), BodyOwnerKind::Fn)
                && is_public_or_hook(tcx, *def_id)
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
        };
        callee_visitor.visit_body(body);
        pending.extend(callee_visitor.callees);
        reachable.push(def_id);
    }

    reachable
}

fn report_reachable_debug_assertions<'tcx>(
    tcx: TyCtxt<'tcx>,
    source_map: &SourceMap,
    reachable_bodies: &[LocalDefId],
    diagnostics: &mut Vec<RustcDiagnostic>,
) {
    let mut debug_assert_visitor = Sec002Visitor {
        source_map,
        diagnostics,
        reported_lines: HashSet::new(),
    };

    for def_id in reachable_bodies {
        if let Some(body) = tcx.hir_maybe_body_owned_by(*def_id) {
            debug_assert_visitor.visit_body(body);
        }
    }
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
        };
        panic_visitor.visit_body(body);
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
            typeck: tcx.typeck(*def_id),
            diagnostics,
            reported_lines: &mut reported_lines,
        };
        visitor.visit_body(body);
    }
}

impl PolkadotCallbacks {
    fn rule_enabled(&self, rule_id: &str) -> bool {
        self.enabled_rules.is_empty()
            || self.enabled_rules.iter().any(|enabled| {
                enabled == "SEC" || rule_id == enabled || rule_id.starts_with(enabled)
            })
    }
}

fn report_unbounded_storage_aliases<'tcx>(
    tcx: TyCtxt<'tcx>,
    source_map: &SourceMap,
    diagnostics: &mut Vec<RustcDiagnostic>,
) {
    for item_id in tcx.hir_free_items() {
        let item = tcx.hir_item(item_id);
        if !matches!(item.kind, ItemKind::TyAlias(..))
            || !is_frame_storage_alias(tcx, item.owner_id.def_id, item.hir_id(), source_map)
            || is_explicitly_unbounded_storage(tcx, item.owner_id.def_id, item.hir_id(), source_map)
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
) -> bool {
    has_hir_attr(tcx, hir_id, &["pallet", "storage"])
        || source_prefix_before_definition(tcx, def_id, source_map)
            .is_some_and(|prefix| prefix.contains("#[pallet::storage]"))
}

fn is_explicitly_unbounded_storage(
    tcx: TyCtxt<'_>,
    def_id: LocalDefId,
    hir_id: rustc_hir::HirId,
    source_map: &SourceMap,
) -> bool {
    has_hir_attr(tcx, hir_id, &["pallet", "unbounded"])
        || source_prefix_before_definition(tcx, def_id, source_map)
            .is_some_and(|prefix| prefix.contains("#[pallet::unbounded]"))
}

fn report_vec_event_fields<'tcx>(
    tcx: TyCtxt<'tcx>,
    source_map: &SourceMap,
    diagnostics: &mut Vec<RustcDiagnostic>,
) {
    for item_id in tcx.hir_free_items() {
        let item = tcx.hir_item(item_id);
        let ItemKind::Enum(_, _, enum_def) = item.kind else {
            continue;
        };
        if !is_frame_event(
            tcx,
            item.owner_id.def_id,
            item.hir_id(),
            item.span,
            source_map,
        ) {
            continue;
        }

        for variant in enum_def.variants {
            for field in variant.data.fields() {
                let field_ty = tcx.type_of(field.def_id).instantiate_identity();
                if type_contains_unbounded_event_vec(tcx, field_ty) {
                    let (file, line, column) = span_location(source_map, field.span);
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
        }
    }
}

fn is_frame_event(
    tcx: TyCtxt<'_>,
    def_id: LocalDefId,
    hir_id: rustc_hir::HirId,
    span: Span,
    source_map: &SourceMap,
) -> bool {
    if has_hir_attr(tcx, hir_id, &["pallet", "event"]) {
        return true;
    }

    let location = source_map.lookup_char_pos(span.source_callsite().lo());
    let source_path = location.file.name.prefer_local().to_string();
    let Ok(source) = std::fs::read_to_string(source_path) else {
        return false;
    };
    let lines = source.lines().collect::<Vec<_>>();
    let definition_line = location.line.saturating_sub(1).min(lines.len());
    let start_line = definition_line.saturating_sub(64);
    let context = lines[start_line..=definition_line].join("\n");

    context.contains("#[pallet::event]")
        && tcx.def_path_str(def_id.to_def_id()).ends_with("::Event")
}

fn report_unbounded_public_vec_inputs<'tcx>(
    tcx: TyCtxt<'tcx>,
    def_id: LocalDefId,
    body: &'tcx rustc_hir::Body<'tcx>,
    source_map: &SourceMap,
    diagnostics: &mut Vec<RustcDiagnostic>,
) {
    if !tcx.local_visibility(def_id).is_public() || !is_frame_dispatchable(tcx, def_id, source_map)
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

fn report_missing_weight_for_unbounded_inputs<'tcx>(
    tcx: TyCtxt<'tcx>,
    def_id: LocalDefId,
    body: &'tcx rustc_hir::Body<'tcx>,
    source_map: &SourceMap,
    diagnostics: &mut Vec<RustcDiagnostic>,
) {
    if !tcx.local_visibility(def_id).is_public() {
        return;
    }

    let Some(weight_attributes) = pallet_weight_attributes(tcx, def_id, source_map) else {
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
        if weight_accounts_for_param(&weight_attributes.snippet, &param_name) {
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
    snippet: String,
    deprecated: bool,
}

fn pallet_weight_attributes(
    tcx: TyCtxt<'_>,
    def_id: LocalDefId,
    source_map: &SourceMap,
) -> Option<PalletWeightAttributes> {
    let prefix = source_prefix_before_definition(tcx, def_id, source_map)?;
    let start = prefix.rfind("#[pallet::weight")?;
    let attribute_block = &prefix[start..];
    let attribute = balanced_attribute(attribute_block)?;

    attribute
        .strip_prefix("#[pallet::weight")
        .map(|_| PalletWeightAttributes {
            snippet: attribute.chars().filter(|ch| !ch.is_whitespace()).collect(),
            deprecated: attribute_block.contains("#[deprecated"),
        })
}

fn is_frame_dispatchable(tcx: TyCtxt<'_>, def_id: LocalDefId, source_map: &SourceMap) -> bool {
    let Some(prefix) = source_prefix_before_definition(tcx, def_id, source_map) else {
        return false;
    };
    let Some(call_index_start) = prefix.rfind("#[pallet::call_index") else {
        return false;
    };
    let attribute_block = &prefix[call_index_start..];

    attribute_block.contains("#[pallet::weight") && !attribute_block.contains("\npub fn ")
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
    reported_lines: HashSet<(String, usize)>,
}

impl<'tcx> Visitor<'tcx> for Sec002Visitor<'_> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if let Some(call_site) = macro_call_site(expr.span, "debug_assert") {
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
            for arm in arms {
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
            }
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

struct Sec008Visitor<'a, 'tcx> {
    source_map: &'a SourceMap,
    tcx: TyCtxt<'tcx>,
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    diagnostics: &'a mut Vec<RustcDiagnostic>,
    reported_lines: &'a mut HashSet<(String, usize)>,
}

impl<'tcx> Visitor<'tcx> for Sec008Visitor<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if let ExprKind::MethodCall(segment, receiver, _, _) = expr.kind {
            let method = segment.ident.name.as_str();
            if matches!(method, "unwrap" | "expect")
                && !receiver_is_result_with_uninhabited_error(
                    self.tcx,
                    self.typeck.expr_ty(receiver),
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
}

struct LocalCalleeVisitor<'a, 'tcx> {
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    callees: HashSet<LocalDefId>,
}

impl<'tcx> Visitor<'tcx> for LocalCalleeVisitor<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        match expr.kind {
            ExprKind::Call(callee, _) => self.record_callee(callee),
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
}

impl LocalCalleeVisitor<'_, '_> {
    fn record_callee(&mut self, callee: &Expr<'_>) {
        let ExprKind::Path(qpath) = callee.kind else {
            return;
        };
        if let Res::Def(_, def_id) = self.typeck.qpath_res(&qpath, callee.hir_id) {
            if let Some(local_def_id) = def_id.as_local() {
                self.callees.insert(local_def_id);
            }
        }
    }
}

struct Sec009Visitor<'a, 'tcx> {
    source_map: &'a SourceMap,
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    diagnostics: &'a mut Vec<RustcDiagnostic>,
    reported_lines: &'a mut HashSet<(String, usize)>,
}

impl<'tcx> Visitor<'tcx> for Sec009Visitor<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if let ExprKind::Binary(op, lhs, rhs) = expr.kind {
            let (file, line, column) = span_location(self.source_map, expr.span);
            if !span_line_starts_with_attribute(self.source_map, expr.span)
                && is_raw_arithmetic(op.node)
                && is_integral(self.typeck.expr_ty(lhs))
                && is_integral(self.typeck.expr_ty(rhs))
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
}

impl<'tcx> Visitor<'tcx> for Sec011Visitor<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if let ExprKind::Call(callee, _) = expr.kind {
            if associated_call_name(callee).is_some_and(|name| name == "iter" || name == "drain")
                && associated_call_receiver_type(self.typeck, callee)
                    .is_some_and(|ty| type_is_frame_storage_owner(self.tcx, ty))
            {
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
        }

        intravisit::walk_expr(self, expr);
    }
}

struct Sec012Visitor<'a, 'tcx> {
    source_map: &'a SourceMap,
    tcx: TyCtxt<'tcx>,
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    diagnostics: &'a mut Vec<RustcDiagnostic>,
}

impl<'tcx> Visitor<'tcx> for Sec012Visitor<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if let ExprKind::Call(callee, args) = expr.kind {
            if associated_call_name(callee).is_some_and(|name| name == "clear_prefix")
                && associated_call_receiver_type(self.typeck, callee)
                    .is_some_and(|ty| type_is_frame_storage_owner(self.tcx, ty))
                && args
                    .get(1)
                    .is_some_and(|limit| is_unbounded_clear_prefix_limit(limit))
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

fn is_public_or_hook(tcx: TyCtxt<'_>, def_id: LocalDefId) -> bool {
    if tcx.local_visibility(def_id).is_public() {
        return true;
    }

    let item_name = tcx.item_name(def_id.to_def_id());
    let name = item_name.as_str();
    matches!(
        name,
        "on_poll" | "on_idle" | "on_initialize" | "on_finalize"
    )
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

fn macro_call_site(span: Span, macro_name: &str) -> Option<Span> {
    let mut call_site = span;
    for _ in 0..32 {
        if call_site.ctxt().is_root() {
            return None;
        }
        let expn_data = call_site.ctxt().outer_expn_data();
        if let ExpnKind::Macro(_, name) = expn_data.kind {
            if name.as_str() == macro_name {
                return Some(expn_data.call_site.source_callsite());
            }
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
        ExprKind::Tup(values) | ExprKind::Array(values) => values
            .iter()
            .any(|value| expr_references_tainted_binding(typeck, value, tainted_bindings)),
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
        QPath::Resolved(_, _) | QPath::LangItem(_, _) => None,
    }
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
    match ty.kind() {
        TyKind::Adt(adt, args) => {
            let name = tcx.def_path_str(adt.did());
            is_recursive_decode_target_name(&name)
                || args.iter().any(|arg| match arg.kind() {
                    GenericArgKind::Type(arg_ty) => {
                        type_contains_recursive_decode_target(tcx, arg_ty)
                    }
                    _ => false,
                })
        }
        TyKind::Alias(kind, alias_ty) => {
            is_recursive_decode_target_name(&tcx.def_path_str(alias_ty.def_id))
                || expand_alias_type(tcx, *kind, *alias_ty)
                    .is_some_and(|expanded| type_contains_recursive_decode_target(tcx, expanded))
        }
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
    if !name.contains("frame_support::storage") {
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

fn weight_accounts_for_param(weight_snippet: &str, param_name: &str) -> bool {
    [
        format!("{param_name}.len()"),
        format!("{param_name}.encoded_size()"),
        format!("{param_name}.using_encoded("),
        format!("{param_name}.using_encoded(|"),
        format!("encoded_size({param_name})"),
        format!("encoded_size(&{param_name})"),
    ]
    .iter()
    .any(|needle| weight_snippet.contains(needle))
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
    env::var("POLKADOT_LINTER_RUSTC_FILE_CONTAINS")
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
    env::var("POLKADOT_LINTER_RUSTC_RULES")
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
        .filter(|diagnostic| {
            file_filters.is_empty()
                || file_filters
                    .iter()
                    .any(|needle| diagnostic.file.contains(needle))
        })
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
        eprintln!("usage: polkadot-linter-rustc <rustc args>");
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

    if let Ok(path) = env::var("POLKADOT_LINTER_RUSTC_JSONL") {
        append_jsonl_diagnostics(&path, &diagnostics);
    } else if !wrapper_mode {
        println!(
            "{}",
            serde_json::to_string_pretty(&diagnostics).expect("rustc diagnostics should serialize")
        );
    }
    process::exit(result);
}
