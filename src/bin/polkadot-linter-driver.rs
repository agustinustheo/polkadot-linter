#![feature(rustc_private)]

extern crate rustc_ast;
extern crate rustc_driver;
extern crate rustc_expand;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_session;
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
    BinOpKind, BodyOwnerKind, Expr, ExprKind, HirId, LetStmt, Pat, PatKind, QPath, StmtKind,
};
use rustc_interface::interface::Compiler;
use rustc_middle::ty::{AliasTyKind, BorrowKind, GenericArgKind, Ty, TyCtxt, TyKind, UpvarCapture};
use rustc_span::{source_map::SourceMap, Span, Symbol};
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
    parsed_weight_attributes: Vec<ParsedWeightAttribute>,
    parsed_transactional_attributes: Vec<ParsedTransactionalAttribute>,
    try_runtime_spans: Vec<Span>,
}

#[derive(Clone)]
struct ParsedWeightAttribute {
    function_name: String,
    file: String,
    line: usize,
    attribute_start_line: usize,
    attribute_end_line: usize,
    has_call_index: bool,
}

#[derive(Clone)]
struct ParsedTransactionalAttribute {
    function_name: String,
    file: String,
    line: usize,
}

impl Callbacks for PolkadotCallbacks {
    fn after_crate_root_parsing(
        &mut self,
        compiler: &Compiler,
        krate: &mut rustc_ast::ast::Crate,
    ) -> Compilation {
        let mut visitor = ParsedWeightAttributeVisitor {
            sess: &compiler.sess,
            source_map: compiler.sess.source_map(),
            parsed_attributes: &mut self.parsed_weight_attributes,
            parsed_transactional_attributes: &mut self.parsed_transactional_attributes,
            try_runtime_spans: &mut self.try_runtime_spans,
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
        let (genesis_bodies, bookkeeping_bodies) =
            context_excluded_bodies(tcx, &self.parsed_weight_attributes, &self.try_runtime_spans);
        if self.rule_enabled("SEC004") {
            report_raw_weight_arithmetic(
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
        let reachable_entry_point_bodies = (self.rule_enabled("SEC011")
            || self.rule_enabled("SEC012")
            || self.rule_enabled("SEC015"))
        .then(|| reachable_local_function_bodies(tcx, false, &genesis_bodies))
        .unwrap_or_default();
        if self.rule_enabled("SEC011") {
            report_reachable_storage_iteration(
                tcx,
                tcx.sess.source_map(),
                &reachable_entry_point_bodies,
                &bookkeeping_bodies,
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

            if self.rule_enabled("SEC019") || self.rule_enabled("SEC020") {
                Sec019Sec020Visitor {
                    tcx,
                    typeck,
                    sec019: self.rule_enabled("SEC019"),
                    sec020: self.rule_enabled("SEC020"),
                    diagnostics: &mut self.diagnostics,
                }
                .visit_body(body);
            }

            let root = tcx.typeck_root_def_id(def_id.to_def_id());
            let mut auth = AuthGuardVisitor {
                tcx,
                typeck,
                root: root.expect_local(),
                sec021: self.rule_enabled("SEC021"),
                sec022: self.rule_enabled("SEC022") && !is_test_or_bench_code(tcx, def_id),
                // A closure's comparison can run after a guard written below it: suppressed.
                sec023: self.rule_enabled("SEC023")
                    && root == def_id.to_def_id()
                    && is_ensure_origin_method(tcx, root),
                sec024: self.rule_enabled("SEC024") && !is_test_or_bench_code(tcx, def_id),
                diagnostics: &mut self.diagnostics,
            };
            if auth.sec021 || auth.sec022 || auth.sec023 || auth.sec024 {
                auth.visit_body(body);
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
        || env::var_os("POLKADOT_LINTER_DRIVER_MANIFEST_ROOT").is_some_and(|root| {
            let root = PathBuf::from(root);
            let crate_source = PathBuf::from(crate_source);
            let crate_source = crate_source
                .is_relative()
                .then(|| root.join(&crate_source))
                .unwrap_or(crate_source);
            crate_source.starts_with(root)
        })
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
    genesis_helpers: &HashSet<LocalDefId>,
) -> Vec<LocalDefId> {
    let pending = tcx
        .hir_body_owners()
        .filter(|def_id| {
            matches!(tcx.hir_body_owner_kind(*def_id), BodyOwnerKind::Fn)
                && is_reachable_entry_point(tcx, *def_id)
                && !genesis_helpers.contains(def_id)
                && (!fallible_entry_points_only || returns_fallible(tcx, *def_id))
        })
        .collect::<Vec<_>>();
    collect_local_callees(tcx, pending)
}

fn collect_local_callees(tcx: TyCtxt<'_>, mut pending: Vec<LocalDefId>) -> Vec<LocalDefId> {
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

fn report_reachable_storage_iteration<'tcx>(
    tcx: TyCtxt<'tcx>,
    source_map: &SourceMap,
    reachable_bodies: &[LocalDefId],
    bookkeeping_bodies: &HashSet<LocalDefId>,
    diagnostics: &mut Vec<RustcDiagnostic>,
) {
    for def_id in reachable_bodies {
        if bookkeeping_bodies.contains(def_id) {
            continue;
        }
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

/// SEC019 (imbalance captured into a storage-layer closure) and SEC020
/// (verifier `bool` verdict discarded) both inspect calls in one body.
struct Sec019Sec020Visitor<'a, 'tcx> {
    tcx: TyCtxt<'tcx>,
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    sec019: bool,
    sec020: bool,
    diagnostics: &'a mut Vec<RustcDiagnostic>,
}

impl<'tcx> Visitor<'tcx> for Sec019Sec020Visitor<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        let (name_span, args, args_owner) = match expr.kind {
            ExprKind::Call(callee, args) => (callee.span, args, callee.hir_id),
            ExprKind::MethodCall(segment, _, args, _) => (segment.ident.span, args, expr.hir_id),
            _ => (expr.span, &[][..], expr.hir_id),
        };
        if let Some((def_id, name)) = callee_name(self.tcx, self.typeck, expr) {
            if self.sec019
                && is_transactional_layer_fn(self.tcx, def_id, name.as_str())
                // `impl ProcessTransaction for ()` runs the closure without a storage layer.
                && !(self.tcx.trait_of_assoc(def_id).is_some()
                    && self.typeck.node_args(args_owner).type_at(0).is_unit())
            {
                for arg in args {
                    if let ExprKind::Closure(closure) = arg.kind {
                        self.report_escaping_imbalances(arg, closure);
                    }
                }
            }
            if self.sec020
                && is_verifier_name(name.as_str())
                && type_carries_bool_verdict(self.tcx, self.typeck.expr_ty(expr))
                && result_is_discarded(self.tcx, expr.hir_id, Followed::Verdict)
            {
                let (file, line, column) = span_location(self.tcx.sess.source_map(), name_span);
                self.diagnostics.push(RustcDiagnostic {
                    rule_id: "SEC020",
                    rule_name: "discarded-verification-result",
                    file,
                    line,
                    column,
                    message: format!(
                        "`{}` returns a bool verdict that is never inspected; `Ok(false)`/`false` (an invalid proof or signature) is treated as success. Bind the verdict and reject when it is false",
                        name
                    ),
                });
            }
        }
        intravisit::walk_expr(self, expr);
    }
}

impl<'tcx> Sec019Sec020Visitor<'_, 'tcx> {
    fn report_escaping_imbalances(
        &mut self,
        closure_expr: &'tcx Expr<'tcx>,
        closure: &'tcx rustc_hir::Closure<'tcx>,
    ) {
        let captured = self
            .typeck
            .closure_min_captures_flattened(closure.def_id)
            .filter(|capture| {
                matches!(
                    capture.info.capture_kind,
                    UpvarCapture::ByRef(BorrowKind::Mutable)
                ) && capture.place.projections.is_empty()
                    && type_holds_imbalance(self.tcx, capture.place.ty())
            })
            .map(|capture| capture.get_root_variable())
            .collect::<HashSet<_>>();
        if captured.is_empty() {
            return;
        }
        let TyKind::Closure(_, closure_args) = self.typeck.node_type(closure_expr.hir_id).kind()
        else {
            return;
        };
        let output = closure_args.as_closure().sig().output().skip_binder();
        let body_value = self.tcx.hir_body(closure.body).value;
        let mut body = EscapeScan {
            tcx: self.tcx,
            typeck: self.typeck,
            captured: &captured,
            body_value: body_value.hir_id,
            returns_outcome: output
                .ty_adt_def()
                .is_some_and(|adt| self.tcx.item_name(adt.did()).as_str() == "TransactionOutcome"),
            outer_loop: None,
            last_exit: None,
            moves: Vec::new(),
        };
        body.visit_expr(body_value);
        let mut reported = HashSet::new();
        for (variable, _) in body
            .moves
            .into_iter()
            .filter(|&(local, at)| body.last_exit >= Some(at) && reported.insert(local))
        {
            let (file, line, column) =
                span_location(self.tcx.sess.source_map(), self.tcx.hir_span(variable));
            self.diagnostics.push(RustcDiagnostic {
                rule_id: "SEC019",
                rule_name: "imbalance-escapes-transaction",
                file,
                line,
                column,
                message: format!(
                    "`{}` is declared outside the transactional closure and receives a token imbalance inside it; it outlives the storage layer, so when the closure rolls back, storage is restored but the imbalance's Drop still writes issuance afterwards. Declare it inside the closure, or resolve it before returning",
                    self.tcx.hir_name(variable)
                ),
            });
        }
    }
}

/// Scans a transactional closure body for imbalances moved into captured locals
/// (`withdrawn.subsume_assets(credit)`, `slot = credit`) and for exits that can roll back
/// after such a move. Positions are call-site byte offsets (so a cross-crate `ensure!` counts
/// where it is written); inside a loop, a move counts from the outermost loop's start, since
/// a later iteration can exit, unless its block then unconditionally leaves that loop.
/// Nested closures are skipped: they may never run, and their exits don't end this closure.
struct EscapeScan<'a, 'tcx> {
    tcx: TyCtxt<'tcx>,
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    captured: &'a HashSet<HirId>,
    body_value: HirId,
    /// `with_transaction*` closures return `TransactionOutcome`; only `Rollback` rolls back.
    returns_outcome: bool,
    outer_loop: Option<(HirId, Span)>,
    last_exit: Option<rustc_span::BytePos>,
    moves: Vec<(HirId, rustc_span::BytePos)>,
}

impl<'tcx> Visitor<'tcx> for EscapeScan<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        let moves_imbalance = |value: &Expr<'_>| {
            let ty = self.typeck.expr_ty(value);
            !ty.is_ref() && type_holds_imbalance(self.tcx, ty)
        };
        let receiver = match expr.kind {
            ExprKind::MethodCall(_, receiver, args, _) if args.iter().any(moves_imbalance) => {
                local_binding_id(self.typeck, receiver)
            }
            ExprKind::Assign(place, value, _) if moves_imbalance(value) => {
                local_binding_id(self.typeck, place)
            }
            ExprKind::Closure(_) => return,
            ExprKind::Loop(..) if self.outer_loop.is_none() => {
                self.outer_loop = Some((expr.hir_id, expr.span.source_callsite()));
                intravisit::walk_expr(self, expr);
                self.outer_loop = None;
                return;
            }
            ExprKind::Match(_, _, rustc_hir::MatchSource::TryDesugar(_)) => {
                if !self.returns_outcome {
                    self.exit_at(expr.span);
                }
                None
            }
            ExprKind::Call(
                callee @ Expr {
                    kind: ExprKind::Path(qpath),
                    ..
                },
                _,
            ) => {
                if let Res::Def(rustc_hir::def::DefKind::Ctor(..), ctor) =
                    self.typeck.qpath_res(qpath, callee.hir_id)
                {
                    let variant = self.tcx.parent(ctor);
                    let exits = if self.returns_outcome {
                        self.tcx.item_name(variant).as_str() == "Rollback"
                            && self.tcx.item_name(self.tcx.parent(variant)).as_str()
                                == "TransactionOutcome"
                    } else {
                        self.tcx
                            .is_lang_item(variant, rustc_hir::LangItem::ResultErr)
                    };
                    if exits && self.is_returned(expr.hir_id) {
                        self.exit_at(expr.span);
                    }
                }
                None
            }
            _ => None,
        };
        if let Some(local) = receiver.filter(|local| self.captured.contains(local)) {
            let at = match self.outer_loop {
                Some((id, span)) if !self.leaves_loop_after(expr.hir_id, id) => span.lo(),
                _ => expr.span.source_callsite().hi(),
            };
            self.moves.push((local, at));
        }
        intravisit::walk_expr(self, expr);
    }
}

impl EscapeScan<'_, '_> {
    fn exit_at(&mut self, span: Span) {
        self.last_exit = self.last_exit.max(Some(span.source_callsite().lo()));
    }

    /// Whether, after `expr`, control unconditionally leaves `outer_loop`: some enclosing block,
    /// below that loop, ends with a `return` or a `break` out of it after the statement holding
    /// `expr`. A `break` out of an inner loop or labelled block continues the search past it;
    /// reaching any loop without such an exit means `expr` may run again.
    fn leaves_loop_after(&self, expr: HirId, outer_loop: HirId) -> bool {
        let (mut current, mut skip_to) = (expr, None);
        for (parent_id, node) in self.tcx.hir_parent_iter(expr) {
            match node {
                _ if skip_to.is_some() => skip_to = skip_to.filter(|&to| to != parent_id),
                rustc_hir::Node::Block(block) => {
                    let rest = block.stmts.iter().skip_while(|stmt| stmt.hir_id != current);
                    let rest = rest.filter_map(|stmt| match stmt.kind {
                        StmtKind::Expr(e) | StmtKind::Semi(e) => Some(e),
                        _ => None,
                    });
                    for e in rest.chain(block.expr) {
                        match e.kind {
                            ExprKind::Ret(_) => return true,
                            ExprKind::Break(dest, _) => match dest.target_id {
                                Ok(target) if target == outer_loop => return true,
                                Ok(target) => skip_to = Some(target),
                                Err(_) => continue,
                            },
                            _ => continue,
                        }
                        break;
                    }
                }
                rustc_hir::Node::Expr(Expr {
                    kind: ExprKind::Loop(..),
                    ..
                }) => return false,
                _ => {}
            }
            current = parent_id;
        }
        false
    }

    /// Whether `expr` is `return`ed or is the closure's value (a tail or branch of its body).
    fn is_returned(&self, expr: HirId) -> bool {
        let mut current = expr;
        for (parent_id, node) in self.tcx.hir_parent_iter(expr) {
            if current == self.body_value {
                return true;
            }
            let passes_value = match node {
                rustc_hir::Node::Expr(parent) => match parent.kind {
                    ExprKind::Ret(_) => return true,
                    ExprKind::Block(..) | ExprKind::DropTemps(_) => true,
                    ExprKind::If(cond, ..) | ExprKind::Match(cond, ..) => cond.hir_id != current,
                    _ => false,
                },
                rustc_hir::Node::Block(block) => block.expr.is_some_and(|e| e.hir_id == current),
                rustc_hir::Node::Arm(arm) => arm.body.hir_id == current,
                _ => false,
            };
            if !passes_value {
                return false;
            }
            current = parent_id;
        }
        false
    }
}

/// Functions that run a closure in a storage layer: `with_storage_layer` and
/// `ProcessTransaction::process` roll back when the closure returns `Err`, `with_transaction*`
/// when it returns `TransactionOutcome::Rollback`.
fn is_transactional_layer_fn(tcx: TyCtxt<'_>, def_id: DefId, name: &str) -> bool {
    let krate = tcx.crate_name(def_id.krate);
    match krate.as_str() {
        "frame_support" => name.starts_with("with_transaction") || name == "with_storage_layer",
        krate if krate.ends_with("xcm_executor") => {
            name.starts_with("transactional_process")
                || (name == "process"
                    && tcx.def_kind(def_id).is_assoc()
                    && tcx
                        .associated_item(def_id)
                        .trait_container(tcx)
                        .or_else(|| implemented_trait_id(tcx, def_id))
                        .is_some_and(|tr| tcx.item_name(tr).as_str() == "ProcessTransaction"))
        }
        _ => false,
    }
}

/// Whether `ty` (or a generic argument of it, e.g. `Vec<Credit<..>>`) is a value whose `Drop`
/// writes total issuance: fungible(s) `Imbalance`, legacy `Positive/NegativeImbalance`, or
/// xcm-executor `AssetsInHolding` (which holds imbalances).
/// References and raw pointers are not followed: a `Vec<&Credit<..>>` owns no imbalance.
fn type_holds_imbalance<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> bool {
    let mut walker = ty.walk();
    while let Some(arg) = walker.next() {
        let GenericArgKind::Type(ty) = arg.kind() else {
            continue;
        };
        let def_id = match ty.kind() {
            TyKind::Adt(adt, _) => adt.did(),
            TyKind::Alias(AliasTyKind::Projection, alias) => alias.def_id,
            TyKind::Ref(..) | TyKind::RawPtr(..) => {
                walker.skip_current_subtree();
                continue;
            }
            _ => continue,
        };
        let krate = tcx.crate_name(def_id.krate);
        if match tcx.item_name(def_id).as_str() {
            "NegativeImbalance" | "PositiveImbalance" => true,
            "Imbalance" => krate.as_str() == "frame_support",
            "AssetsInHolding" => krate.as_str().ends_with("xcm_executor"),
            _ => false,
        } {
            return true;
        }
    }
    false
}

fn is_verifier_name(name: &str) -> bool {
    name.contains("verify") || matches!(name, "is_valid" | "check_signature" | "validate_proof")
}

/// `bool`, `Result<bool, _>` or `Option<bool>`.
fn type_carries_bool_verdict<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> bool {
    ty.is_bool()
        || (type_is_option_or_result(tcx, ty)
            && matches!(ty.kind(), TyKind::Adt(_, args) if args.type_at(0).is_bool()))
}

/// The callee of a function or method call and its name.
fn callee_name(
    tcx: TyCtxt<'_>,
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    call: &Expr<'_>,
) -> Option<(DefId, Symbol)> {
    let def_id = match call.kind {
        ExprKind::Call(callee, _) => resolved_call_def_id(typeck, callee),
        ExprKind::MethodCall(..) => typeck.type_dependent_def_id(call.hir_id),
        _ => None,
    }?;
    Some((def_id, tcx.opt_item_name(def_id)?))
}

/// What [`result_is_discarded`] follows from the call.
#[derive(Clone, Copy, PartialEq)]
enum Followed {
    /// A verifier's `bool` verdict, which `?` and `unwrap` pass on and `is_ok()` or
    /// `map(|_| ..)` drop.
    Verdict,
    /// An origin check's `Err`, which `?` propagates and `ok()`/`map`/`is_ok()` keep.
    OriginError,
    /// An `Err` holding funds, which `?`, `map_err` and `match` handle.
    FundsError,
}

/// Follows the call's value through the adapters that keep it and reports whether it ends in a
/// statement, a `let _ =` (for an `Err` also a never-used `let _name =`), or an adapter
/// that drops it.
fn result_is_discarded(tcx: TyCtxt<'_>, call: HirId, followed: Followed) -> bool {
    let verdict = followed == Followed::Verdict;
    let (keep, drop): (&[&str], &[&str]) = if verdict {
        (
            &[
                "map_err",
                "inspect_err",
                "ok",
                "ok_or",
                "ok_or_else",
                "expect",
                "unwrap",
                "unwrap_or",
                "unwrap_or_default",
            ],
            &["is_ok", "is_err", "is_some", "is_none"],
        )
    } else if followed == Followed::OriginError {
        (&["ok", "err", "map", "map_err", "is_ok", "is_err"], &[])
    } else {
        // Not `err`: `.err().map(..)` would act on the funds themselves.
        (&["ok", "map", "is_ok", "is_err"], &[])
    };
    let mut current = call;
    for (parent_id, node) in tcx.hir_parent_iter(call) {
        match node {
            rustc_hir::Node::Expr(parent) => match parent.kind {
                ExprKind::DropTemps(_) => {}
                // `?` desugars to `match Try::branch(value) { .. }`.
                ExprKind::Match(_, _, rustc_hir::MatchSource::TryDesugar(_)) if verdict => {}
                ExprKind::Call(callee, _)
                    if verdict && matches!(callee.kind, ExprKind::Path(QPath::LangItem(..))) => {}
                ExprKind::MethodCall(segment, receiver, args, _) if receiver.hir_id == current => {
                    let name = segment.ident.as_str();
                    if drop.contains(&name) {
                        return true;
                    }
                    if !keep.contains(&name) {
                        return verdict
                            && matches!(name, "map" | "and_then")
                            && matches!(args, [arg] if matches!(arg.kind, ExprKind::Closure(closure)
                                if tcx.hir_body(closure.body).params.first()
                                    .is_some_and(|param| matches!(param.pat.kind, PatKind::Wild))));
                    }
                }
                _ => return false,
            },
            rustc_hir::Node::Stmt(stmt) => return matches!(stmt.kind, StmtKind::Semi(_)),
            rustc_hir::Node::LetStmt(local) => {
                return match local.pat.kind {
                    PatKind::Wild => true,
                    PatKind::Binding(_, binding, ident, None) if !verdict => {
                        ident.as_str().starts_with('_')
                            && !root_body_exprs(tcx, local.hir_id.owner.def_id)
                                .iter()
                                .any(|expr| {
                                    matches!(expr.kind, ExprKind::Path(QPath::Resolved(None, path))
                                    if path.res == Res::Local(binding))
                                })
                    }
                    _ => false,
                };
            }
            _ => return false,
        }
        current = parent_id;
    }
    false
}

/// Every expression in the typeck-root body of `def_id`, closures included.
fn root_body_exprs<'tcx>(tcx: TyCtxt<'tcx>, def_id: LocalDefId) -> Vec<&'tcx Expr<'tcx>> {
    struct Collect<'tcx>(TyCtxt<'tcx>, Vec<&'tcx Expr<'tcx>>);
    impl<'tcx> Visitor<'tcx> for Collect<'tcx> {
        fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
            self.1.push(expr);
            if let ExprKind::Closure(closure) = expr.kind {
                self.visit_body(self.0.hir_body(closure.body));
            }
            intravisit::walk_expr(self, expr);
        }
    }
    let root = tcx.typeck_root_def_id(def_id.to_def_id()).expect_local();
    let mut collect = Collect(tcx, Vec::new());
    collect.visit_body(tcx.hir_body_owned_by(root));
    collect.1
}

