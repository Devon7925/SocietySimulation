//! A tiny, safe expression language used inside laws.
//!
//! Laws are data, not code: every condition ("who does this apply to?") and
//! amount ("how much?") is a short expression evaluated against a [`Scope`].
//!
//! ```text
//! age >= 65 && !employed
//! income > 20 ? 0.4 : 0.1
//! good == "ale" && (hour >= 22 || hour < 6)
//! job in ["farmer", "brewer"] || has("licensed_brewer")
//! ```
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    List(Vec<Value>),
}

impl Value {
    pub fn truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Num(n) => *n != 0.0 && !n.is_nan(),
            Value::Str(s) => !s.is_empty(),
            Value::List(l) => !l.is_empty(),
        }
    }
    pub fn num(&self) -> f64 {
        match self {
            Value::Num(n) => *n,
            Value::Bool(true) => 1.0,
            Value::Str(s) => s.parse().unwrap_or(0.0),
            _ => 0.0,
        }
    }
    pub fn as_str(&self) -> &str {
        match self {
            Value::Str(s) => s,
            _ => "",
        }
    }
}
impl From<bool> for Value { fn from(b: bool) -> Self { Value::Bool(b) } }
impl From<f32> for Value { fn from(n: f32) -> Self { Value::Num(n as f64) } }
impl From<f64> for Value { fn from(n: f64) -> Self { Value::Num(n) } }
impl From<i32> for Value { fn from(n: i32) -> Self { Value::Num(n as f64) } }
impl From<u32> for Value { fn from(n: u32) -> Self { Value::Num(n as f64) } }
impl From<usize> for Value { fn from(n: usize) -> Self { Value::Num(n as f64) } }
impl From<&str> for Value { fn from(s: &str) -> Self { Value::Str(s.to_string()) } }
impl From<String> for Value { fn from(s: String) -> Self { Value::Str(s) } }

/// Where an expression's names come from. `path` is a dotted name split into
/// parts (`worker.age` -> `["worker", "age"]`). Unknown names yield `Null`.
pub trait Scope {
    fn get(&self, path: &[String]) -> Value;
    /// Scope-specific functions such as `has("status")`. Return `None` if unknown.
    fn call(&self, _name: &str, _args: &[Value]) -> Option<Value> {
        None
    }
    /// Does this scope define `root` as a variable (used by validation)?
    fn knows(&self, _root: &str) -> bool {
        true
    }
}

/// A scope with no variables at all.
pub struct EmptyScope;
impl Scope for EmptyScope {
    fn get(&self, _: &[String]) -> Value { Value::Null }
}

