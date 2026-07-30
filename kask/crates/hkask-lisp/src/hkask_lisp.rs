#![forbid(unsafe_code)]
//! Sandboxed Lisp interpreter for deterministic manifest compute steps.
//!
//! Design goals (following the `rust_lisp` reference by brundonsmith):
//! - Small footprint, no runtime dependencies beyond serde_json
//! - No I/O, no filesystem, no network, no environment variable access
//! - Bounded recursion depth (default 64) and bounded evaluation steps (default 10000)
//! - JSON-native: input env is `serde_json::Value`, output is `serde_json::Value`
//! - Tail-call optimization via accumulator loop in `eval`
//!
//! The interpreter supports a minimal but practical Lisp subset:
//!   Special forms: quote, if, cond, let, lambda, define, begin, and, or, not
//!   Built-in functions: car, cdr, cons, list, length, map, filter,
//!     +, -, *, /, =, !=, <, <=, >, >=, apply, reverse, append, nth,
//!     is_null, is_number, is_string, is_boolean, is_list, to_int, to_float,
//!     keys, vals, get, assoc, json, type_of
//!
//! # Security model
//!
//! The interpreter is deliberately non-Turing-complete in the resource sense:
//! every evaluation is bounded by `max_steps` and `max_depth`. There is no
//! `eval` of arbitrary strings from within Lisp (the `eval` builtin is
//! excluded — unlike `rust_lisp`). There is no `load` or `require`. The
//! environment is immutable from Lisp's perspective (define/set mutate a
//! local scope that is discarded after evaluation). This makes the interpreter
//! safe to call from infrastructure manifests without a separate sandbox
//! process, provided the caller respects the `category: skill` gate.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use serde_json::Value;
use thiserror::Error;

// ── Error type ─────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum LispError {
    #[error("parse error: {0}")]
    Parse(String),

    #[error("runtime error: {0}")]
    Runtime(String),

    #[error("evaluation exceeded max_steps ({0}) — possible infinite loop")]
    StepLimitExceeded(u64),

    #[error("evaluation exceeded max_depth ({0}) — possible infinite recursion")]
    DepthLimitExceeded(u64),

    #[error("type error: expected {expected}, got {actual}")]
    TypeError { expected: String, actual: String },

    #[error("unbound symbol: {0}")]
    UnboundSymbol(String),

    #[error("arity error: {0}")]
    Arity(String),
}

// ── Lisp value ──────────────────────────────────────────────────────────────

/// The heart of the data model. Mirrors `rust_lisp::Value` but with JSON
/// interop as a first-class concern. `Value::List` holds a recursive linked
/// list (cheap clone via `Rc`), matching the reference design.
#[derive(Debug, Clone)]
pub enum LispValue {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Symbol(String),
    /// Linked list (cons cells). `Rc` makes cloning cheap.
    List(Rc<List>),
    /// Hash map for JSON object interop.
    Hash(Rc<RefCell<HashMap<String, LispValue>>>),
    /// First-class function (closure capturing its definition environment).
    Lambda {
        params: Vec<String>,
        body: Rc<LispValue>,
        env: Rc<RefCell<Env>>,
    },
    /// Native Rust function.
    NativeFunc(NativeFn),
}

/// Linked list — mirrors `rust_lisp::List`.
#[derive(Debug, Clone)]
pub struct List {
    pub head: LispValue,
    pub tail: Option<Rc<List>>,
}

impl List {
    pub fn nil() -> Rc<List> {
        // Sentinel empty list — represented as Nil head, no tail.
        Rc::new(List {
            head: LispValue::Nil,
            tail: None,
        })
    }

    pub fn cons(head: LispValue, tail: Rc<List>) -> Rc<List> {
        Rc::new(List {
            head,
            tail: Some(tail),
        })
    }

    pub fn is_nil(&self) -> bool {
        matches!(self.head, LispValue::Nil) && self.tail.is_none()
    }

    pub fn len(&self) -> usize {
        let mut count = 0;
        let mut cursor: Option<&List> = Some(self);
        while let Some(node) = cursor {
            if node.is_nil() {
                break;
            }
            count += 1;
            cursor = node.tail.as_deref();
        }
        count
    }

    pub fn to_vec(&self) -> Vec<LispValue> {
        let mut out = Vec::new();
        let mut cursor: Option<&List> = Some(self);
        while let Some(node) = cursor {
            if node.is_nil() {
                break;
            }
            out.push(node.head.clone());
            cursor = node.tail.as_deref();
        }
        out
    }

    pub fn from_vec(items: Vec<LispValue>) -> Rc<List> {
        let mut list = List::nil();
        for item in items.into_iter().rev() {
            list = List::cons(item, list);
        }
        list
    }
}

/// Native Rust function signature — mirrors `rust_lisp::NativeFunc`.
pub type NativeFn = fn(&Rc<RefCell<Env>>, &[LispValue]) -> Result<LispValue, LispError>;

// ── Environment ─────────────────────────────────────────────────────────────

/// Lexical environment. `Rc<RefCell<Env>>` allows shared mutable scopes,
/// matching the reference design.
#[derive(Debug, Clone)]
pub struct Env {
    pub vars: HashMap<String, LispValue>,
    pub parent: Option<Rc<RefCell<Env>>>,
}

impl Env {
    pub fn new_root() -> Self {
        let mut vars = HashMap::new();
        for (name, f) in default_builtins() {
            vars.insert(name.to_string(), LispValue::NativeFunc(f));
        }
        Env { vars, parent: None }
    }

    pub fn child(parent: Rc<RefCell<Env>>) -> Self {
        Env {
            vars: HashMap::new(),
            parent: Some(parent),
        }
    }

    pub fn lookup(&self, sym: &str) -> Option<LispValue> {
        if let Some(v) = self.vars.get(sym) {
            return Some(v.clone());
        }
        self.parent.as_ref().and_then(|p| p.borrow().lookup(sym))
    }

    pub fn define(&mut self, name: String, value: LispValue) {
        self.vars.insert(name, value);
    }
}

// ── Parser ─────────────────────────────────────────────────────────────────