/// Code compiled only for tests or benchmarks: under `#[cfg(test)]` or
/// `#[cfg(feature = "runtime-benchmarks")]`, in a test or benchmarking file, or in a
/// benchmarking crate.
fn is_test_or_bench_code(tcx: TyCtxt<'_>, def_id: LocalDefId) -> bool {
    let hir_id = tcx.local_def_id_to_hir_id(def_id);
    let file = tcx.sess.source_map().span_to_filename(tcx.def_span(def_id));
    let path = match file {
        rustc_span::FileName::Real(name) => name.local_path().map(Path::to_path_buf),
        _ => None,
    };
    tcx.crate_name(rustc_hir::def_id::LOCAL_CRATE)
        .as_str()
        .contains("benchmark")
        || path.is_some_and(|path| {
            let class_path = polkadot_linter::engine::classification_path(&path, Path::new(""));
            polkadot_linter::engine::LintEngine::is_test_file(&class_path, "")
                || polkadot_linter::engine::LintEngine::is_benchmark_file(&class_path)
        })
        || std::iter::once(hir_id)
            .chain(tcx.hir_parent_id_iter(hir_id))
            .flat_map(|id| tcx.hir_attrs(id))
            .filter(|attr| attr.has_name(rustc_span::sym::cfg_trace))
            .filter_map(|attr| tcx.sess.source_map().span_to_snippet(attr.span()).ok())
            .any(|cfg| {
                let cfg = cfg
                    .trim_start_matches(['#', '!', '['])
                    .trim_end_matches(']');
                syn::parse_str::<syn::Meta>(cfg).is_ok_and(|meta| match meta {
                    syn::Meta::List(list) if list.path.is_ident("cfg") => list
                        .parse_args()
                        .is_ok_and(|pred| cfg_requires_test_or_bench(&pred)),
                    _ => false,
                })
            })
}