#[derive(Debug)]
pub enum Ast {
    Lit(Value),
    Var(Vec<String>),
    List(Vec<Ast>),
    Not(Box<Ast>),
    Neg(Box<Ast>),
    Tern(Box<Ast>, Box<Ast>, Box<Ast>),
    Call(String, Vec<Ast>),
    Bin(&'static str, Box<Ast>, Box<Ast>),
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Str(String),
    Lit(Value),
    Id(String),
    Op(&'static str),
}

const OPS2: [&str; 6] = ["&&", "||", "==", "!=", "<=", ">="];
const OPS1: [&str; 15] = ["+", "-", "*", "/", "%", "<", ">", "!", "?", ":", "(", ")", ",", ".", "["];

fn tokenize(src: &str) -> Result<Vec<Tok>, String> {
    let c: Vec<char> = src.chars().collect();
    let n = c.len();
    let mut i = 0;
    let mut out = Vec::new();
    while i < n {
        let ch = c[i];
        if ch.is_whitespace() { i += 1; continue; }
        if ch.is_ascii_digit() || (ch == '.' && i + 1 < n && c[i + 1].is_ascii_digit()) {
            let mut j = i;
            while j < n && (c[j].is_ascii_digit() || c[j] == '.') { j += 1; }
            let s: String = c[i..j].iter().collect();
            let mut v: f64 = s.parse().map_err(|_| format!("bad number \"{s}\""))?;
            if j < n && c[j] == '%' && !(j + 1 < n && (c[j + 1].is_alphanumeric() || c[j + 1] == '(' || c[j + 1] == ' ' && j + 2 < n && (c[j + 2].is_alphanumeric() || c[j + 2] == '('))) {
                v /= 100.0; // "15%" reads as 0.15
                j += 1;
            }
            out.push(Tok::Num(v));
            i = j;
            continue;
        }
        if ch == '"' || ch == '\'' {
            let mut j = i + 1;
            let mut s = String::new();
            while j < n && c[j] != ch {
                if c[j] == '\\' && j + 1 < n { j += 1; }
                s.push(c[j]);
                j += 1;
            }
            if j >= n { return Err(format!("unterminated string in \"{src}\"")); }
            out.push(Tok::Str(s));
            i = j + 1;
            continue;
        }
        if ch.is_ascii_alphabetic() || ch == '_' {
            let mut j = i;
            while j < n && (c[j].is_ascii_alphanumeric() || c[j] == '_') { j += 1; }
            let word: String = c[i..j].iter().collect();
            match word.to_ascii_lowercase().as_str() {
                "in" => out.push(Tok::Op("in")),
                "and" => out.push(Tok::Op("&&")),
                "or" => out.push(Tok::Op("||")),
                "not" => out.push(Tok::Op("!")),
                "true" => out.push(Tok::Lit(Value::Bool(true))),
                "false" => out.push(Tok::Lit(Value::Bool(false))),
                "null" => out.push(Tok::Lit(Value::Null)),
                _ => out.push(Tok::Id(word)),
            }
            i = j;
            continue;
        }
        if i + 2 < n && (c[i..i + 3] == ['=', '=', '='] || c[i..i + 3] == ['!', '=', '=']) {
            out.push(Tok::Op(if ch == '=' { "==" } else { "!=" }));
            i += 3;
            continue;
        }
        if i + 1 < n {
            let two: String = c[i..i + 2].iter().collect();
            if let Some(op) = OPS2.iter().find(|o| **o == two) {
                out.push(Tok::Op(op));
                i += 2;
                continue;
            }
        }
        if ch == ']' { out.push(Tok::Op("]")); i += 1; continue; }
        let one = ch.to_string();
        if let Some(op) = OPS1.iter().find(|o| **o == one) {
            out.push(Tok::Op(op));
            i += 1;
            continue;
        }
        return Err(format!("unexpected character \"{ch}\" in \"{src}\""));
    }
    Ok(out)
}

fn prec(op: &str) -> Option<u8> {
    Some(match op {
        "||" => 1,
        "&&" => 2,
        "==" | "!=" => 3,
        "<" | "<=" | ">" | ">=" | "in" => 4,
        "+" | "-" => 5,
        "*" | "/" | "%" => 6,
        _ => return None,
    })
}

const FUNCS: [&str; 12] = ["min", "max", "abs", "floor", "ceil", "round", "sqrt", "pow", "clamp", "if", "contains", "lower"];

struct Parser<'a> {
    t: Vec<Tok>,
    p: usize,
    src: &'a str,
}

impl<'a> Parser<'a> {
    fn is_op(&self, op: &str) -> bool {
        matches!(self.t.get(self.p), Some(Tok::Op(o)) if *o == op)
    }
    fn eat(&mut self, op: &str) -> Result<(), String> {
        if self.is_op(op) { self.p += 1; Ok(()) } else { Err(format!("expected \"{op}\" in \"{}\"", self.src)) }
    }
    fn args(&mut self, close: &str) -> Result<Vec<Ast>, String> {
        let mut items = Vec::new();
        if !self.is_op(close) {
            loop {
                items.push(self.ternary()?);
                if self.is_op(",") { self.p += 1; } else { break; }
            }
        }
        self.eat(close)?;
        Ok(items)
    }
    fn primary(&mut self) -> Result<Ast, String> {
        let tk = self.t.get(self.p).cloned().ok_or_else(|| format!("unexpected end of \"{}\"", self.src))?;
        self.p += 1;
        match tk {
            Tok::Num(n) => Ok(Ast::Lit(Value::Num(n))),
            Tok::Str(s) => Ok(Ast::Lit(Value::Str(s))),
            Tok::Lit(v) => Ok(Ast::Lit(v)),
            Tok::Id(name) => {
                if self.is_op("(") {
                    self.p += 1;
                    let a = self.args(")")?;
                    return Ok(Ast::Call(name.to_ascii_lowercase(), a));
                }
                let mut path = vec![name];
                while self.is_op(".") {
                    self.p += 1;
                    match self.t.get(self.p).cloned() {
                        Some(Tok::Id(n)) => { path.push(n); self.p += 1; }
                        _ => return Err(format!("bad member access in \"{}\"", self.src)),
                    }
                }
                Ok(Ast::Var(path))
            }
            Tok::Op("(") => { let e = self.ternary()?; self.eat(")")?; Ok(e) }
            Tok::Op("[") => Ok(Ast::List(self.args("]")?)),
            Tok::Op("!") => Ok(Ast::Not(Box::new(self.primary()?))),
            Tok::Op("-") => Ok(Ast::Neg(Box::new(self.primary()?))),
            Tok::Op("+") => self.primary(),
            Tok::Op(o) => Err(format!("unexpected \"{o}\" in \"{}\"", self.src)),
        }
    }
    fn binary(&mut self, min: u8) -> Result<Ast, String> {
        let mut left = self.primary()?;
        loop {
            let op = match self.t.get(self.p) {
                Some(Tok::Op(o)) => *o,
                _ => return Ok(left),
            };
            let pr = match prec(op) { Some(p) if p >= min => p, _ => return Ok(left) };
            self.p += 1;
            let right = self.binary(pr + 1)?;
            left = Ast::Bin(op, Box::new(left), Box::new(right));
        }
    }
    fn ternary(&mut self) -> Result<Ast, String> {
        let c = self.binary(1)?;
        if self.is_op("?") {
            self.p += 1;
            let a = self.ternary()?;
            self.eat(":")?;
            let b = self.ternary()?;
            return Ok(Ast::Tern(Box::new(c), Box::new(a), Box::new(b)));
        }
        Ok(c)
    }
}

pub fn parse(src: &str) -> Result<Ast, String> {
    let mut p = Parser { t: tokenize(src)?, p: 0, src };
    let ast = p.ternary()?;
    if p.p < p.t.len() {
        return Err(format!("unexpected trailing input in \"{src}\""));
    }
    Ok(ast)
}

thread_local! {
    static CACHE: RefCell<HashMap<String, Result<Rc<Ast>, String>>> = RefCell::new(HashMap::new());
}

/// Parse with caching. Expressions are short and reused constantly.
pub fn compile(src: &str) -> Result<Rc<Ast>, String> {
    CACHE.with(|c| {
        if let Some(hit) = c.borrow().get(src) {
            return hit.clone();
        }
        let r = parse(src).map(Rc::new);
        c.borrow_mut().insert(src.to_string(), r.clone());
        r
    })
}

fn eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Str(x), Value::Str(y)) => x.eq_ignore_ascii_case(y),
        (Value::Null, Value::Null) => true,
        (Value::Null, _) | (_, Value::Null) => false,
        (Value::Str(_), _) | (_, Value::Str(_)) => a.num() == b.num() && (matches!(a, Value::Num(_)) || matches!(b, Value::Num(_))),
        _ => a.num() == b.num(),
    }
}