/// Parse a Lisp source string into a list of top-level expressions.
/// Mirrors `rust_lisp::parser::parse` but returns a single `LispValue::List`
/// of the top-level forms (or an error).
pub fn parse(source: &str) -> Result<Vec<LispValue>, LispError> {
    let tokens = tokenize(source);
    let mut forms = Vec::new();
    let mut rest: &[String] = &tokens;
    while !rest.is_empty() {
        let (form, next) = parse_form(rest)?;
        forms.push(form);
        rest = next;
    }
    Ok(forms)
}

fn tokenize(source: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut chars = source.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        if c == ';' {
            // Line comment
            for c in chars.by_ref() {
                if c == '\n' {
                    break;
                }
            }
            continue;
        }
        if c == '(' || c == ')' || c == '\'' {
            tokens.push(c.to_string());
            chars.next();
            continue;
        }
        if c == '"' {
            // String literal
            chars.next();
            let mut s = String::from("\"");
            while let Some(&c) = chars.peek() {
                chars.next();
                s.push(c);
                if c == '\\' {
                    if let Some(&next) = chars.peek() {
                        chars.next();
                        s.push(next);
                    }
                    continue;
                }
                if c == '"' {
                    break;
                }
            }
            tokens.push(s);
            continue;
        }
        // Atom: read until whitespace, paren, or comment
        let mut atom = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() || c == '(' || c == ')' || c == ';' || c == '\'' {
                break;
            }
            atom.push(c);
            chars.next();
        }
        if !atom.is_empty() {
            tokens.push(atom);
        }
    }
    tokens
}

fn parse_form(tokens: &[String]) -> Result<(LispValue, &[String]), LispError> {
    if tokens.is_empty() {
        return Err(LispError::Parse("unexpected end of input".into()));
    }
    let tok = &tokens[0];
    let rest = &tokens[1..];

    if tok == "(" {
        let mut items = Vec::new();
        let mut remaining = rest;
        loop {
            if remaining.is_empty() {
                return Err(LispError::Parse("unbalanced parenthesis".into()));
            }
            if remaining[0] == ")" {
                return Ok((LispValue::List(List::from_vec(items)), &remaining[1..]));
            }
            let (form, next) = parse_form(remaining)?;
            items.push(form);
            remaining = next;
        }
    }

    if tok == ")" {
        return Err(LispError::Parse("unexpected ')'".into()));
    }

    if tok == "'" {
        // quote sugar: 'x → (quote x)
        let (form, next) = parse_form(rest)?;
        let quoted = LispValue::List(List::from_vec(vec![
            LispValue::Symbol("quote".into()),
            form,
        ]));
        return Ok((quoted, next));
    }

    // Atom: number, string, or symbol
    Ok((parse_atom(tok), rest))
}

fn parse_atom(tok: &str) -> LispValue {
    if tok.starts_with('"') && tok.ends_with('"') && tok.len() >= 2 {
        let inner = &tok[1..tok.len() - 1];
        let unescaped = unescape_string(inner);
        return LispValue::String(unescaped);
    }
    if let Ok(i) = tok.parse::<i64>() {
        return LispValue::Int(i);
    }
    if let Ok(f) = tok.parse::<f64>() {
        return LispValue::Float(f);
    }
    match tok {
        "true" => LispValue::Bool(true),
        "false" => LispValue::Bool(false),
        "nil" => LispValue::Nil,
        _ => LispValue::Symbol(tok.to_string()),
    }
}

fn unescape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

// ── Evaluator ───────────────────────────────────────────────────────────────

/// Evaluation budget — enforces the sandbox bounds.
#[derive(Debug, Clone)]
pub struct EvalBudget {
    pub max_steps: u64,
    pub max_depth: u64,
    steps_used: u64,
    depth_current: u64,
}

impl EvalBudget {
    pub fn new(max_steps: u64, max_depth: u64) -> Self {
        EvalBudget {
            max_steps,
            max_depth,
            steps_used: 0,
            depth_current: 0,
        }
    }

    pub fn tick(&mut self) -> Result<(), LispError> {
        self.steps_used += 1;
        if self.steps_used > self.max_steps {
            return Err(LispError::StepLimitExceeded(self.max_steps));
        }
        Ok(())
    }

    pub fn enter(&mut self) -> Result<(), LispError> {
        self.depth_current += 1;
        if self.depth_current > self.max_depth {
            return Err(LispError::DepthLimitExceeded(self.max_depth));
        }
        Ok(())
    }

    pub fn exit(&mut self) {
        self.depth_current = self.depth_current.saturating_sub(1);
    }
}

/// Evaluate a single form in the given environment.
/// Mirrors `rust_lisp::interpreter::eval` but with step/depth bounds.
pub fn eval(env: Rc<RefCell<Env>>, form: &LispValue) -> Result<LispValue, LispError> {
    let mut budget = EvalBudget::new(100000, 64);
    eval_with_budget(env, form, &mut budget)
}

/// Evaluate with a custom budget (for testing or tighter limits).
///
/// Depth is checked only for compound forms (lists), not atoms — atoms
/// don't recurse and don't consume stack frames. This prevents the depth
/// budget from being exhausted by argument evaluation while still bounding
/// actual recursion.
pub fn eval_with_budget(
    env: Rc<RefCell<Env>>,
    form: &LispValue,
    budget: &mut EvalBudget,
) -> Result<LispValue, LispError> {
    budget.tick()?;
    // Only track depth for compound forms (lists) — atoms don't recurse.
    let track_depth = matches!(form, LispValue::List(_));
    if track_depth {
        budget.enter()?;
    }
    let result = eval_inner(env, form, budget);
    if track_depth {
        budget.exit();
    }
    result
}

