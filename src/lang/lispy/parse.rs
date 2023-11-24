//! an unfinished lang frontend for replacing the scheme lang by a more efficient one

use super::super::common::hex_decode;
use super::token::Token;
use crate::ast::Ident;
use alloc::{string::String, vec, vec::Vec};
use logos::Logos;

pub struct Lexer<'a>(logos::Lexer<'a, Token>);

impl<'a> Lexer<'a> {
    pub fn new(content: &'a str) -> Self {
        let lex = Token::lexer(content);
        Lexer(lex)
    }
}

pub type Span = core::ops::Range<usize>;
type Err = ();

fn span_merge(start: &Span, end: &Span) -> Span {
    assert!(
        start.end < end.start,
        "merging span failed start={:?} end={:?}",
        start,
        end
    );
    Span {
        start: start.start,
        end: end.end,
    }
}

pub type Spanned<T> = (T, Span);

impl<'a> Iterator for Lexer<'a> {
    type Item = Spanned<Result<Token, Err>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.0.next() {
            None => None,
            Some(token) => {
                let span = self.0.span();
                Some((token, span))
            }
        }
    }
}

#[derive(Clone)]
pub enum Expr {
    /// Atom is just some ident like 'foo' or '+'
    Atom(Span, Ident),
    /// Literal value a number '123', string '"foo"', or bytes '#ABCD#'
    Literal(Span, Literal),
    /// List of expression '(a b c)'
    List(Span, ListExpr),
    // (define (id args) expr
    Define(Span, Vec<Spanned<Ident>>, Vec<Expr>),
}

impl Expr {
    pub fn literal(&self) -> Option<(&Literal, &Span)> {
        match &self {
            Expr::Literal(span, lit) => Some((lit, span)),
            _ => None,
        }
    }
    pub fn atom(&self) -> Option<(&Ident, &Span)> {
        match &self {
            Expr::Atom(span, atom) => Some((atom, span)),
            _ => None,
        }
    }

    pub fn atom_eq(&self, s: &str) -> bool {
        match &self {
            Expr::Atom(_, ident) => ident.matches(s),
            _ => false,
        }
    }

    #[allow(unused)]
    pub fn list(&self) -> Option<(&ListExpr, &Span)> {
        match &self {
            Expr::List(span, si) => Some((&si, &span)),
            _ => None,
        }
    }
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Atom(span, _) => span.clone(),
            Expr::Literal(span, _) => span.clone(),
            Expr::List(span, _) => span.clone(),
            Expr::Define(span, _, _) => span.clone(),
        }
    }
}

type ListExpr = Vec<Expr>;

#[derive(Clone, PartialEq, Eq)]
pub enum Literal {
    Bytes(Vec<u8>),
    Number(String),
    String(String),
}

pub struct ListCreate {
    start: Span,
    exprs: Vec<Expr>,
}

pub struct Parser<'a> {
    errored: bool,
    context: Vec<ListCreate>,
    lex: Lexer<'a>,
}

#[derive(Clone, Debug)]
pub enum ParseError {
    NotStartedList(Span),
    UnterminatedList(Span),
    LexingError(), //
    DefineEmptyName {
        define_span: Span,
        args_span: Span,
    },
    DefineArgumentNotList {
        define_span: Span,
        args_span: Span,
    },
    DefineArgumentNotAtom {
        define_span: Span,
        args_span: Span,
        arg_invalid_span: Span,
    },
}

pub enum ParserRet {
    Continue,
    Yield(Expr),
}

/// drop nb elements from the start of the vector in place
fn vec_drop_start<T>(v: &mut Vec<T>, nb: usize) {
    if nb == 0 {
        return;
    } else if nb >= v.len() {
        v.truncate(0)
    } else {
        v.reverse();
        v.truncate(v.len() - nb);
        v.reverse()
    }
}