fn builtin(name: &str, a: &[Value]) -> Option<Value> {
    let n = |i: usize| a.get(i).map(|v| v.num()).unwrap_or(0.0);
    Some(match name {
        "min" => Value::Num(a.iter().map(|v| v.num()).fold(f64::INFINITY, f64::min)),
        "max" => Value::Num(a.iter().map(|v| v.num()).fold(f64::NEG_INFINITY, f64::max)),
        "abs" => Value::Num(n(0).abs()),
        "floor" => Value::Num(n(0).floor()),
        "ceil" => Value::Num(n(0).ceil()),
        "round" => Value::Num(n(0).round()),
        "sqrt" => Value::Num(n(0).max(0.0).sqrt()),
        "pow" => Value::Num(n(0).powf(n(1))),
        "clamp" => Value::Num(n(0).max(n(1)).min(n(2))),
        "if" => if a.first().map(|v| v.truthy()).unwrap_or(false) { a.get(1).cloned().unwrap_or(Value::Null) } else { a.get(2).cloned().unwrap_or(Value::Null) },
        "contains" => Value::Bool(a.first().map(|v| v.as_str().to_ascii_lowercase()).unwrap_or_default().contains(&a.get(1).map(|v| v.as_str().to_ascii_lowercase()).unwrap_or_default())),
        "lower" => Value::Str(a.first().map(|v| v.as_str().to_ascii_lowercase()).unwrap_or_default()),
        _ => return None,
    })
}