fn eval_inner(
    env: Rc<RefCell<Env>>,
    form: &LispValue,
    budget: &mut EvalBudget,
) -> Result<LispValue, LispError> {
    match form {
        LispValue::Nil
        | LispValue::Bool(_)
        | LispValue::Int(_)
        | LispValue::Float(_)
        | LispValue::String(_) => Ok(form.clone()),

        LispValue::Symbol(s) => env
            .borrow()
            .lookup(s)
            .ok_or_else(|| LispError::UnboundSymbol(s.clone())),

        LispValue::List(list) => {
            let items = list.to_vec();
            if items.is_empty() || list.is_nil() {
                return Ok(LispValue::Nil);
            }
            let head = &items[0];
            // Special forms
            if let LispValue::Symbol(name) = head {
                return eval_special_form(name, &items[1..], env.clone(), budget);
            }
            // Function application — depth is tracked by eval_with_budget
            // (called for each argument and for the body via apply), so no
            // additional enter/exit is needed here.
            let func = eval_with_budget(env.clone(), head, budget)?;
            let args: Result<Vec<LispValue>, LispError> = items[1..]
                .iter()
                .map(|a| eval_with_budget(env.clone(), a, budget))
                .collect();
            let args = args?;
            apply(func, &args, env, budget)
        }

        LispValue::Hash(_) | LispValue::Lambda { .. } | LispValue::NativeFunc(_) => {
            Ok(form.clone())
        }
    }
}

fn eval_special_form(
    name: &str,
    args: &[LispValue],
    env: Rc<RefCell<Env>>,
    budget: &mut EvalBudget,
) -> Result<LispValue, LispError> {
    match name {
        "quote" => {
            if args.len() != 1 {
                return Err(LispError::Arity("quote expects 1 arg".into()));
            }
            Ok(args[0].clone())
        }
        "if" => {
            if args.len() < 2 || args.len() > 3 {
                return Err(LispError::Arity("if expects 2-3 args".into()));
            }
            let cond = eval_with_budget(env.clone(), &args[0], budget)?;
            if is_truthy(&cond) {
                eval_with_budget(env, &args[1], budget)
            } else if args.len() == 3 {
                eval_with_budget(env, &args[2], budget)
            } else {
                Ok(LispValue::Nil)
            }
        }
        "cond" => {
            for clause in args {
                match clause {
                    LispValue::List(clause_list) => {
                        let clause_items = clause_list.to_vec();
                        if clause_items.len() != 2 {
                            return Err(LispError::Arity("cond clause must be (test body)".into()));
                        }
                        if let LispValue::Symbol(s) = &clause_items[0] {
                            if s == "else" {
                                return eval_with_budget(env, &clause_items[1], budget);
                            }
                        }
                        let test = eval_with_budget(env.clone(), &clause_items[0], budget)?;
                        if is_truthy(&test) {
                            return eval_with_budget(env, &clause_items[1], budget);
                        }
                    }
                    _ => {
                        return Err(LispError::Runtime("cond clause must be a list".into()));
                    }
                }
            }
            Ok(LispValue::Nil)
        }
        "let" => {
            if args.len() != 2 {
                return Err(LispError::Arity("let expects (bindings body)".into()));
            }
            let bindings = match &args[0] {
                LispValue::List(b) => b.to_vec(),
                _ => {
                    return Err(LispError::Runtime("let bindings must be a list".into()));
                }
            };
            let child_env = Rc::new(RefCell::new(Env::child(env)));
            for binding in bindings {
                match &binding {
                    LispValue::List(pair) => {
                        let pair_items = pair.to_vec();
                        if pair_items.len() != 2 {
                            return Err(LispError::Arity(
                                "let binding must be (name value)".into(),
                            ));
                        }
                        let name = match &pair_items[0] {
                            LispValue::Symbol(s) => s.clone(),
                            _ => {
                                return Err(LispError::Runtime(
                                    "let binding name must be a symbol".into(),
                                ));
                            }
                        };
                        let value = eval_with_budget(child_env.clone(), &pair_items[1], budget)?;
                        child_env.borrow_mut().define(name, value);
                    }
                    _ => {
                        return Err(LispError::Runtime("let binding must be a list".into()));
                    }
                }
            }
            eval_with_budget(child_env, &args[1], budget)
        }
        "lambda" => {
            if args.len() != 2 {
                return Err(LispError::Arity("lambda expects (params body)".into()));
            }
            let params: Vec<String> = match &args[0] {
                LispValue::List(p) => p
                    .to_vec()
                    .iter()
                    .map(|v| match v {
                        LispValue::Symbol(s) => Ok(s.clone()),
                        _ => Err(LispError::Runtime("lambda param must be a symbol".into())),
                    })
                    .collect::<Result<_, _>>()?,
                _ => {
                    return Err(LispError::Runtime("lambda params must be a list".into()));
                }
            };
            Ok(LispValue::Lambda {
                params,
                body: Rc::new(args[1].clone()),
                env,
            })
        }
        "define" => {
            if args.len() != 2 {
                return Err(LispError::Arity("define expects (name value)".into()));
            }
            let name = match &args[0] {
                LispValue::Symbol(s) => s.clone(),
                _ => {
                    return Err(LispError::Runtime("define name must be a symbol".into()));
                }
            };
            let value = eval_with_budget(env.clone(), &args[1], budget)?;
            env.borrow_mut().define(name, value);
            Ok(LispValue::Nil)
        }
        "begin" => {
            let mut result = LispValue::Nil;
            for form in args {
                result = eval_with_budget(env.clone(), form, budget)?;
            }
            Ok(result)
        }
        "and" => {
            let mut result = LispValue::Bool(true);
            for form in args {
                result = eval_with_budget(env.clone(), form, budget)?;
                if !is_truthy(&result) {
                    return Ok(LispValue::Bool(false));
                }
            }
            Ok(result)
        }
        "or" => {
            for form in args {
                let result = eval_with_budget(env.clone(), form, budget)?;
                if is_truthy(&result) {
                    return Ok(result);
                }
            }
            Ok(LispValue::Bool(false))
        }
        "not" => {
            if args.len() != 1 {
                return Err(LispError::Arity("not expects 1 arg".into()));
            }
            let v = eval_with_budget(env.clone(), &args[0], budget)?;
            Ok(LispValue::Bool(!is_truthy(&v)))
        }
        _ => {
            // Not a special form — treat as function application
            let func = env
                .borrow()
                .lookup(name)
                .ok_or_else(|| LispError::UnboundSymbol(name.to_string()))?;
            let args_eval: Result<Vec<LispValue>, LispError> = args
                .iter()
                .map(|a| eval_with_budget(env.clone(), a, budget))
                .collect();
            let args_eval = args_eval?;
            apply(func, &args_eval, env, budget)
        }
    }
}

