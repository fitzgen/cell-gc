use cell_gc::{GcHeapSession, GcLeaf};
use cell_gc::collections::VecRef;
use compile;
use std::borrow::Borrow;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::collections::HashSet;
use vm::{EnvironmentRef, Trampoline};

#[derive(Debug, IntoHeap)]
pub struct Pair<'h> {
    pub car: Value<'h>,
    pub cdr: Value<'h>,
}

#[derive(Clone, Debug, PartialEq, IntoHeap)]
pub enum Value<'h> {
    Nil,
    Bool(bool),
    Int(i32),
    Symbol(GcLeaf<InternedString>),
    Lambda(PairRef<'h>),
    Code(compile::CodeRef<'h>),
    Builtin(GcLeaf<BuiltinFnPtr>),
    Cons(PairRef<'h>),
    Vector(VecRef<'h, Value<'h>>),
    Environment(EnvironmentRef<'h>),
}

pub use self::Value::*;

pub struct BuiltinFnPtr(
    pub for<'b> fn(&mut GcHeapSession<'b>, Vec<Value<'b>>)
        -> Result<Trampoline<'b>, String>,
);

// This can't be #[derive]d because function pointers aren't Clone.
// But they are Copy. A very weird thing about Rust.
impl Clone for BuiltinFnPtr {
    fn clone(&self) -> BuiltinFnPtr {
        BuiltinFnPtr(self.0)
    }
}

impl PartialEq for BuiltinFnPtr {
    fn eq(&self, other: &BuiltinFnPtr) -> bool {
        self.0 as usize == other.0 as usize
    }
}

impl fmt::Debug for BuiltinFnPtr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "BuiltinFn({:p})", self.0 as usize as *mut ())?;
        Ok(())
    }
}

impl<'h> fmt::Display for Value<'h> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Note that this will need to add a set of already-printed pairs if we add
        // `set-car!` and/or `set-cdr!` and introduce the possibility of cycles.
        match *self {
            Nil => write!(f, "nil"),
            Bool(true) => write!(f, "#t"),
            Bool(false) => write!(f, "#f"),
            Int(n) => write!(f, "{}", n),
            Symbol(ref s) => write!(f, "{}", s.as_str()),
            Lambda(_) => write!(f, "#lambda"),
            Code(_) => write!(f, "#code"),
            Builtin(_) => write!(f, "#builtin"),
            Cons(ref p) => {
                write!(f, "(")?;
                write_pair(f, p.clone())?;
                write!(f, ")")
            }
            Vector(ref v) => {
                write!(f, "#(")?;
                for i in 0..v.len() {
                    if i != 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{}", v.get(i))?;
                }
                write!(f, ")")
            }
            Environment(_) => write!(f, "#environment"),
        }
    }
}

fn write_pair<'h>(f: &mut fmt::Formatter, pair: PairRef<'h>) -> fmt::Result {
    write!(f, "{}", pair.car())?;
    match pair.cdr() {
        Nil => Ok(()),
        Cons(p) => {
            write!(f, " ")?;
            write_pair(f, p)
        }
        otherwise => {
            write!(f, " . ")?;
            write!(f, "{}", otherwise)
        }
    }
}

impl<'h> Value<'h> {
    pub fn is_nil(&self) -> bool {
        match *self {
            Nil => true,
            _ => false,
        }
    }

    pub fn is_boolean(&self) -> bool {
        match *self {
            Bool(_) => true,
            _ => false,
        }
    }

    /// True unless this value is `#f`. Conditional expressions (`if`, `cond`,
    /// etc.) should use this to check whether a value is a "true value".
    pub fn to_bool(&self) -> bool {
        match *self {
            Bool(false) => false,
            _ => true,
        }
    }

    pub fn as_int(self, error_msg: &str) -> Result<i32, String> {
        match self {
            Int(i) => Ok(i),
            _ => Err(format!("{}: number required", error_msg)),
        }
    }

    pub fn as_index(self, error_msg: &str) -> Result<usize, String> {
        match self {
            Int(i) => {
                if i >= 0 {
                    Ok(i as usize)
                } else {
                    Err(format!("{}: negative vector index", error_msg))
                }
            }
            _ => Err(format!("{}: vector index required", error_msg)),
        }
    }

    pub fn is_pair(&self) -> bool {
        match *self {
            Cons(_) => true,
            _ => false,
        }
    }

    pub fn as_pair(self, msg: &'static str) -> Result<(Value<'h>, Value<'h>), String> {
        match self {
            Cons(r) => Ok((r.car(), r.cdr())),
            _ => Err(msg.to_string()),
        }
    }

    pub fn is_vector(&self) -> bool {
        match *self {
            Vector(_) => true,
            _ => false,
        }
    }

    pub fn as_vector(self, error_msg: &str) -> Result<VecRef<'h, Value<'h>>, String> {
        match self {
            Vector(v) => Ok(v),
            _ => Err(format!("{}: vector expected", error_msg)),
        }
    }

    pub fn as_symbol(self, error_msg: &str) -> Result<InternedString, String> {
        match self {
            Symbol(s) => Ok(s.unwrap()),
            _ => Err(error_msg.to_string()),
        }
    }

    pub fn is_procedure(&self) -> bool {
        match *self {
            Lambda(_) => true,
            Builtin(_) => true,
            _ => false,
        }
    }
}

impl<'h> Iterator for Value<'h> {
    type Item = Result<Value<'h>, String>;

    fn next(&mut self) -> Option<Self::Item> {
        let (car, cdr) = match *self {
            Nil => return None,
            Cons(ref pair) => (pair.car(), pair.cdr()),
            _ => return Some(Err("improper list".into())),
        };
        *self = cdr;
        Some(Ok(car))
    }
}


#[derive(Clone, Debug)]
pub struct InternedString(Arc<String>);

// Note: If we ever impl Hash for InternedString, it will be better to use a
// custom pointer-based implementation than to use derive(Hash), which would
// hash the contents of the string.
impl PartialEq for InternedString {
    fn eq(&self, other: &InternedString) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for InternedString {}

lazy_static! {
    static ref STRINGS: Mutex<HashSet<InternedStringByValue>> = Mutex::new(HashSet::new());
}

#[derive(Eq, Hash, PartialEq)]
struct InternedStringByValue(Arc<String>);

impl Borrow<str> for InternedStringByValue {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl InternedString {
    pub fn get(s: &str) -> InternedString {
        let mut guard = STRINGS.lock().unwrap();
        if let Some(x) = guard.get(s) {
            return InternedString(x.0.clone());
        }
        let s = Arc::new(s.to_string());
        guard.insert(InternedStringByValue(s.clone()));
        InternedString(s)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}