/// The cfg predicate holds only when `test` or `feature = "runtime-benchmarks"` is set:
/// that predicate itself, `all(..)` with one such operand, or `any(..)` of only such operands.
fn cfg_requires_test_or_bench(pred: &syn::Meta) -> bool {
    let operands = |list: &syn::MetaList| {
        list.parse_args_with(
            syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
        )
        .map(|operands| {
            operands
                .into_iter()
                .map(|op| cfg_requires_test_or_bench(&op))
        })
    };
    match pred {
        syn::Meta::Path(path) => path.is_ident("test"),
        syn::Meta::NameValue(nv) => {
            nv.path.is_ident("feature")
                && matches!(&nv.value, syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(s), .. })
                    if s.value() == "runtime-benchmarks")
        }
        syn::Meta::List(list) if list.path.is_ident("all") => {
            operands(list).is_ok_and(|mut ops| ops.any(|op| op))
        }
        syn::Meta::List(list) if list.path.is_ident("any") => {
            operands(list).is_ok_and(|mut ops| ops.all(|op| op)) && !list.tokens.is_empty()
        }
        _ => false,
    }
}

/// SEC021 (`if verify(..) { reject }`), SEC022 (origin-check `Result` discarded), SEC023
/// (`Option != Option` rejecting in an `EnsureOrigin*` impl, so `None == None` is accepted) and
/// SEC024 (a `Result` whose `Err` holds funds, e.g. `resolve`'s credit, discarded).
struct AuthGuardVisitor<'a, 'tcx> {
    tcx: TyCtxt<'tcx>,
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    /// The enclosing function (the body itself, or the function a closure is in).
    root: LocalDefId,
    sec021: bool,
    /// Off in test and benchmark code.
    sec022: bool,
    /// Only inside an `EnsureOrigin*` method.
    sec023: bool,
    /// Off in test and benchmark code.
    sec024: bool,
    diagnostics: &'a mut Vec<RustcDiagnostic>,
}