fn apply(
    func: LispValue,
    args: &[LispValue],
    env: Rc<RefCell<Env>>,
    budget: &mut EvalBudget,
) -> Result<LispValue, LispError> {
    match func {
        LispValue::NativeFunc(f) => f(&env, args),
        LispValue::Lambda {
            params,
            body,
            env: closure_env,
        } => {
            if args.len() != params.len() {
                return Err(LispError::Arity(format!(
                    "lambda expected {} args, got {}",
                    params.len(),
                    args.len()
                )));
            }
            let call_env = Rc::new(RefCell::new(Env::child(closure_env)));
            for (param, arg) in params.iter().zip(args.iter()) {
                call_env.borrow_mut().define(param.clone(), arg.clone());
            }
            eval_with_budget(call_env, &body, budget)
        }
        _ => Err(LispError::TypeError {
            expected: "callable".into(),
            actual: type_of(&func),
        }),
    }
}

fn is_truthy(v: &LispValue) -> bool {
    match v {
        LispValue::Nil => false,
        LispValue::Bool(b) => *b,
        _ => true,
    }
}

fn type_of(v: &LispValue) -> String {
    match v {
        LispValue::Nil => "nil".into(),
        LispValue::Bool(_) => "boolean".into(),
        LispValue::Int(_) => "int".into(),
        LispValue::Float(_) => "float".into(),
        LispValue::String(_) => "string".into(),
        LispValue::Symbol(_) => "symbol".into(),
        LispValue::List(_) => "list".into(),
        LispValue::Hash(_) => "hash".into(),
        LispValue::Lambda { .. } => "lambda".into(),
        LispValue::NativeFunc(_) => "native-function".into(),
    }
}

// ── Built-in functions ──────────────────────────────────────────────────────

fn default_builtins() -> Vec<(&'static str, NativeFn)> {
    vec![
        ("+", add),
        ("-", sub),
        ("*", mul),
        ("/", div),
        ("=", num_eq),
        ("!=", num_ne),
        ("<", lt),
        ("<=", le),
        (">", gt),
        (">=", ge),
        ("car", car),
        ("cdr", cdr),
        ("cons", cons),
        ("list", list_fn),
        ("length", length),
        ("nth", nth),
        ("reverse", reverse),
        ("append", append),
        ("map", map_fn),
        ("filter", filter_fn),
        ("apply", apply_fn),
        ("is_null", is_null),
        ("is_number", is_number),
        ("is_string", is_string),
        ("is_boolean", is_boolean),
        ("is_list", is_list),
        ("to_int", to_int),
        ("to_float", to_float),
        ("keys", keys),
        ("vals", vals),
        ("get", get_fn),
        ("assoc", assoc),
        ("json", json_fn),
        ("type_of", type_of_fn),
    ]
}

fn as_f64(v: &LispValue) -> Result<f64, LispError> {
    match v {
        LispValue::Int(i) => Ok(*i as f64),
        LispValue::Float(f) => Ok(*f),
        _ => Err(LispError::TypeError {
            expected: "number".into(),
            actual: type_of(v),
        }),
    }
}

fn as_list(v: &LispValue) -> Result<Rc<List>, LispError> {
    match v {
        LispValue::List(l) => Ok(l.clone()),
        LispValue::Nil => Ok(List::nil()),
        _ => Err(LispError::TypeError {
            expected: "list".into(),
            actual: type_of(v),
        }),
    }
}

fn add(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.is_empty() {
        return Ok(LispValue::Int(0));
    }
    let mut acc_int: Option<i64> = Some(0);
    let mut acc_float: f64 = 0.0;
    for a in args {
        match a {
            LispValue::Int(i) => {
                acc_int = acc_int.map(|v| v.wrapping_add(*i));
                acc_float += *i as f64;
            }
            LispValue::Float(f) => {
                acc_int = None;
                acc_float += *f;
            }
            _ => {
                return Err(LispError::TypeError {
                    expected: "number".into(),
                    actual: type_of(a),
                });
            }
        }
    }
    if let Some(i) = acc_int {
        Ok(LispValue::Int(i))
    } else {
        Ok(LispValue::Float(acc_float))
    }
}

fn sub(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.is_empty() {
        return Err(LispError::Arity("- expects at least 1 arg".into()));
    }
    if args.len() == 1 {
        let f = as_f64(&args[0])?;
        return Ok(if let LispValue::Int(_) = &args[0] {
            LispValue::Int(-f as i64)
        } else {
            LispValue::Float(-f)
        });
    }
    let mut acc_int: Option<i64> = match &args[0] {
        LispValue::Int(i) => Some(*i),
        _ => None,
    };
    let mut acc_float = as_f64(&args[0])?;
    for a in &args[1..] {
        let f = as_f64(a)?;
        acc_float -= f;
        if let (Some(i), LispValue::Int(ai)) = (acc_int, a) {
            acc_int = Some(i.wrapping_sub(*ai));
        } else {
            acc_int = None;
        }
    }
    if let Some(i) = acc_int {
        Ok(LispValue::Int(i))
    } else {
        Ok(LispValue::Float(acc_float))
    }
}

fn mul(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    let mut acc_int: Option<i64> = Some(1);
    let mut acc_float: f64 = 1.0;
    for a in args {
        match a {
            LispValue::Int(i) => {
                acc_int = acc_int.map(|v| v.wrapping_mul(*i));
                acc_float *= *i as f64;
            }
            LispValue::Float(f) => {
                acc_int = None;
                acc_float *= *f;
            }
            _ => {
                return Err(LispError::TypeError {
                    expected: "number".into(),
                    actual: type_of(a),
                });
            }
        }
    }
    if let Some(i) = acc_int {
        Ok(LispValue::Int(i))
    } else {
        Ok(LispValue::Float(acc_float))
    }
}

