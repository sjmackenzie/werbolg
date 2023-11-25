//! Werbolg Execution machine

use crate::ir;

mod bindings;
mod location;
mod value;

use alloc::{vec, vec::Vec};
use bindings::{Bindings, BindingsStack};
pub use location::Location;
pub use value::{Value, ValueKind, NIF};

pub struct ExecutionMachine {
    pub root: Bindings<BindingValue>,
    pub module: Bindings<BindingValue>,
    pub local: BindingsStack<BindingValue>,
    pub stacktrace: Vec<Location>,
}

impl ExecutionMachine {
    pub fn new() -> Self {
        Self {
            root: Bindings::new(),
            module: Bindings::new(),
            local: BindingsStack::new(),
            stacktrace: Vec::new(),
        }
    }

    pub fn aborted(&self) -> bool {
        false
    }

    pub fn add_module_binding(&mut self, ident: ir::Ident, value: Value) {
        self.module.add(ident, value)
    }

    pub fn add_local_binding(&mut self, ident: ir::Ident, value: Value) {
        self.local.add(ident, value)
    }

    pub fn add_native_fun(&mut self, ident: &'static str, f: NIF) {
        let value = Value::NativeFun(ident, f);
        let ident = ir::Ident::from(ident);
        self.root.add(ident, value)
    }

    pub fn get_binding(&self, ident: &ir::Ident) -> Result<Value, ExecutionError> {
        let bind = self
            .local
            .get(ident)
            .or_else(|| self.module.get(ident))
            .or_else(|| self.root.get(ident));
        match bind {
            None => Err(ExecutionError::MissingBinding(ident.clone())),
            Some(val) => Ok(val.clone()),
        }
    }

    pub fn scope_enter(&mut self, location: &Location) {
        self.local.scope_enter();
        self.stacktrace.push(location.clone())
    }

    pub fn scope_leave(&mut self) {
        self.stacktrace.pop().unwrap();
        self.local.scope_leave();
    }
}

pub type BindingValue = Value;

#[derive(Debug, Clone)]
pub enum ExecutionError {
    ArityError {
        expected: usize,
        got: usize,
    },
    MissingBinding(ir::Ident),
    CallingNotFunc {
        location: Location,
        value_is: ValueKind,
    },
    ValueKindUnexpected {
        value_expected: ValueKind,
        value_got: ValueKind,
    },
    Abort,
}

pub fn exec(em: &mut ExecutionMachine, module: ir::Module) -> Result<Value, ExecutionError> {
    exec_stmts(em, &module.statements)
}

pub fn exec_stmts(
    em: &mut ExecutionMachine,
    stmts: &[ir::Statement],
) -> Result<Value, ExecutionError> {
    let mut last_value = None;
    for statement in stmts {
        match statement {
            ir::Statement::Function(span, ir::FunDef { name, vars, body }) => {
                em.add_module_binding(
                    name.clone(),
                    Value::Fun(Location::from_span(span), vars.clone(), body.clone()),
                );
            }
            ir::Statement::Expr(e) => {
                let v = exec_expr(em, &e)?;
                last_value = Some(v)
            }
        }
    }
    match last_value {
        None => Ok(Value::Unit),
        Some(val) => Ok(val),
    }
}

pub enum ExecutionAtom {
    List(usize),
    ThenElse(ir::Expr, ir::Expr),
    Call(usize, Location),
    Then(ir::Expr),
    Let(ir::Ident, ir::Expr),
    PopScope,
}

impl ExecutionAtom {
    pub fn arity(&self) -> usize {
        match self {
            ExecutionAtom::List(u) => *u,
            ExecutionAtom::ThenElse(_, _) => 1,
            ExecutionAtom::Call(u, _) => *u,
            ExecutionAtom::Then(_) => 1,
            ExecutionAtom::Let(_, _) => 1,
            ExecutionAtom::PopScope => 1,
        }
    }
}

pub struct ExecutionStack {
    pub values: Vec<Value>,
    pub work: Vec<Work>,
    pub constr: Vec<ExecutionAtom>,
}

pub struct Work(Vec<ir::Expr>);

impl ExecutionStack {
    pub fn new() -> Self {
        ExecutionStack {
            values: Vec::new(),
            work: Vec::new(),
            constr: Vec::new(),
        }
    }

    pub fn push_work1(&mut self, constr: ExecutionAtom, expr: &ir::Expr) {
        self.work.push(Work(vec![expr.clone()]));
        self.constr.push(constr);
    }

    pub fn push_work(&mut self, constr: ExecutionAtom, exprs: &Vec<ir::Expr>) {
        assert!(!exprs.is_empty());
        self.work.push(Work(exprs.clone()));
        self.constr.push(constr);
    }

    pub fn push_value(&mut self, value: Value) {
        self.values.push(value)
    }

    pub fn next_work(&mut self) -> ExprNext {
        fn pop_end_rev<T>(v: &mut Vec<T>, mut nb: usize) -> Vec<T> {
            if nb > v.len() {
                panic!(
                    "pop_end_rev: trying to get {} values, but {} found",
                    nb,
                    v.len()
                );
            }
            let mut ret = Vec::with_capacity(nb);
            while nb > 0 {
                ret.push(v.pop().unwrap());
                nb -= 1;
            }
            ret
        }

        match self.work.pop() {
            None => {
                let val = self.values.pop().expect("one value if no expression left");
                assert!(self.values.is_empty());
                ExprNext::Finish(val)
            }
            Some(mut exprs) => {
                if exprs.0.is_empty() {
                    let constr = self.constr.pop().unwrap();
                    let nb_args = constr.arity();
                    let args = pop_end_rev(&mut self.values, nb_args);
                    ExprNext::Reduce(constr, args)
                } else {
                    let x = exprs.0.pop().unwrap();
                    self.work.push(Work(exprs.0));
                    ExprNext::Shift(x)
                }
            }
        }
    }
}