pub fn eval_ast(ast: &Ast, sc: &dyn Scope) -> Value {
    match ast {
        Ast::Lit(v) => v.clone(),
        Ast::Var(path) => sc.get(path),
        Ast::List(items) => Value::List(items.iter().map(|x| eval_ast(x, sc)).collect()),
        Ast::Not(e) => Value::Bool(!eval_ast(e, sc).truthy()),
        Ast::Neg(e) => Value::Num(-eval_ast(e, sc).num()),
        Ast::Tern(c, a, b) => if eval_ast(c, sc).truthy() { eval_ast(a, sc) } else { eval_ast(b, sc) },
        Ast::Call(name, args) => {
            let vals: Vec<Value> = args.iter().map(|x| eval_ast(x, sc)).collect();
            sc.call(name, &vals).or_else(|| builtin(name, &vals)).unwrap_or(Value::Null)
        }
        Ast::Bin(op, l, r) => {
            if *op == "&&" {
                let a = eval_ast(l, sc);
                return if a.truthy() { eval_ast(r, sc) } else { a };
            }
            if *op == "||" {
                let a = eval_ast(l, sc);
                return if a.truthy() { a } else { eval_ast(r, sc) };
            }
            let a = eval_ast(l, sc);
            let b = eval_ast(r, sc);
            match *op {
                "==" => Value::Bool(eq(&a, &b)),
                "!=" => Value::Bool(!eq(&a, &b)),
                "in" => Value::Bool(matches!(&b, Value::List(items) if items.iter().any(|x| eq(&a, x)))),
                // Something unknown is neither more nor less than anything.
                "<" | "<=" | ">" | ">=" if a == Value::Null || b == Value::Null => Value::Bool(false),
                "<" => Value::Bool(a.num() < b.num()),
                "<=" => Value::Bool(a.num() <= b.num()),
                ">" => Value::Bool(a.num() > b.num()),
                ">=" => Value::Bool(a.num() >= b.num()),
                "+" => match (&a, &b) {
                    (Value::Str(x), _) => Value::Str(format!("{x}{}", b.as_str())),
                    _ => Value::Num(a.num() + b.num()),
                },
                "-" => Value::Num(a.num() - b.num()),
                "*" => Value::Num(a.num() * b.num()),
                "/" => { let d = b.num(); Value::Num(if d == 0.0 { 0.0 } else { a.num() / d }) }
                "%" => { let d = b.num(); Value::Num(if d == 0.0 { 0.0 } else { a.num() % d }) }
                _ => Value::Null,
            }
        }
    }
}

/// Evaluate `src` against `sc`. A broken expression evaluates to `Null`
/// rather than stopping the simulation; validation catches these up front.
pub fn eval(src: &str, sc: &dyn Scope) -> Value {
    match compile(src) {
        Ok(ast) => eval_ast(&ast, sc),
        Err(_) => Value::Null,
    }
}
pub fn eval_num(src: &str, sc: &dyn Scope, default: f32) -> f32 {
    if src.trim().is_empty() { return default; }
    match eval(src, sc) {
        Value::Num(n) if n.is_finite() => n as f32,
        Value::Bool(true) => 1.0,
        Value::Bool(false) => 0.0,
        _ => default,
    }
}
pub fn eval_bool(src: &str, sc: &dyn Scope, default: bool) -> bool {
    if src.trim().is_empty() { return default; }
    eval(src, sc).truthy()
}