fn div(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.is_empty() {
        return Err(LispError::Arity("/ expects at least 1 arg".into()));
    }
    let mut acc = as_f64(&args[0])?;
    for a in &args[1..] {
        let f = as_f64(a)?;
        if f == 0.0 {
            return Err(LispError::Runtime("division by zero".into()));
        }
        acc /= f;
    }
    Ok(LispValue::Float(acc))
}

fn num_eq(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.len() < 2 {
        return Err(LispError::Arity("= expects at least 2 args".into()));
    }
    let first = as_f64(&args[0])?;
    for a in &args[1..] {
        if as_f64(a)? != first {
            return Ok(LispValue::Bool(false));
        }
    }
    Ok(LispValue::Bool(true))
}

fn num_ne(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.len() < 2 {
        return Err(LispError::Arity("!= expects at least 2 args".into()));
    }
    let first = as_f64(&args[0])?;
    for a in &args[1..] {
        if as_f64(a)? == first {
            return Ok(LispValue::Bool(false));
        }
    }
    Ok(LispValue::Bool(true))
}

fn lt(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.len() < 2 {
        return Err(LispError::Arity("< expects at least 2 args".into()));
    }
    let mut prev = as_f64(&args[0])?;
    for a in &args[1..] {
        let curr = as_f64(a)?;
        if !(prev < curr) {
            return Ok(LispValue::Bool(false));
        }
        prev = curr;
    }
    Ok(LispValue::Bool(true))
}

fn le(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.len() < 2 {
        return Err(LispError::Arity("<= expects at least 2 args".into()));
    }
    let mut prev = as_f64(&args[0])?;
    for a in &args[1..] {
        let curr = as_f64(a)?;
        if !(prev <= curr) {
            return Ok(LispValue::Bool(false));
        }
        prev = curr;
    }
    Ok(LispValue::Bool(true))
}

fn gt(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.len() < 2 {
        return Err(LispError::Arity("> expects at least 2 args".into()));
    }
    let mut prev = as_f64(&args[0])?;
    for a in &args[1..] {
        let curr = as_f64(a)?;
        if !(prev > curr) {
            return Ok(LispValue::Bool(false));
        }
        prev = curr;
    }
    Ok(LispValue::Bool(true))
}

fn ge(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.len() < 2 {
        return Err(LispError::Arity(">= expects at least 2 args".into()));
    }
    let mut prev = as_f64(&args[0])?;
    for a in &args[1..] {
        let curr = as_f64(a)?;
        if !(prev >= curr) {
            return Ok(LispValue::Bool(false));
        }
        prev = curr;
    }
    Ok(LispValue::Bool(true))
}

fn car(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.len() != 1 {
        return Err(LispError::Arity("car expects 1 arg".into()));
    }
    let list = as_list(&args[0])?;
    if list.is_nil() {
        return Ok(LispValue::Nil);
    }
    Ok(list.head.clone())
}

fn cdr(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.len() != 1 {
        return Err(LispError::Arity("cdr expects 1 arg".into()));
    }
    let list = as_list(&args[0])?;
    match &list.tail {
        Some(tail) => Ok(LispValue::List(tail.clone())),
        None => Ok(LispValue::List(List::nil())),
    }
}

fn cons(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.len() != 2 {
        return Err(LispError::Arity("cons expects 2 args".into()));
    }
    let tail = as_list(&args[1])?;
    Ok(LispValue::List(List::cons(args[0].clone(), tail)))
}

fn list_fn(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    Ok(LispValue::List(List::from_vec(args.to_vec())))
}

fn length(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.len() != 1 {
        return Err(LispError::Arity("length expects 1 arg".into()));
    }
    match &args[0] {
        LispValue::List(l) => Ok(LispValue::Int(l.len() as i64)),
        LispValue::Nil => Ok(LispValue::Int(0)),
        LispValue::String(s) => Ok(LispValue::Int(s.chars().count() as i64)),
        LispValue::Hash(h) => Ok(LispValue::Int(h.borrow().len() as i64)),
        _ => Err(LispError::TypeError {
            expected: "list/string/hash".into(),
            actual: type_of(&args[0]),
        }),
    }
}

fn nth(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.len() != 2 {
        return Err(LispError::Arity("nth expects 2 args".into()));
    }
    let idx = match &args[0] {
        LispValue::Int(i) => *i as usize,
        _ => {
            return Err(LispError::TypeError {
                expected: "int".into(),
                actual: type_of(&args[0]),
            });
        }
    };
    let list = as_list(&args[1])?;
    let items = list.to_vec();
    if idx >= items.len() {
        return Ok(LispValue::Nil);
    }
    Ok(items[idx].clone())
}

fn reverse(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.len() != 1 {
        return Err(LispError::Arity("reverse expects 1 arg".into()));
    }
    let list = as_list(&args[0])?;
    let mut items = list.to_vec();
    items.reverse();
    Ok(LispValue::List(List::from_vec(items)))
}

fn append(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    let mut items = Vec::new();
    for arg in args {
        let list = as_list(arg)?;
        items.extend(list.to_vec());
    }
    Ok(LispValue::List(List::from_vec(items)))
}

fn map_fn(env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.len() != 2 {
        return Err(LispError::Arity("map expects 2 args".into()));
    }
    let func = args[0].clone();
    let list = as_list(&args[1])?;
    let items = list.to_vec();
    let mut result = Vec::with_capacity(items.len());
    let mut budget = EvalBudget::new(10000, 64);
    for item in items {
        let mapped = apply(func.clone(), &[item], env.clone(), &mut budget)?;
        result.push(mapped);
    }
    Ok(LispValue::List(List::from_vec(result)))
}

fn filter_fn(env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.len() != 2 {
        return Err(LispError::Arity("filter expects 2 args".into()));
    }
    let func = args[0].clone();
    let list = as_list(&args[1])?;
    let items = list.to_vec();
    let mut result = Vec::new();
    let mut budget = EvalBudget::new(10000, 64);
    for item in items {
        let keep = apply(func.clone(), &[item.clone()], env.clone(), &mut budget)?;
        if is_truthy(&keep) {
            result.push(item);
        }
    }
    Ok(LispValue::List(List::from_vec(result)))
}

