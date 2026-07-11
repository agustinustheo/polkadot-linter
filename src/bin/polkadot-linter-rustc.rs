#![feature(rustc_private)]

extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_span;

use std::{env, process};

use rustc_driver::{Callbacks, Compilation};
use rustc_hir::{
    def_id::LocalDefId,
    intravisit::{self, Visitor},
    BinOpKind, BodyOwnerKind, Expr, ExprKind,
};
use rustc_middle::ty::{Ty, TyCtxt, TyKind};
use rustc_span::{source_map::SourceMap, Span};
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
            if !returns_fallible(tcx, def_id) {
                continue;
            }

            let typeck = tcx.typeck(def_id);
            let body = tcx.hir_body_owned_by(def_id);
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