impl<'a> Parser<'a> {
    pub fn new(lex: Lexer<'a>) -> Self {
        Self {
            lex,
            errored: false,
            context: Vec::new(),
        }
    }
    fn process_list(&mut self, list_span: Span, exprs: Vec<Expr>) -> Result<Expr, ParseError> {
        fn parse_define(list_span: Span, mut exprs: Vec<Expr>) -> Result<Expr, ParseError> {
            // (define (name args*) body)
            // (define name body)
            let idents = match &exprs[1] {
                Expr::List(span_args, id_args) => {
                    // on empty list, raise an error
                    if id_args.len() == 0 {
                        return Err(ParseError::DefineEmptyName {
                            define_span: list_span,
                            args_span: span_args.clone(),
                        });
                    }

                    let mut idents = Vec::new();
                    for id_arg in id_args {
                        match id_arg.atom() {
                            None => {
                                return Err(ParseError::DefineArgumentNotAtom {
                                    define_span: list_span,
                                    args_span: span_args.clone(),
                                    arg_invalid_span: id_arg.span(),
                                })
                            }
                            Some(sident) => idents.push((sident.0.clone(), sident.1.clone())),
                        }
                    }
                    idents
                }
                Expr::Atom(span_id, id) => vec![(id.clone(), span_id.clone())],
                _ => {
                    return Err(ParseError::DefineArgumentNotList {
                        define_span: list_span,
                        args_span: exprs[1].span(),
                    });
                }
            };

            // drop 'define' atom and first name or list of name+args
            vec_drop_start(&mut exprs, 2);
            Ok(Expr::Define(list_span, idents, exprs))
        }

        match exprs.first() {
            None => Ok(Expr::List(list_span, exprs)),
            Some(first_elem) => {
                if first_elem.atom_eq("define") {
                    parse_define(list_span, exprs)
                } else {
                    Ok(Expr::List(list_span, exprs))
                }
            }
        }
    }

    fn push_list(&mut self, span: Span) -> Result<ParserRet, ParseError> {
        self.context.push(ListCreate {
            start: span,
            exprs: Vec::with_capacity(0),
        });
        Ok(ParserRet::Continue)
    }

    fn pop_list(&mut self, end_span: Span) -> Result<ParserRet, ParseError> {
        match self.context.pop() {
            None => Err(ParseError::NotStartedList(end_span)),
            Some(ListCreate { start, exprs }) => {
                let list_span = span_merge(&start, &end_span);
                let e = self.process_list(list_span, exprs)?;
                match self.context.last_mut() {
                    None => Ok(ParserRet::Yield(e)),
                    Some(ctx) => {
                        ctx.exprs.push(e);
                        Ok(ParserRet::Continue)
                    }
                }
            }
        }
    }

    fn push_literal(&mut self, span: Span, literal: Literal) -> Result<ParserRet, ParseError> {
        match self.context.last_mut() {
            None => {
                let ret = Expr::Literal(span, literal);
                Ok(ParserRet::Yield(ret))
            }
            Some(ctx) => {
                ctx.exprs.push(Expr::Literal(span, literal));
                Ok(ParserRet::Continue)
            }
        }
    }

    fn push_ident(&mut self, span: Span, ident: Ident) -> Result<ParserRet, ParseError> {
        match self.context.last_mut() {
            None => {
                let ret = Expr::Atom(span, ident);
                Ok(ParserRet::Yield(ret))
            }
            Some(ctx) => {
                ctx.exprs.push(Expr::Atom(span, ident));
                Ok(ParserRet::Continue)
            }
        }
    }

    fn push_token(&mut self, span: Span, tok: Token) -> Result<ParserRet, ParseError> {
        match tok {
            Token::ParenOpen => self.push_list(span),
            Token::ParenClose => self.pop_list(span),
            Token::Number(n) => self.push_literal(span, Literal::Number(n)),
            Token::Bytes(b) => self.push_literal(span, Literal::Bytes(hex_decode(&b))),
            Token::String(s) => self.push_literal(span, Literal::String(s)),
            Token::Ident(a) => self.push_ident(span, Ident::from(a)),
        }
    }