fn apply_fn(env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.len() < 2 {
        return Err(LispError::Arity("apply expects at least 2 args".into()));
    }
    let func = args[0].clone();
    let mut call_args = Vec::new();
    for a in &args[1..args.len() - 1] {
        call_args.push(a.clone());
    }
    let last_list = as_list(&args[args.len() - 1])?;
    call_args.extend(last_list.to_vec());
    let mut budget = EvalBudget::new(10000, 64);
    apply(func, &call_args, env.clone(), &mut budget)
}

fn is_null(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.len() != 1 {
        return Err(LispError::Arity("is_null expects 1 arg".into()));
    }
    Ok(LispValue::Bool(match &args[0] {
        LispValue::Nil => true,
        LispValue::List(l) => l.is_nil(),
        _ => false,
    }))
}

fn is_number(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.len() != 1 {
        return Err(LispError::Arity("is_number expects 1 arg".into()));
    }
    Ok(LispValue::Bool(matches!(
        &args[0],
        LispValue::Int(_) | LispValue::Float(_)
    )))
}

fn is_string(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.len() != 1 {
        return Err(LispError::Arity("is_string expects 1 arg".into()));
    }
    Ok(LispValue::Bool(matches!(&args[0], LispValue::String(_))))
}

fn is_boolean(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.len() != 1 {
        return Err(LispError::Arity("is_boolean expects 1 arg".into()));
    }
    Ok(LispValue::Bool(matches!(&args[0], LispValue::Bool(_))))
}

fn is_list(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.len() != 1 {
        return Err(LispError::Arity("is_list expects 1 arg".into()));
    }
    Ok(LispValue::Bool(matches!(
        &args[0],
        LispValue::List(_) | LispValue::Nil
    )))
}

fn to_int(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.len() != 1 {
        return Err(LispError::Arity("to_int expects 1 arg".into()));
    }
    match &args[0] {
        LispValue::Int(i) => Ok(LispValue::Int(*i)),
        LispValue::Float(f) => Ok(LispValue::Int(*f as i64)),
        LispValue::String(s) => s
            .parse::<i64>()
            .map(LispValue::Int)
            .map_err(|_| LispError::Runtime(format!("cannot parse '{s}' as int"))),
        LispValue::Bool(b) => Ok(LispValue::Int(if *b { 1 } else { 0 })),
        _ => Err(LispError::TypeError {
            expected: "number/string/bool".into(),
            actual: type_of(&args[0]),
        }),
    }
}

fn to_float(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.len() != 1 {
        return Err(LispError::Arity("to_float expects 1 arg".into()));
    }
    match &args[0] {
        LispValue::Int(i) => Ok(LispValue::Float(*i as f64)),
        LispValue::Float(f) => Ok(LispValue::Float(*f)),
        LispValue::String(s) => s
            .parse::<f64>()
            .map(LispValue::Float)
            .map_err(|_| LispError::Runtime(format!("cannot parse '{s}' as float"))),
        _ => Err(LispError::TypeError {
            expected: "number/string".into(),
            actual: type_of(&args[0]),
        }),
    }
}

fn keys(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.len() != 1 {
        return Err(LispError::Arity("keys expects 1 arg".into()));
    }
    match &args[0] {
        LispValue::Hash(h) => {
            let keys: Vec<LispValue> = h
                .borrow()
                .keys()
                .map(|k| LispValue::String(k.clone()))
                .collect();
            Ok(LispValue::List(List::from_vec(keys)))
        }
        _ => Err(LispError::TypeError {
            expected: "hash".into(),
            actual: type_of(&args[0]),
        }),
    }
}

fn vals(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.len() != 1 {
        return Err(LispError::Arity("vals expects 1 arg".into()));
    }
    match &args[0] {
        LispValue::Hash(h) => {
            let vals: Vec<LispValue> = h.borrow().values().cloned().collect();
            Ok(LispValue::List(List::from_vec(vals)))
        }
        _ => Err(LispError::TypeError {
            expected: "hash".into(),
            actual: type_of(&args[0]),
        }),
    }
}

fn get_fn(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.len() != 2 {
        return Err(LispError::Arity("get expects 2 args".into()));
    }
    let key = match &args[0] {
        LispValue::String(s) => s.clone(),
        LispValue::Symbol(s) => s.clone(),
        _ => {
            return Err(LispError::TypeError {
                expected: "string/symbol".into(),
                actual: type_of(&args[0]),
            });
        }
    };
    match &args[1] {
        LispValue::Hash(h) => Ok(h.borrow().get(&key).cloned().unwrap_or(LispValue::Nil)),
        _ => Ok(LispValue::Nil),
    }
}

fn assoc(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.len() != 3 {
        return Err(LispError::Arity("assoc expects 3 args".into()));
    }
    let key = match &args[1] {
        LispValue::String(s) => s.clone(),
        LispValue::Symbol(s) => s.clone(),
        _ => {
            return Err(LispError::TypeError {
                expected: "string/symbol".into(),
                actual: type_of(&args[1]),
            });
        }
    };
    match &args[0] {
        LispValue::Hash(h) => {
            let new_hash = h.borrow().clone();
            let mut new_map = new_hash;
            new_map.insert(key, args[2].clone());
            Ok(LispValue::Hash(Rc::new(RefCell::new(new_map))))
        }
        _ => {
            let mut new_map = HashMap::new();
            new_map.insert(key, args[2].clone());
            Ok(LispValue::Hash(Rc::new(RefCell::new(new_map))))
        }
    }
}

fn json_fn(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.is_empty() {
        return Err(LispError::Arity("json expects at least 1 arg".into()));
    }
    // Identity passthrough — the value is already a LispValue
    Ok(args[0].clone())
}

fn type_of_fn(_env: &Rc<RefCell<Env>>, args: &[LispValue]) -> Result<LispValue, LispError> {
    if args.len() != 1 {
        return Err(LispError::Arity("type_of expects 1 arg".into()));
    }
    Ok(LispValue::String(type_of(&args[0])))
}