impl<'tcx> Visitor<'tcx> for AuthGuardVisitor<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        match expr.kind {
            ExprKind::If(condition, then_branch, else_branch)
                if self.branch_rejects(then_branch)
                    && !else_branch.is_some_and(|branch| self.branch_rejects(branch)) =>
            {
                // `ensure!(c, e)` expands to `if !c { return Err(e) }`.
                let (mut condition, mut negated) = (strip_drop_temps(condition), false);
                while let ExprKind::Unary(rustc_hir::UnOp::Not, inner) = condition.kind {
                    (condition, negated) = (strip_drop_temps(inner), !negated);
                }
                match condition.kind {
                    ExprKind::Binary(op, left, right)
                        if self.sec023
                            && matches!(
                                (op.node, negated),
                                (BinOpKind::Ne, false) | (BinOpKind::Eq, true)
                            )
                            && [left, right]
                                .iter()
                                .all(|side| self.may_be_none(side, condition.span)) =>
                    {
                        self.report("SEC023", condition.span, "the comparison is between two `Option`s in an `EnsureOrigin` impl, so when both are `None` they compare equal and the check passes. Unwrap each side (`ok_or(..)?`) before comparing".into());
                    }
                    ExprKind::Call(..) | ExprKind::MethodCall(..)
                        if self.sec021
                            && !negated
                            && self.typeck.expr_ty(condition).is_bool()
                            && callee_name(self.tcx, self.typeck, condition)
                                .is_some_and(|(_, name)| is_verifier_name(name.as_str())) =>
                    {
                        self.report("SEC021", condition.span, "the rejecting branch runs when the verifier returns `true`, so valid proofs or signatures are rejected and invalid ones accepted. Negate the condition".into());
                    }
                    _ => {}
                }
            }
            ExprKind::Call(..) | ExprKind::MethodCall(..)
                if self.sec024
                    && self.err_holds_funds(expr)
                    // An adapter (`.map(..)`) on such a `Result` is reported at the inner call.
                    && !matches!(expr.kind, ExprKind::MethodCall(_, receiver, ..)
                        if self.err_holds_funds(receiver))
                    && result_is_discarded(self.tcx, expr.hir_id, Followed::FundsError) =>
            {
                let name = callee_name(self.tcx, self.typeck, expr)
                    .map_or_else(|| "call".into(), |(_, name)| format!("`{name}`"));
                self.report("SEC024", expr.span, format!("the `Result` of {name} is discarded, but its `Err` returns the funds (credit, debt or imbalance) that could not be placed; dropping it silently burns or mints them. Handle the `Err` (e.g. send it to an `OnUnbalanced` handler or `defensive`) or document the drop with `map_err(drop)`"));
            }
            ExprKind::Call(..) | ExprKind::MethodCall(..) if self.sec022 => {
                if let Some((_, name)) =
                    callee_name(self.tcx, self.typeck, expr).filter(|&(def_id, name)| {
                        is_origin_check(self.tcx, def_id, name.as_str())
                            && result_is_discarded(self.tcx, expr.hir_id, Followed::OriginError)
                    })
                {
                    self.report("SEC022", expr.span, format!("the `Result` of origin check `{name}` is discarded, so this origin requirement is not enforced. Propagate it with `?`"));
                }
            }
            _ => {}
        }
        intravisit::walk_expr(self, expr);
    }
}