pub enum ExprNext {
    Shift(ir::Expr),
    Reduce(ExecutionAtom, Vec<Value>),
    Finish(Value),
}

/// Decompose the work for a given expression
///
/// It either:
/// * Push a value when the work doesn't need further evaluation
/// * Push expressions to evaluate on the work stack and the action to complete
///   when all the evaluation of those expression is commplete
fn work(
    em: &mut ExecutionMachine,
    stack: &mut ExecutionStack,
    e: &ir::Expr,
) -> Result<(), ExecutionError> {
    match e {
        ir::Expr::Literal(_, lit) => stack.push_value(Value::from(lit)),
        ir::Expr::Ident(_, ident) => stack.push_value(em.get_binding(ident)?),
        ir::Expr::List(_, l) => stack.push_work(ExecutionAtom::List(l.len()), l),
        ir::Expr::Lambda(span, args, body) => {
            let val = Value::Fun(
                Location::from_span(span),
                args.clone(),
                body.as_ref().clone(),
            );
            stack.push_value(val)
        }
        ir::Expr::Let(ident, e1, e2) => stack.push_work1(
            ExecutionAtom::Let(ident.clone().unspan(), e2.as_ref().clone()),
            e1,
        ),
        ir::Expr::Then(e1, e2) => stack.push_work1(ExecutionAtom::Then(e2.as_ref().clone()), e1),
        ir::Expr::Call(span, v) => {
            stack.push_work(ExecutionAtom::Call(v.len(), Location::from_span(span)), v)
        }
        ir::Expr::If {
            span: _,
            cond,
            then_expr,
            else_expr,
        } => stack.push_work1(
            ExecutionAtom::ThenElse(then_expr.unspan().clone(), else_expr.unspan().clone()),
            cond.unspan(),
        ),
    };
    Ok(())
}

fn eval(
    em: &mut ExecutionMachine,
    stack: &mut ExecutionStack,
    ea: ExecutionAtom,
    args: Vec<Value>,
) -> Result<(), ExecutionError> {
    fn process_call(
        em: &mut ExecutionMachine,
        stack: &mut ExecutionStack,
        location: &Location,
        args: Vec<Value>,
    ) -> Result<Option<Value>, ExecutionError> {
        if let Some((first, args)) = args.split_first() {
            let k = first.into();
            match first {
                Value::Fun(location, bind_names, fun_stmts) => {
                    em.scope_enter(location);
                    check_arity(bind_names.len(), args.len())?;
                    for (bind_name, arg_value) in bind_names.iter().zip(args.iter()) {
                        em.add_local_binding(bind_name.0.clone().unspan(), arg_value.clone())
                    }
                    stack.push_work1(ExecutionAtom::PopScope, fun_stmts);
                    Ok(None)
                }
                Value::NativeFun(_name, f) => {
                    em.scope_enter(&location);
                    let res = f(em, args)?;
                    em.scope_leave();
                    Ok(Some(res))
                }
                Value::List(_)
                | Value::Bool(_)
                | Value::Number(_)
                | Value::String(_)
                | Value::Decimal(_)
                | Value::Bytes(_)
                | Value::Opaque(_)
                | Value::Unit => Err(ExecutionError::CallingNotFunc {
                    location: location.clone(),
                    value_is: k,
                }),
            }
        } else {
            Ok(Some(Value::Unit))
        }
    }

    match ea {
        ExecutionAtom::List(_) => {
            stack.push_value(Value::List(args));
            Ok(())
        }
        ExecutionAtom::ThenElse(then_expr, else_expr) => {
            let cond_val = args.into_iter().next().unwrap();
            let cond_bool = cond_val.bool()?;

            if cond_bool {
                work(em, stack, &then_expr)?
            } else {
                work(em, stack, &else_expr)?
            }
            Ok(())
        }
        ExecutionAtom::Call(_, loc) => match process_call(em, stack, &loc, args)? {
            None => Ok(()),
            Some(value) => {
                stack.push_value(value);
                Ok(())
            }
        },
        ExecutionAtom::Then(e) => {
            let first_val = args.into_iter().next().unwrap();
            first_val.unit()?;
            work(em, stack, &e)?;
            Ok(())
        }
        ExecutionAtom::PopScope => {
            assert_eq!(args.len(), 1);
            em.scope_leave();
            stack.push_value(args[0].clone());
            Ok(())
        }
        ExecutionAtom::Let(ident, then) => {
            let bind_val = args.into_iter().next().unwrap();
            em.add_local_binding(ident, bind_val);
            work(em, stack, &then)?;
            Ok(())
        }
    }
}

pub fn exec_expr(em: &mut ExecutionMachine, e: &ir::Expr) -> Result<Value, ExecutionError> {
    let mut stack = ExecutionStack::new();
    work(em, &mut stack, e)?;

    loop {
        if em.aborted() {
            return Err(ExecutionError::Abort);
        }
        match stack.next_work() {
            ExprNext::Finish(v) => {
                assert!(stack.values.is_empty());
                assert!(stack.constr.is_empty());
                assert!(stack.work.is_empty());
                return Ok(v);
            }
            ExprNext::Shift(e) => work(em, &mut stack, &e)?,
            ExprNext::Reduce(ea, args) => {
                eval(em, &mut stack, ea, args)?;
            }
        }
    }
}

fn check_arity(expected: usize, got: usize) -> Result<(), ExecutionError> {
    if expected == got {
        Ok(())
    } else {
        Err(ExecutionError::ArityError { expected, got })
    }
}