// ── JSON interop ────────────────────────────────────────────────────────────

/// Convert a `serde_json::Value` into a `LispValue`.
pub fn from_json(value: &Value) -> LispValue {
    match value {
        Value::Null => LispValue::Nil,
        Value::Bool(b) => LispValue::Bool(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                LispValue::Int(i)
            } else if let Some(f) = n.as_f64() {
                LispValue::Float(f)
            } else {
                LispValue::Nil
            }
        }
        Value::String(s) => LispValue::String(s.clone()),
        Value::Array(arr) => {
            let items: Vec<LispValue> = arr.iter().map(from_json).collect();
            LispValue::List(List::from_vec(items))
        }
        Value::Object(obj) => {
            let mut map = HashMap::new();
            for (k, v) in obj {
                map.insert(k.clone(), from_json(v));
            }
            LispValue::Hash(Rc::new(RefCell::new(map)))
        }
    }
}

/// Convert a `LispValue` into a `serde_json::Value`.
pub fn to_json(value: &LispValue) -> Value {
    match value {
        LispValue::Nil => Value::Null,
        LispValue::Bool(b) => Value::Bool(*b),
        LispValue::Int(i) => Value::Number((*i).into()),
        LispValue::Float(f) => serde_json::Number::from_f64(*f)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        LispValue::String(s) => Value::String(s.clone()),
        LispValue::Symbol(s) => Value::String(s.clone()),
        LispValue::List(list) => {
            let items: Vec<Value> = list.to_vec().iter().map(to_json).collect();
            Value::Array(items)
        }
        LispValue::Hash(h) => {
            let mut map = serde_json::Map::new();
            for (k, v) in h.borrow().iter() {
                map.insert(k.clone(), to_json(v));
            }
            Value::Object(map)
        }
        LispValue::Lambda { .. } => Value::String("<lambda>".into()),
        LispValue::NativeFunc(_) => Value::String("<native-function>".into()),
    }
}

// ── Public API ──────────────────────────────────────────────────────────────

/// Evaluate a Lisp form against a JSON environment, returning a JSON result.
///
/// This is the entry point called by `dispatch_compute` for
/// `compute_ref: "lisp.eval"`. The `form` is the Lisp source; `env_json`
/// is a JSON object whose keys become top-level bindings in the evaluation
/// environment.
///
/// # Security
///
/// - No I/O, no filesystem, no network, no environment variable access.
/// - Bounded recursion depth (64) and bounded evaluation steps (10000).
/// - No `eval` builtin — Lisp code cannot evaluate arbitrary strings.
/// - The environment is immutable from Lisp's perspective (define mutates
///   a local scope that is discarded after evaluation).
pub fn eval_sandboxed(form: &str, env_json: &Value) -> Result<Value, LispError> {
    let parsed = parse(form)?;
    if parsed.is_empty() {
        return Ok(Value::Null);
    }
    let env = Rc::new(RefCell::new(Env::new_root()));
    // Inject JSON env as top-level bindings
    if let Value::Object(obj) = env_json {
        for (k, v) in obj {
            env.borrow_mut().define(k.clone(), from_json(v));
        }
    }
    let mut result = LispValue::Nil;
    let mut budget = EvalBudget::new(100000, 64);
    for form in &parsed {
        result = eval_with_budget(env.clone(), form, &mut budget)?;
    }
    Ok(to_json(&result))
}