impl<'tcx> AuthGuardVisitor<'_, 'tcx> {
    fn report(&mut self, rule_id: &'static str, span: Span, message: String) {
        let rule_name = match rule_id {
            "SEC021" => "inverted-verification-guard",
            "SEC022" => "discarded-origin-check",
            "SEC024" => "discarded-funds-in-error",
            _ => "option-equality-authorization",
        };
        let (file, line, column) = span_location(self.tcx.sess.source_map(), span);
        self.diagnostics.push(RustcDiagnostic {
            rule_id,
            rule_name,
            file,
            line,
            column,
            message,
        });
    }

    /// `expr` has type `Result<_, E>` where `E` owns an imbalance (`Credit`, `Debt`, ..).
    fn err_holds_funds(&self, expr: &Expr<'tcx>) -> bool {
        result_error_type(self.tcx, self.typeck.expr_ty(expr))
            .is_some_and(|err| type_holds_imbalance(self.tcx, err))
    }

    /// An `Option` operand that may be `None` at the comparison at `at`: not a `Some(..)`/`None`
    /// literal, not a local bound from `Some(..)`, and not checked earlier in the function with
    /// `.is_none()`/`.is_some()`/`if let Some`/`let Some(..) = .. else`.
    fn may_be_none(&self, side: &Expr<'_>, at: Span) -> bool {
        let mut inner = side;
        while let ExprKind::AddrOf(_, _, value) = inner.kind {
            inner = value;
        }
        let is_ctor = |expr: &Expr<'_>| {
            let ctor = match expr.kind {
                ExprKind::Call(callee, _) => resolved_call_def_id(self.typeck, callee),
                ExprKind::Path(ref qpath) => self.typeck.qpath_res(qpath, expr.hir_id).opt_def_id(),
                _ => None,
            };
            ctor.is_some_and(|ctor| {
                matches!(self.tcx.def_kind(ctor), rustc_hir::def::DefKind::Ctor(..))
            })
        };
        let bound_to_ctor = local_binding_id(self.typeck, inner).is_some_and(|binding| {
            matches!(self.tcx.parent_hir_node(binding),
                rustc_hir::Node::LetStmt(LetStmt { init: Some(init), .. }) if is_ctor(strip_drop_temps(init)))
        });
        if is_ctor(inner)
            || bound_to_ctor
            || !matches!(self.typeck.expr_ty(side).peel_refs().kind(),
                TyKind::Adt(adt, _) if self.tcx.is_diagnostic_item(rustc_span::sym::Option, adt.did()))
        {
            return false;
        }
        let checked_earlier =
            |checked: &Expr<'_>| checked.span.lo() < at.lo() && self.same_operand(checked, inner);
        !root_body_exprs(self.tcx, self.root).iter().any(|expr| match expr.kind {
            ExprKind::MethodCall(segment, receiver, ..) => {
                matches!(segment.ident.as_str(), "is_none" | "is_some") && checked_earlier(receiver)
            }
            ExprKind::Let(let_expr) => {
                matches!(let_expr.pat.kind, PatKind::TupleStruct(..)) && checked_earlier(let_expr.init)
            }
            ExprKind::Block(block, _) => block.stmts.iter().any(|stmt| {
                matches!(stmt.kind, StmtKind::Let(LetStmt { pat, init: Some(init), els: Some(_), .. })
                    if matches!(pat.kind, PatKind::TupleStruct(..)) && checked_earlier(init))
            }),
            _ => false,
        })
    }

    /// `a` and `b` (ignoring `&`/`&mut`) name the same place: the same local or path, or the same
    /// field of the same place. Other operands (calls) compare by source text.
    fn same_operand(&self, a: &Expr<'_>, b: &Expr<'_>) -> bool {
        let (a, b) = (strip_drop_temps(a), strip_drop_temps(b));
        match (a.kind, b.kind) {
            (ExprKind::AddrOf(_, _, a), _) => self.same_operand(a, b),
            (_, ExprKind::AddrOf(_, _, b)) => self.same_operand(a, b),
            (ExprKind::Path(ref qa), ExprKind::Path(ref qb)) => {
                let res = self.typeck.qpath_res(qa, a.hir_id);
                res != Res::Err && res == self.typeck.qpath_res(qb, b.hir_id)
            }
            (ExprKind::Field(base_a, field_a), ExprKind::Field(base_b, field_b)) => {
                field_a.name == field_b.name && self.same_operand(base_a, base_b)
            }
            (ExprKind::Path(_) | ExprKind::Field(..), _)
            | (_, ExprKind::Path(_) | ExprKind::Field(..)) => false,
            _ => {
                let source_map = self.tcx.sess.source_map();
                source_map
                    .span_to_snippet(a.span)
                    .is_ok_and(|text| Ok(text) == source_map.span_to_snippet(b.span))
            }
        }
    }

    /// Whether the `if` block `branch` itself rejects: one of its own statements (or its tail) is
    /// `return Err(..)`, `return false` or `Err(..)?`, or its tail is `Err(..)`. Nested control
    /// flow and closures do not count.
    fn branch_rejects(&self, branch: &Expr<'_>) -> bool {
        let ExprKind::Block(block, _) = branch.kind else {
            return false;
        };
        let is_err = |expr: &Expr<'_>| {
            matches!(strip_drop_temps(expr).kind, ExprKind::Call(callee, _)
                if resolved_call_def_id(self.typeck, callee).is_some_and(|ctor| self.tcx
                    .is_lang_item(self.tcx.parent(ctor), rustc_hir::LangItem::ResultErr)))
        };
        let rejects = |expr: &Expr<'_>| match strip_drop_temps(expr).kind {
            ExprKind::Ret(Some(value)) => {
                is_err(value)
                    || matches!(strip_drop_temps(value).kind,
                        ExprKind::Lit(lit) if matches!(lit.node, rustc_ast::LitKind::Bool(false)))
            }
            // `Err(e)?` desugars to `match Try::branch(Err(e)) { .. }`.
            ExprKind::Match(scrutinee, _, rustc_hir::MatchSource::TryDesugar(_)) => {
                matches!(scrutinee.kind, ExprKind::Call(_, [value]) if is_err(value))
            }
            _ => false,
        };
        block.stmts.iter().any(
            |stmt| matches!(stmt.kind, StmtKind::Semi(expr) | StmtKind::Expr(expr) if rejects(expr)),
        ) || block.expr.is_some_and(|tail| rejects(tail) || is_err(tail))
    }
}

