#![feature(rustc_private)]

extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_span;

use std::{collections::HashSet, env, process};

use rustc_driver::{Callbacks, Compilation};
use rustc_hir::{
    def_id::LocalDefId,
    intravisit::{self, Visitor},
    BinOpKind, BodyOwnerKind, Expr, ExprKind, QPath,
};
use rustc_middle::ty::{GenericArgKind, Ty, TyCtxt, TyKind};
use rustc_span::{hygiene::ExpnKind, source_map::SourceMap, Span};
use serde::Serialize;

#[derive(Debug, Serialize)]
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
}

impl Callbacks for PolkadotCallbacks {
    fn after_analysis<'tcx>(
        &mut self,
        _compiler: &rustc_interface::interface::Compiler,
        tcx: TyCtxt<'tcx>,
    ) -> Compilation {
        for def_id in tcx.hir_body_owners() {
            if !matches!(tcx.hir_body_owner_kind(def_id), BodyOwnerKind::Fn) {
                continue;
            }
            let typeck = tcx.typeck(def_id);
            let body = tcx.hir_body_owned_by(def_id);

            report_unbounded_public_vec_inputs(
                tcx,
                def_id,
                body,
                tcx.sess.source_map(),
                &mut self.diagnostics,
            );

            let mut decode_visitor = Sec003Visitor {
                source_map: tcx.sess.source_map(),
                tcx,
                typeck,
                diagnostics: &mut self.diagnostics,
            };
            decode_visitor.visit_body(body);

            let mut debug_assert_visitor = Sec002Visitor {
                source_map: tcx.sess.source_map(),
                diagnostics: &mut self.diagnostics,
                reported_lines: HashSet::new(),
            };
            debug_assert_visitor.visit_body(body);

            let mut panic_visitor = Sec008Visitor {
                source_map: tcx.sess.source_map(),
                tcx,
                typeck,
                diagnostics: &mut self.diagnostics,
            };
            panic_visitor.visit_body(body);

            if !returns_fallible(tcx, def_id) {
                continue;
            }

            let mut visitor = Sec009Visitor {
                source_map: tcx.sess.source_map(),
                typeck,
                diagnostics: &mut self.diagnostics,
            };
            visitor.visit_body(body);
        }

        Compilation::Stop
    }
}

fn report_unbounded_public_vec_inputs<'tcx>(
    tcx: TyCtxt<'tcx>,
    def_id: LocalDefId,
    body: &'tcx rustc_hir::Body<'tcx>,
    source_map: &SourceMap,
    diagnostics: &mut Vec<RustcDiagnostic>,
) {
    if !tcx.local_visibility(def_id).is_public() {
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
}

impl<'tcx> Visitor<'tcx> for Sec003Visitor<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if is_unlimited_decode_call(expr)
            && (type_contains_recursive_decode_target(self.tcx, self.typeck.expr_ty(expr))
                || decode_receiver_contains_recursive_target(self.tcx, self.typeck, expr))
        {
            let (file, line, column) = span_location(self.source_map, expr.span);
            self.diagnostics.push(RustcDiagnostic {
                rule_id: "SEC003",
                rule_name: "missing-decode-depth-limit",
                file,
                line,
                column,
                message: "Recursive runtime type is decoded without a depth limit".to_string(),
            });
        }

        intravisit::walk_expr(self, expr);
    }
}

struct Sec008Visitor<'a, 'tcx> {
    source_map: &'a SourceMap,
    tcx: TyCtxt<'tcx>,
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    diagnostics: &'a mut Vec<RustcDiagnostic>,
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

        intravisit::walk_expr(self, expr);
    }
}

struct Sec009Visitor<'a, 'tcx> {
    source_map: &'a SourceMap,
    typeck: &'a rustc_middle::ty::TypeckResults<'tcx>,
    diagnostics: &'a mut Vec<RustcDiagnostic>,
}

impl<'tcx> Visitor<'tcx> for Sec009Visitor<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        if let ExprKind::Binary(op, lhs, rhs) = expr.kind {
            if is_raw_arithmetic(op.node)
                && is_integral(self.typeck.expr_ty(lhs))
                && is_integral(self.typeck.expr_ty(rhs))
            {
                let (file, line, column) = span_location(self.source_map, expr.span);
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

fn returns_fallible<'tcx>(tcx: TyCtxt<'tcx>, def_id: LocalDefId) -> bool {
    let sig = tcx.fn_sig(def_id).instantiate_identity().skip_binder();
    type_is_fallible(tcx, sig.output())
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

fn decode_receiver_contains_recursive_target<'tcx>(
    tcx: TyCtxt<'tcx>,
    typeck: &rustc_middle::ty::TypeckResults<'tcx>,
    expr: &'tcx Expr<'tcx>,
) -> bool {
    match expr.kind {
        ExprKind::Call(callee, _) => {
            let ExprKind::Path(QPath::TypeRelative(ty, _)) = callee.kind else {
                return false;
            };
            type_contains_recursive_decode_target(tcx, typeck.node_type(ty.hir_id))
        }
        ExprKind::MethodCall(_, receiver, _, _) => {
            type_contains_recursive_decode_target(tcx, typeck.expr_ty(receiver))
        }
        _ => false,
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
        TyKind::Alias(_, alias_ty) => {
            let expanded = tcx.type_of(alias_ty.def_id).instantiate(tcx, alias_ty.args);
            type_contains_recursive_decode_target(tcx, expanded)
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
        TyKind::Alias(_, alias_ty) => {
            let expanded = tcx.type_of(alias_ty.def_id).instantiate(tcx, alias_ty.args);
            type_contains_unbounded_vec(tcx, expanded)
        }
        TyKind::Ref(_, inner, _) => type_contains_unbounded_vec(tcx, *inner),
        TyKind::Slice(_) => false,
        TyKind::Array(inner, _) => type_contains_unbounded_vec(tcx, *inner),
        TyKind::Tuple(types) => types
            .iter()
            .any(|inner| type_contains_unbounded_vec(tcx, inner)),
        _ => false,
    }
}

fn is_vec_type_name(name: &str) -> bool {
    matches!(name, "Vec") || name.ends_with("::Vec")
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
        TyKind::Alias(_, alias_ty) => {
            let expanded = tcx.type_of(alias_ty.def_id).instantiate(tcx, alias_ty.args);
            type_is_uninhabited(tcx, expanded)
        }
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

fn main() {
    let mut rustc_args = env::args().skip(1).collect::<Vec<_>>();
    if rustc_args.is_empty() {
        eprintln!("usage: polkadot-linter-rustc <rustc args>");
        process::exit(2);
    }
    rustc_args.insert(0, "rustc".to_string());
    if !rustc_args.iter().any(|arg| arg == "--crate-name") {
        rustc_args.push("--crate-name".to_string());
        rustc_args.push("lint_target".to_string());
    }
    if !rustc_args.iter().any(|arg| arg == "--error-format=json") {
        rustc_args.push("--error-format=json".to_string());
    }

    let mut callbacks = PolkadotCallbacks::default();
    let result = rustc_driver::catch_with_exit_code(move || {
        rustc_driver::run_compiler(&rustc_args, &mut callbacks);
        println!(
            "{}",
            serde_json::to_string_pretty(&callbacks.diagnostics)
                .expect("rustc diagnostics should serialize")
        );
    });
    process::exit(result);
}