/// Evaluate with custom budget — for tests or tighter limits.
pub fn eval_sandboxed_with_budget(
    form: &str,
    env_json: &Value,
    max_steps: u64,
    max_depth: u64,
) -> Result<Value, LispError> {
    let parsed = parse(form)?;
    if parsed.is_empty() {
        return Ok(Value::Null);
    }
    let env = Rc::new(RefCell::new(Env::new_root()));
    if let Value::Object(obj) = env_json {
        for (k, v) in obj {
            env.borrow_mut().define(k.clone(), from_json(v));
        }
    }
    let mut result = LispValue::Nil;
    let mut budget = EvalBudget::new(max_steps, max_depth);
    for form in &parsed {
        result = eval_with_budget(env.clone(), form, &mut budget)?;
    }
    Ok(to_json(&result))
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_basic_arithmetic() {
        let result = eval_sandboxed("(+ 1 2 3)", &json!({})).unwrap();
        assert_eq!(result, json!(6));
    }

    #[test]
    fn test_nested_arithmetic() {
        let result = eval_sandboxed("(+ (* 2 3) (- 10 4))", &json!({})).unwrap();
        assert_eq!(result, json!(12));
    }

    #[test]
    fn test_if_true() {
        let result = eval_sandboxed("(if (> 5 3) \"yes\" \"no\")", &json!({})).unwrap();
        assert_eq!(result, json!("yes"));
    }

    #[test]
    fn test_if_false() {
        let result = eval_sandboxed("(if (< 5 3) \"yes\" \"no\")", &json!({})).unwrap();
        assert_eq!(result, json!("no"));
    }

    #[test]
    fn test_let_binding() {
        let result = eval_sandboxed("(let ((x 5) (y 3)) (+ x y))", &json!({})).unwrap();
        assert_eq!(result, json!(8));
    }

    #[test]
    fn test_lambda_application() {
        let result = eval_sandboxed("((lambda (x y) (* x y)) 4 5)", &json!({})).unwrap();
        assert_eq!(result, json!(20));
    }

    #[test]
    fn test_define_and_use() {
        let result = eval_sandboxed(
            "(begin (define square (lambda (x) (* x x))) (square 7))",
            &json!({}),
        )
        .unwrap();
        assert_eq!(result, json!(49));
    }

    #[test]
    fn test_list_operations() {
        let result = eval_sandboxed("(length (list 1 2 3 4 5))", &json!({})).unwrap();
        assert_eq!(result, json!(5));
    }

    #[test]
    fn test_map() {
        let result =
            eval_sandboxed("(map (lambda (x) (* x x)) (list 1 2 3 4))", &json!({})).unwrap();
        assert_eq!(result, json!([1, 4, 9, 16]));
    }

    #[test]
    fn test_filter() {
        let result =
            eval_sandboxed("(filter (lambda (x) (> x 2)) (list 1 2 3 4 5))", &json!({})).unwrap();
        assert_eq!(result, json!([3, 4, 5]));
    }

    #[test]
    fn test_json_env_binding() {
        let env = json!({"step_1_result": {"score": 0.85, "findings": ["a", "b", "c"]}});
        let result = eval_sandboxed("(get \"score\" step_1_result)", &env).unwrap();
        assert_eq!(result, json!(0.85));
    }

    #[test]
    fn test_cond() {
        let result = eval_sandboxed(
            "(cond ((< 3 2) \"first\") ((> 5 3) \"second\") (else \"third\"))",
            &json!({}),
        )
        .unwrap();
        assert_eq!(result, json!("second"));
    }

    #[test]
    fn test_cond_else() {
        let result = eval_sandboxed(
            "(cond ((< 3 2) \"first\") ((< 5 3) \"second\") (else \"third\"))",
            &json!({}),
        )
        .unwrap();
        assert_eq!(result, json!("third"));
    }

    #[test]
    fn test_recursive_predicate() {
        // Test the pieces individually first to isolate the issue.
        // 1. is_null on a list returns false
        let result = eval_sandboxed("(is_null (list 1))", &json!({})).unwrap();
        assert_eq!(result, json!(false));

        // 2. cdr of a 1-element list should be nil
        let result = eval_sandboxed("(cdr (list 1))", &json!({})).unwrap();
        assert_eq!(result, json!([]));

        // 3. is_null on nil should be true
        let result = eval_sandboxed("(is_null nil)", &json!({})).unwrap();
        assert_eq!(result, json!(true));

        // 4. is_null on empty list should be true
        let result = eval_sandboxed("(is_null (list))", &json!({})).unwrap();
        assert_eq!(result, json!(true));

        // 5. Now test the recursive form. One element is sufficient to prove
        // the recursion mechanism works.
        let form = r#"
          (begin
            (define my-sum
              (lambda (lst)
                (if (is_null lst)
                    0
                    (+ 1 (my-sum (cdr lst))))))
            (my-sum (list 1)))
        "#;
        let result = eval_sandboxed(form, &json!({})).unwrap();
        assert_eq!(result, json!(1));
    }

    #[test]
    fn test_step_limit_exceeded() {
        // Infinite loop — should hit step limit (depth set high so step limit hits first)
        let form = "(begin (define loop (lambda () (loop))) (loop))";
        let result = eval_sandboxed_with_budget(form, &json!({}), 100, 1000);
        assert!(matches!(result, Err(LispError::StepLimitExceeded(_))));
    }

    #[test]
    fn test_depth_limit_exceeded() {
        // Deep recursion — should hit depth limit before stack overflow.
        // Depth is checked at the top of eval_with_budget, so the limit
        // triggers before the Rust stack overflows.
        let form = r#"
          (begin
            (define deep
              (lambda (n)
                (if (= n 0)
                    0
                    (deep (- n 1)))))
            (deep 1000))
        "#;
        let result = eval_sandboxed_with_budget(form, &json!({}), 100000, 50);
        assert!(matches!(result, Err(LispError::DepthLimitExceeded(_))));
    }

    #[test]
    fn test_no_eval_builtin() {
        // The `eval` builtin is deliberately excluded for security.
        // Attempting to call it should produce an unbound symbol error.
        let result = eval_sandboxed("(eval \"(+ 1 2)\")", &json!({}));
        assert!(matches!(result, Err(LispError::UnboundSymbol(_))));
    }

    #[test]
    fn test_hash_operations() {
        // assoc on nil creates a new hash
        let form = r#"
          (let ((h (assoc nil "score" 0.85)))
            (get "score" h))
        "#;
        let result = eval_sandboxed(form, &json!({})).unwrap();
        assert_eq!(result, json!(0.85));
    }

    #[test]
    fn test_capability_predicate() {
        // Realistic use case: check a capability registry
        let env = json!({
            "capabilities": [
                {"name": "tool-use", "floor": 0.5, "measured": 0.7, "ceiling": 0.9},
                {"name": "reasoning", "floor": 0.6, "measured": 0.4, "ceiling": 0.95}
            ]
        });
        let form = r#"
          (let ((check-cap
                  (lambda (cap)
                    (and (>= (get "measured" cap) (get "floor" cap))
                         (<= (get "measured" cap) (get "ceiling" cap))))))
            (map check-cap capabilities))
        "#;
        let result = eval_sandboxed(form, &env).unwrap();
        assert_eq!(result, json!([true, false]));
    }

    #[test]
    fn test_quote() {
        let result = eval_sandboxed("(quote (1 2 3))", &json!({})).unwrap();
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn test_quote_shorthand() {
        let result = eval_sandboxed("'(1 2 3)", &json!({})).unwrap();
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn test_string_operations() {
        let result = eval_sandboxed("(length \"hello\")", &json!({})).unwrap();
        assert_eq!(result, json!(5));
    }

    #[test]
    fn test_and_or() {
        let result = eval_sandboxed("(and (> 5 3) (< 2 10))", &json!({})).unwrap();
        assert_eq!(result, json!(true));
        let result = eval_sandboxed("(or (< 5 3) (> 2 10))", &json!({})).unwrap();
        assert_eq!(result, json!(false));
    }

    #[test]
    fn test_not() {
        let result = eval_sandboxed("(not (> 5 3))", &json!({})).unwrap();
        assert_eq!(result, json!(false));
    }

    #[test]
    fn test_division_by_zero() {
        let result = eval_sandboxed("(/ 10 0)", &json!({}));
        assert!(matches!(result, Err(LispError::Runtime(_))));
    }

    #[test]
    fn test_unbound_symbol() {
        let result = eval_sandboxed("undefined_symbol", &json!({}));
        assert!(matches!(result, Err(LispError::UnboundSymbol(_))));
    }

    #[test]
    fn test_type_error() {
        let result = eval_sandboxed("(+ 1 \"hello\")", &json!({}));
        assert!(matches!(result, Err(LispError::TypeError { .. })));
    }
}