/// `frame_system::ensure_*` origin checks and `EnsureOrigin*::{ensure,try}_origin`.
fn is_origin_check(tcx: TyCtxt<'_>, def_id: DefId, name: &str) -> bool {
    match tcx.crate_name(def_id.krate).as_str() {
        "frame_system" => matches!(
            name,
            "ensure_signed"
                | "ensure_root"
                | "ensure_none"
                | "ensure_signed_or_root"
                | "ensure_authorized"
        ),
        _ => {
            matches!(
                name,
                "ensure_origin" | "try_origin" | "ensure_origin_or_root"
            ) && is_ensure_origin_method(tcx, def_id)
        }
    }
}

/// A method of (or implementing) frame-support's `EnsureOrigin` / `EnsureOriginWithArg`.
fn is_ensure_origin_method(tcx: TyCtxt<'_>, def_id: DefId) -> bool {
    tcx.def_kind(def_id).is_assoc()
        && tcx
            .trait_of_assoc(def_id)
            .or_else(|| implemented_trait_id(tcx, def_id))
            .is_some_and(|tr| {
                tcx.crate_name(tr.krate).as_str() == "frame_support"
                    && tcx.item_name(tr).as_str().starts_with("EnsureOrigin")
            })
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
        let Some(trait_id) = implemented_trait_id(tcx, def_id.to_def_id()) else {
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

struct Sem009Visitor<'a, 'tcx> {
    source_map: &'a SourceMap,
    tcx: TyCtxt<'tcx>,
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    diagnostics: &'a mut Vec<RustcDiagnostic>,
    reported_lines: &'a mut HashSet<(String, usize)>,
}

impl<'tcx> Visitor<'tcx> for Sem009Visitor<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if let ExprKind::If(condition, then_branch, None) = expr.kind {
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
                if let ExprKind::Block(block, _) = then_branch.kind {
                    if block.stmts.len() == 1 && block.expr.is_none() {
                        finder.visit_stmt(&block.stmts[0]);
                    }
                }

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

fn type_is_weight<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> bool {
    match ty.kind() {
        TyKind::Adt(adt, _) => tcx.def_path_str(adt.did()).ends_with("::Weight"),
        TyKind::Alias(kind, alias_ty) => expand_alias_type(tcx, *kind, *alias_ty)
            .is_some_and(|expanded| type_is_weight(tcx, expanded)),
        _ => false,
    }
}

fn is_frame_system_root_check(path: &str) -> bool {
    path.ends_with("::frame_system::ensure_root") || path.contains("frame_system::ensure_root")
}

struct ParsedWeightAttributeVisitor<'a> {
    sess: &'a rustc_session::Session,
    source_map: &'a SourceMap,
    parsed_attributes: &'a mut Vec<ParsedWeightAttribute>,
    parsed_transactional_attributes: &'a mut Vec<ParsedTransactionalAttribute>,
    try_runtime_spans: &'a mut Vec<Span>,
}

impl ParsedWeightAttributeVisitor<'_> {
    fn collect_try_runtime(&mut self, span: Span, attributes: &[rustc_ast::ast::Attribute]) {
        if attributes.iter().any(|attr| {
            attr.has_name(Symbol::intern("cfg"))
                && attr.meta_item_list().is_some_and(|items| {
                    items.len() == 1
                        && items[0].has_name(Symbol::intern("feature"))
                        && items[0]
                            .value_str()
                            .is_some_and(|value| value.as_str() == "try-runtime")
                })
        }) {
            self.try_runtime_spans.push(span);
        }
    }

    fn collect_function(
        &mut self,
        function_name: &str,
        span: Span,
        attributes: &[rustc_ast::ast::Attribute],
    ) {
        let attributes = rustc_expand::config::pre_configure_attrs(self.sess, attributes);
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
        if self
            .source_map
            .span_to_snippet(weight_attribute.span)
            .is_err()
        {
            return;
        }
        let location = self.source_map.lookup_char_pos(span.lo());
        let attribute_start = self.source_map.lookup_char_pos(weight_attribute.span.lo());
        let attribute_end = self.source_map.lookup_char_pos(weight_attribute.span.hi());
        self.parsed_attributes.push(ParsedWeightAttribute {
            function_name: function_name.to_string(),
            file: location.file.name.prefer_local().to_string(),
            line: location.line,
            attribute_start_line: attribute_start.line,
            attribute_end_line: attribute_end.line,
            has_call_index: attributes.iter().any(|attribute| {
                matches!(&attribute.kind, rustc_ast::ast::AttrKind::Normal(normal) if ast_path_matches(&normal.item.path, &["pallet", "call_index"]))
            }),
        });
    }
}

impl<'ast> ast_visit::Visitor<'ast> for ParsedWeightAttributeVisitor<'_> {
    type Result = ();

    fn visit_item(&mut self, item: &'ast rustc_ast::ast::Item) -> Self::Result {
        self.collect_try_runtime(item.span, &item.attrs);
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
        self.collect_try_runtime(item.span, &item.attrs);
        if let rustc_ast::ast::AssocItemKind::Fn(function) = &item.kind {
            self.collect_function(function.ident.name.as_str(), item.span, &item.attrs);
        }
        ast_visit::walk_assoc_item(self, item, context)
    }
}

