extern crate alloc;
extern crate proc_macro;

mod lang;
mod parse;

use macro_quote_types::ext::span_to_range;
use macro_quote_types::ToTokenTrees;
use proc_macro::{Ident, TokenStream};

use lang::*;
use parse::Parser;

use macro_quote::quote;

#[proc_macro]
pub fn module(item: TokenStream) -> TokenStream {
    let mut parser: Parser = item.into();
    let mut statements = Vec::new();

    while !parser.is_end() {
        match parser.try_chain(&[("use", parse_use), ("fn", parse_fn)]) {
            (Ok(stmt), p) => {
                parser = p;
                let g = generate_statement(stmt);
                statements.push(g);
            }
            (Err(_errs), _p) => {
                panic!("statement failed : {:?}", _errs);
                //break;
            } //panic!("No parser worked:\n{:?}", errs),
        }
    }

    let inx = vec_macro(statements);
    quote! {
        {
            use ::alloc::{vec::Vec, boxed::Box};
            use werbolg_core::{ir, Ident, Variable, Spanned, Path, PathType};
            ir::Module { statements : #inx }
        }
    }
}

fn vec_macro<X: ToTokenTrees>(inner: Vec<X>) -> TokenStream {
    quote! {
        (Box::new([ #(#inner),* ]) as Box<[_]>).into_vec()
    }
}

/*
fn span_to_werbolg(_span: &Span) -> TokenStream {
    quote! {
        ::proc_macro::Span::call_site()
    }
}
*/

fn werbolg_span() -> TokenStream {
    quote! {
        core::ops::Range { start: 0, end: 0 }
    }
}

fn werbolg_span_from_span(span: proc_macro::Span) -> TokenStream {
    let core::ops::Range { start, end } = span_to_range(span);
    quote! {
        core::ops::Range { start: #start, end: #end }
    }
}

fn werbolg_ident(s: &str) -> TokenStream {
    quote! {
        Ident::from(#s)
    }
}

fn werbolg_ident_from_ident(s: &Ident) -> TokenStream {
    let x = s.to_string();
    quote! {
        Ident::from(#x)
    }
}

fn werbolg_variable_from_ident(s: &Ident) -> TokenStream {
    let x = s.to_string();
    let span = werbolg_span();
    quote! {
        Variable(Spanned::new(#span, Ident::from(#x)))
    }
}

fn generate_statement(statement: Statement) -> TokenStream {
    match statement {
        Statement::Use(_, _) => todo!(),
        Statement::Fn(span, is_private, name, vars, body) => {
            //panic!("span: {:?}", span);
            let span = werbolg_span_from_span(span);
            let v = vec_macro(
                vars.iter()
                    .map(|i| werbolg_variable_from_ident(i))
                    .collect(),
            );
            let b = generate_expr(body);
            let name = werbolg_ident_from_ident(&name);
            let private = if is_private {
                quote! { ir::Privacy::Private }
            } else {
                quote! { ir::Privacy::Public }
            };
            quote! {
                ir::Statement::Function(#span, werbolg_core::ir::FunDef {
                    privacy: #private,
                    name: Some(#name),
                    vars: #v,
                    body: #b,
                })
            }
        }
    }
}

fn generate_expr(expr: Expr) -> TokenStream {
    match expr {
        Expr::Let(ident, bind_expr, then_expr) => {
            let bind = generate_expr(*bind_expr);
            let then = generate_expr(*then_expr);
            let ident = werbolg_ident(&ident.to_string());
            quote! { ir::Expr::Let(ir::Binder::Ident(#ident), Box::new(#bind), Box::new(#then)) }
        }
        Expr::Literal(lit) => quote! { #lit },
        Expr::Path(path) => {
            let path = generate_path(path);
            let span = werbolg_span();
            quote! { ir::Expr::Path(#span, #path) }
        }
        Expr::Call(_) => quote! { werbolg_core::Expr::Call() },
        Expr::If(_) => quote! { werbolg_core::Expr::If { span, cond, then_expr, else_expr } },
    }
}

fn generate_path(Path { absolute, path }: Path) -> TokenStream {
    let fr = vec_macro(
        path.into_iter()
            .map(|fr| werbolg_ident_from_ident(&fr))
            .collect::<Vec<_>>(),
    );
    if absolute {
        quote! { Path::new_raw(PathType::Absolute, #fr) }
    } else {
        quote! { Path::new_raw(PathType::Relative, #fr) }
    }
}