fn collect(ast: &Ast, vars: &mut Vec<String>, calls: &mut Vec<String>) {
    match ast {
        Ast::Var(p) => if !vars.contains(&p[0]) { vars.push(p[0].clone()) },
        Ast::Call(n, a) => { if !calls.contains(n) { calls.push(n.clone()); } a.iter().for_each(|x| collect(x, vars, calls)) }
        Ast::List(a) => a.iter().for_each(|x| collect(x, vars, calls)),
        Ast::Not(e) | Ast::Neg(e) => collect(e, vars, calls),
        Ast::Tern(c, a, b) => { collect(c, vars, calls); collect(a, vars, calls); collect(b, vars, calls) }
        Ast::Bin(_, l, r) => { collect(l, vars, calls); collect(r, vars, calls) }
        Ast::Lit(_) => {}
    }
}

/// Check an expression against the variables and functions a context offers.
/// Returns a message suitable for handing back to whoever wrote the law.
pub fn check(src: &str, vars: &[&str], funcs: &[&str]) -> Result<(), String> {
    if src.trim().is_empty() { return Ok(()); }
    let ast = compile(src)?;
    let mut v = Vec::new();
    let mut c = Vec::new();
    collect(&ast, &mut v, &mut c);
    let unknown: Vec<&String> = v.iter().filter(|x| !vars.contains(&x.as_str())).collect();
    if !unknown.is_empty() {
        return Err(format!("unknown variable(s) {} in \"{src}\"", unknown.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")));
    }
    let bad: Vec<&String> = c.iter().filter(|x| !FUNCS.contains(&x.as_str()) && !funcs.contains(&x.as_str())).collect();
    if !bad.is_empty() {
        return Err(format!("unknown function(s) {} in \"{src}\"", bad.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct S;
    impl Scope for S {
        fn get(&self, p: &[String]) -> Value {
            match p.iter().map(|s| s.as_str()).collect::<Vec<_>>().as_slice() {
                ["age"] => 40.into(),
                ["job"] => "Farmer".into(),
                ["income"] => 12.5f32.into(),
                ["worker", "age"] => 12.into(),
                _ => Value::Null,
            }
        }
        fn call(&self, name: &str, args: &[Value]) -> Option<Value> {
            (name == "has").then(|| Value::Bool(args[0].as_str() == "noble"))
        }
    }
    #[test]
    fn arithmetic_and_logic() {
        assert_eq!(eval_num("1 + 2 * 3", &S, 0.0), 7.0);
        assert_eq!(eval_num("income > 10 ? 0.4 : 0.1", &S, 0.0), 0.4);
        assert!(eval_bool("age >= 18 && job == \"farmer\"", &S, false));
        assert!(eval_bool("job in [\"brewer\", \"farmer\"]", &S, false));
        assert!(eval_bool("worker.age < 14", &S, false));
        assert!(eval_bool("has(\"noble\") and not has(\"serf\")", &S, false));
        assert_eq!(eval_num("15%", &S, 0.0), 0.15);
        assert_eq!(eval_num("income % 5", &S, 0.0), 2.5);
        assert_eq!(eval_num("clamp(age, 0, 30) + max(1, 2)", &S, 0.0), 32.0);
        assert!(!eval_bool("unknown_thing > 3", &S, true));
        assert!(!eval_bool("unknown_thing < 3", &S, true));
    }
    #[test]
    fn checking() {
        assert!(check("age > 3", &["age"], &[]).is_ok());
        assert!(check("agee > 3", &["age"], &[]).is_err());
        assert!(check("has(\"x\")", &[], &["has"]).is_ok());
        assert!(check("nope(1)", &[], &[]).is_err());
        assert!(check("age >", &["age"], &[]).is_err());
    }
}