    fn ret_error(&mut self, error: ParseError) -> Result<Expr, ParseError> {
        self.errored = true;
        Err(error)
    }
}

impl<'a> Iterator for Parser<'a> {
    type Item = Result<Expr, ParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.errored {
            return None;
        }

        loop {
            let Some(next) = self.lex.next() else {
                match self.context.pop() {
                    None => return None,
                    Some(ListCreate { start, exprs: _ }) => {
                        // if still have context and there's no more token, some list are not terminated
                        return Some(self.ret_error(ParseError::UnterminatedList(start.clone())));
                    }
                }
            };

            let span = next.1;
            let tok = match next.0 {
                Err(_) => {
                    return Some(self.ret_error(ParseError::LexingError()));
                }
                Ok(n) => n,
            };

            match self.push_token(span.clone(), tok) {
                Err(e) => {
                    return Some(self.ret_error(e));
                }
                Ok(ParserRet::Yield(e)) => return Some(Ok(e)),
                Ok(ParserRet::Continue) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn match_expr(e1: &Expr, e2: &Expr) -> bool {
        match (e1, e2) {
            (Expr::Atom(_, i1), Expr::Atom(_, i2)) => i1 == i2,
            (Expr::Literal(_, l1), Expr::Literal(_, l2)) => l1 == l2,
            (Expr::List(_, l1), Expr::List(_, l2)) => match_exprs(l1, l2),
            (Expr::Define(_, a1, b1), Expr::Define(_, a2, b2)) => {
                a1.len() == a2.len()
                    && a1.iter().zip(a2.iter()).all(|(a1, a2)| a1.0 == a2.0)
                    && match_exprs(b1, b2)
            }
            _ => false,
        }
    }

    fn match_exprs(e1: &Vec<Expr>, e2: &Vec<Expr>) -> bool {
        e1.len() == e2.len() && e1.iter().zip(e2.iter()).all(|(e1, e2)| match_expr(e1, e2))
    }

    #[test]
    fn it_works() {
        let snippet = r#"
        (define (add3 a b c)
            (+ (+ a b) c)
        )
        (add3 10 20 30)
        "#;
        let lex = Lexer::new(snippet);
        let mut parser = Parser::new(lex);

        // fake span factory
        let fs = || Span { start: 0, end: 0 };
        let mk_atom = |s: &str| Expr::Atom(fs(), Ident::from(s));
        let mk_num = |s: &str| Expr::Literal(fs(), Literal::Number(String::from(s)));
        let mk_list = |v: Vec<Expr>| Expr::List(fs(), v);
        let mk_var = |s: &str| (Ident::from(s), fs());

        match parser.next() {
            None => panic!("parser terminated early"),
            Some(e) => match e {
                Err(e) => panic!("parser error on first statement: {:?}", e),
                Ok(d) => {
                    if !match_expr(
                        &d,
                        &Expr::Define(
                            fs(),
                            vec![mk_var("add3"), mk_var("a"), mk_var("b"), mk_var("c")],
                            vec![mk_list(vec![
                                mk_atom("+"),
                                mk_list(vec![mk_atom("+"), mk_atom("a"), mk_atom("b")]),
                                mk_atom("c"),
                            ])],
                        ),
                    ) {
                        panic!("not parsed a define")
                    }
                }
            },
        }

        match parser.next() {
            None => panic!("parser terminated early"),
            Some(e) => match e {
                Err(e) => panic!("parser error on first statement: {:?}", e),
                Ok(d) => {
                    if !match_expr(
                        &d,
                        &mk_list(vec![
                            mk_atom("add3"),
                            mk_num("10"),
                            mk_num("20"),
                            mk_num("30"),
                        ]),
                    ) {
                        panic!("not parsed a define")
                    }
                }
            },
        }

        assert!(
            parser.next().is_none(),
            "parser is unfinished when it should be finished"
        );
    }
}