fn ast_path_matches(path: &rustc_ast::Path, expected: &[&str]) -> bool {
    let segments = path
        .segments
        .iter()
        .map(|segment| segment.ident.name.as_str())
        .collect::<Vec<_>>();
    segments.iter().copied().eq(expected.iter().copied())
        || (expected.len() > 1
            && segments.len() >= expected.len()
            && segments
                .iter()
                .rev()
                .zip(expected.iter().rev())
                .all(|(actual, expected)| *actual == *expected))
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

fn is_frame_dispatchable(
    tcx: TyCtxt<'_>,
    def_id: LocalDefId,
    source_map: &SourceMap,
    parsed_weight_attributes: &[ParsedWeightAttribute],
) -> bool {
    if implemented_trait_id(tcx, def_id.to_def_id()).is_some() {
        return false;
    }
    let definition_span = tcx
        .def_ident_span(def_id.to_def_id())
        .unwrap_or_else(|| tcx.def_span(def_id));
    let definition_location = source_map.lookup_char_pos(definition_span.lo());
    let definition_file = definition_location.file.name.prefer_local().to_string();
    let definition_line = definition_location.line;
    let function_name = tcx.item_name(def_id.to_def_id());
    // FRAME call constructors reuse the dispatchable's source spans but have different names.
    if source_map
        .span_to_snippet(definition_span)
        .is_ok_and(|name| name != function_name.as_str())
    {
        return false;
    }
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

fn resolved_call_def_id(
    typeck: &rustc_middle::ty::TypeckResults<'_>,
    callee: &Expr<'_>,
) -> Option<DefId> {
    typeck
        .type_dependent_def_id(callee.hir_id)
        .or_else(|| match callee.kind {
            ExprKind::Path(qpath) => typeck.qpath_res(&qpath, callee.hir_id).opt_def_id(),
            _ => None,
        })
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

        // `#[pallet::call]` wraps each dispatchable body in a `with_storage_layer` closure.
        if let ExprKind::Closure(closure) = expr.kind {
            self.visit_body(self.tcx.hir_body(closure.body));
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

fn implemented_trait_id(tcx: TyCtxt<'_>, method_def_id: DefId) -> Option<DefId> {
    tcx.def_kind(method_def_id)
        .is_assoc()
        .then(|| tcx.associated_item(method_def_id).impl_container(tcx))
        .flatten()
        .and_then(|impl_def_id| tcx.trait_id_of_impl(impl_def_id))
}

fn context_excluded_bodies(
    tcx: TyCtxt<'_>,
    parsed_weight_attributes: &[ParsedWeightAttribute],
    try_runtime_spans: &[Span],
) -> (HashSet<LocalDefId>, HashSet<LocalDefId>) {
    let mut genesis = Vec::new();
    let mut bookkeeping = Vec::new();
    let mut production = Vec::new();
    for id in tcx.hir_body_owners() {
        if !matches!(tcx.hir_body_owner_kind(id), BodyOwnerKind::Fn) {
            continue;
        }
        let trait_path =
            implemented_trait_id(tcx, id.to_def_id()).map(|trait_id| tcx.def_path_str(trait_id));
        let frame_trait = trait_path
            .as_deref()
            .filter(|path| is_frame_support_path(path));
        let name = tcx.item_name(id.to_def_id());
        let is_genesis = frame_trait.is_some_and(|path| path.ends_with("::BuildGenesisConfig"));
        let is_bookkeeping = frame_trait.is_some_and(|path| {
            matches!(
                path.rsplit("::").next(),
                Some("OnRuntimeUpgrade" | "UncheckedOnRuntimeUpgrade")
            ) || (path.ends_with("::Hooks")
                && matches!(
                    name.as_str(),
                    "on_runtime_upgrade" | "try_state" | "pre_upgrade" | "post_upgrade"
                ))
        }) || try_runtime_spans
            .iter()
            .any(|span| span.contains(tcx.def_span(id).source_callsite()));
        if is_genesis {
            genesis.push(id);
        }
        if is_bookkeeping {
            bookkeeping.push(id);
        }
        if is_frame_dispatchable(tcx, id, tcx.sess.source_map(), parsed_weight_attributes)
            || (!is_genesis
                && !is_bookkeeping
                && trait_path.is_some()
                && is_reachable_entry_point(tcx, id))
        {
            production.push(id);
        }
    }
    let production: HashSet<_> = collect_local_callees(tcx, production).into_iter().collect();
    let genesis_roots: HashSet<_> = genesis.iter().copied().collect();
    let genesis = collect_local_callees(tcx, genesis)
        .into_iter()
        .filter(|id| {
            !production.contains(id)
                && (genesis_roots.contains(id) || !tcx.local_visibility(*id).is_public())
        })
        .collect();
    let bookkeeping = collect_local_callees(tcx, bookkeeping)
        .into_iter()
        .filter(|id| !production.contains(id))
        .collect();
    (genesis, bookkeeping)
}

fn is_reachable_entry_point(tcx: TyCtxt<'_>, def_id: LocalDefId) -> bool {
    let method_name = tcx.item_name(def_id.to_def_id());
    let name = method_name.as_str();
    if let Some(trait_id) = implemented_trait_id(tcx, def_id.to_def_id()) {
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
    let Some(trait_id) = implemented_trait_id(tcx, def_id.to_def_id()) else {
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

fn type_is_fallible(tcx: TyCtxt<'_>, ty: Ty<'_>) -> bool {
    match ty.kind() {
        TyKind::Adt(adt, _) => {
            let name = tcx.def_path_str(adt.did());
            name.ends_with("::Result") || name.contains("result::Result")
        }
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

    let mut records = String::new();
    for diagnostic in diagnostics {
        records.push_str(
            &serde_json::to_string(diagnostic).expect("rustc diagnostic should serialize"),
        );
        records.push('\n');
    }
    output
        .write_all(records.as_bytes())
        .expect("rustc diagnostics should be written");
}

fn mark_driver_invoked() {
    if let Ok(path) = env::var("POLKADOT_LINTER_DRIVER_MARKER") {
        OpenOptions::new()
            .append(true)
            .open(path)
            .and_then(|mut output| output.write_all(b"1"))
            .expect("rustc driver marker should be writable");
    }
}

fn output_file_filters() -> Vec<String> {
    env::var("POLKADOT_LINTER_DRIVER_FILE_FILTERS_JSON")
        .ok()
        .and_then(|value| serde_json::from_str::<Vec<String>>(&value).ok())
        .unwrap_or_else(|| {
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
        })
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
    mark_driver_invoked();
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
