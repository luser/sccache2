use std::cmp::Ordering;
use std::error::Error;
use std::ffi::OsString;
use std::fmt::{self, Debug, Display};
use std::marker::PhantomData;
use std::path::PathBuf;
use std::result::Result as StdResult;

type ArgResult<T> = StdResult<T, ArgError>;

#[derive(Debug, PartialEq)]
pub enum ArgError {
    UnexpectedEndOfArgs,
}

impl ArgError {
    pub fn static_description(&self) -> &'static str {
        match self {
            ArgError::UnexpectedEndOfArgs => "Unexpected end of args",
        }
    }
}

impl Display for ArgError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = self.static_description();
        write!(f, "{}", s)
    }
}

impl Error for ArgError {
    fn cause(&self) -> Option<&Error> { None }
}

pub type Delimiter = Option<u8>;

/// Representation of a parsed argument
#[derive(PartialEq, Clone, Debug)]
pub enum Argument<T> {
    /// Unknown non-flag argument ; e.g. "foo"
    Raw(OsString),
    /// Unknown flag argument ; e.g. "-foo"
    UnknownFlag(OsString),
    /// Known flag argument ; e.g. "-bar"
    Flag(&'static str, T),
    /// Known argument with a value ; e.g. "-qux bar", where the way the
    /// value is passed is described by the ArgDisposition type.
    WithValue(&'static str, T, ArgDisposition),
}

/// How a value is passed to an argument with a value.
#[derive(PartialEq, Clone, Debug)]
pub enum ArgDisposition {
    /// As "-arg value"
    Separated,
    /// As "-arg value", but "-arg<delimiter>value" would be valid too
    CanBeConcatenated(Delimiter),
    /// As "-arg<delimiter>value", but "-arg value" would be valid too
    CanBeSeparated(Delimiter),
    /// As "-arg<delimiter>value"
    Concatenated(Delimiter),
}

pub enum NormalizedDisposition {
    Separated,
    Concatenated,
}

impl<T: ArgumentValue> Argument<T> {
    /// For arguments that allow both a concatenated or separated disposition,
    /// normalize a parsed argument to a prefered disposition.
    pub fn normalize(self, disposition: NormalizedDisposition) -> Self {
        match self {
            Argument::WithValue(s, v, ArgDisposition::CanBeConcatenated(d)) |
            Argument::WithValue(s, v, ArgDisposition::CanBeSeparated(d)) => {
                Argument::WithValue(
                    s,
                    v,
                    match disposition {
                        NormalizedDisposition::Separated => ArgDisposition::Separated,
                        NormalizedDisposition::Concatenated => ArgDisposition::Concatenated(d),
                    },
                )
            }
            a => a,
        }
    }

    pub fn to_os_string(&self) -> OsString {
        match *self {
            Argument::Raw(ref s) |
            Argument::UnknownFlag(ref s) => s.clone(),
            Argument::Flag(ref s, _) |
            Argument::WithValue(ref s, _, _) => s.into(),
        }
    }

    pub fn to_str(&self) -> Option<&'static str> {
        match *self {
            Argument::Flag(s, _) |
            Argument::WithValue(s, _, _) => Some(s),
            _ => None,
        }
    }

    pub fn get_data(&self) -> Option<&T> {
        match *self {
            Argument::Flag(_, ref d) => Some(d),
            Argument::WithValue(_, ref d, _) => Some(d),
            _ => None,
        }
    }
}

pub struct IntoIter<T: ArgumentValue> {
    arg: Argument<T>,
    emitted: usize,
}

/// Transforms a parsed argument into an iterator.
impl<T: ArgumentValue> IntoIterator for Argument<T> {
    type Item = OsString;
    type IntoIter = IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            arg: self,
            emitted: 0,
        }
    }
}

impl<T: ArgumentValue> Iterator for IntoIter<T> {
    type Item = OsString;

    fn next(&mut self) -> Option<Self::Item> {
        let result = match self.arg {
            Argument::Raw(ref s) |
            Argument::UnknownFlag(ref s) => {
                match self.emitted {
                    0 => Some(s.clone()),
                    _ => None,
                }
            }
            Argument::Flag(s, _) => {
                match self.emitted {
                    0 => Some(s.into()),
                    _ => None,
                }
            }
            Argument::WithValue(s, ref v, ref d) => {
                match (self.emitted, d) {
                    (0, &ArgDisposition::CanBeSeparated(d)) |
                    (0, &ArgDisposition::Concatenated(d)) => {
                        let mut s = OsString::from(s);
                        if let Some(d) = d {
                            s.push(OsString::from(String::from_utf8(vec![d]).expect(
                                "delimiter should be ascii",
                            )));
                        }
                        s.push(v.clone().into_arg());
                        Some(s)
                    }
                    (0, &ArgDisposition::Separated) |
                    (0, &ArgDisposition::CanBeConcatenated(_)) => Some(s.into()),
                    (1, &ArgDisposition::Separated) |
                    (1, &ArgDisposition::CanBeConcatenated(_)) => Some(v.clone().into_arg()),
                    _ => None,
                }
            }
        };
        if let Some(_) = result {
            self.emitted += 1;
        }
        result
    }
}

macro_rules! ArgData {
    { __impl $( $x:ident($y:ty), )+ } => {
        impl IntoArg for ArgData {
            fn into_arg(self) -> OsString {
                match self {
                    $(
                        ArgData::$x(inner) => inner.into_arg(),
                    )*
                }
            }
        }
    };
    // PartialEq necessary for tests
    { pub $( $x:ident($y:ty), )+ } => {
        #[derive(Clone, Debug, PartialEq)]
        pub enum ArgData {
            $(
                $x($y),
            )*
        }
        ArgData!{ __impl $( $x($y), )+ }
    };
    { $( $x:ident($y:ty), )+ } => {
        #[derive(Clone, Debug, PartialEq)]
        enum ArgData {
            $(
                $x($y),
            )*
        }
        ArgData!{ __impl $( $x($y), )+ }
    };
}

// The value associated with a parsed argument
pub trait ArgumentValue: IntoArg + Clone + Debug {}

impl<T: IntoArg + Clone + Debug> ArgumentValue for T {}

pub trait FromArg: Sized {
    fn process(arg: OsString) -> ArgResult<Self>;
}

pub trait IntoArg {
    fn into_arg(self) -> OsString;
}

impl FromArg for OsString { fn process(arg: OsString) -> ArgResult<Self> { Ok(arg) } }
impl FromArg for PathBuf { fn process(arg: OsString) -> ArgResult<Self> { Ok(arg.into()) } }

impl IntoArg for () { fn into_arg(self) -> OsString { OsString::new() } }
impl IntoArg for OsString { fn into_arg(self) -> OsString { self } }
impl IntoArg for PathBuf { fn into_arg(self) -> OsString { self.into() } }

/// The description of how an argument may be parsed
#[derive(PartialEq, Clone, Debug)]
pub enum ArgInfo<T> {
    /// An simple flag argument, of the form "-foo"
    Flag(&'static str, T),
    /// An argument with a value ; e.g. "-qux bar", where the way the
    /// value is passed is described by the ArgDisposition type.
    TakeArg(&'static str, fn(OsString) -> ArgResult<T>, ArgDisposition),
}

impl<T: ArgumentValue> ArgInfo<T> {
    /// Transform an argument description into a parsed Argument, given a
    /// string. For arguments with a value, where the value is separate, the
    /// `get_next_arg` function returns the next argument, in raw `OsString`
    /// form.
    fn process<F>(self, arg: &str, get_next_arg: F) -> ArgResult<Argument<T>>
    where
        F: FnOnce() -> Option<OsString>,
    {
        Ok(match self {
            ArgInfo::Flag(s, variant) => {
                debug_assert_eq!(s, arg);
                Argument::Flag(s, variant)
            }
            ArgInfo::TakeArg(s, create, ArgDisposition::Separated) => {
                debug_assert_eq!(s, arg);
                if let Some(a) = get_next_arg() {
                    Argument::WithValue(s, create(a)?, ArgDisposition::Separated)
                } else {
                    return Err(ArgError::UnexpectedEndOfArgs)
                }
            }
            ArgInfo::TakeArg(s, create, ArgDisposition::Concatenated(d)) => {
                let mut len = s.len();
                debug_assert_eq!(&arg[..len], s);
                if let Some(d) = d {
                    debug_assert_eq!(arg.as_bytes()[len], d);
                    len += 1;
                }
                Argument::WithValue(
                    s,
                    create(arg[len..].into())?,
                    ArgDisposition::Concatenated(d),
                )
            }
            ArgInfo::TakeArg(s, create, ArgDisposition::CanBeSeparated(d)) |
            ArgInfo::TakeArg(s, create, ArgDisposition::CanBeConcatenated(d)) => {
                let derived = if arg == s {
                    ArgInfo::TakeArg(s, create, ArgDisposition::Separated)
                } else {
                    ArgInfo::TakeArg(s, create, ArgDisposition::Concatenated(d))
                };
                match derived.process(arg, get_next_arg) {
                    Err(ArgError::UnexpectedEndOfArgs) if d.is_none() => {
                        Argument::WithValue(
                            s,
                            create("".into())?,
                            ArgDisposition::Concatenated(d),
                        )
                    }
                    Ok(Argument::WithValue(s, v, ArgDisposition::Concatenated(d))) => {
                        Argument::WithValue(s, v, ArgDisposition::CanBeSeparated(d))
                    }
                    Ok(Argument::WithValue(s, v, ArgDisposition::Separated)) => {
                        Argument::WithValue(s, v, ArgDisposition::CanBeConcatenated(d))
                    }
                    a => a?,
                }
            }
        })
    }

    /// Returns whether the given string matches the argument description, and if not,
    /// how it differs.
    fn cmp(&self, arg: &str) -> Ordering {
        match self {
            &ArgInfo::TakeArg(s, _, ArgDisposition::CanBeSeparated(None)) |
            &ArgInfo::TakeArg(s, _, ArgDisposition::Concatenated(None)) if arg.starts_with(s) => {
                Ordering::Equal
            }
            &ArgInfo::TakeArg(s, _, ArgDisposition::CanBeSeparated(Some(d))) |
            &ArgInfo::TakeArg(s, _, ArgDisposition::Concatenated(Some(d)))
                if arg.len() > s.len() && arg.starts_with(s) => arg.as_bytes()[s.len()].cmp(&d),
            _ => self.flag_str().cmp(arg),
        }
    }

    fn flag_str(&self) -> &'static str {
        match self {
            &ArgInfo::Flag(s, _) |
            &ArgInfo::TakeArg(s, _, _) => s,
        }
    }
}

/// Binary search for a `key` in a sorted array of items, given a comparison
/// function. This implementation is tweaked to handle the case where the
/// comparison function does prefix matching, where multiple items in the array
/// might match, but the last match is the one actually matching.
fn bsearch<'a, K, T, F>(key: K, items: &'a [T], cmp: F) -> Option<&'a T>
where
    F: Fn(&T, &K) -> Ordering,
{
    let mut slice = items;
    while !slice.is_empty() {
        let middle = slice.len() / 2;
        match cmp(&slice[middle], &key) {
            Ordering::Equal => {
                let found_after = if slice.len() == 1 {
                    None
                } else {
                    bsearch(key, &slice[middle + 1..], cmp)
                };
                return found_after.or(Some(&slice[middle]));
            }
            Ordering::Greater => {
                slice = &slice[..middle];
            }
            Ordering::Less => {
                slice = &slice[middle + 1..];
            }
        }
    }
    None
}

/// Trait for generically search over a "set" of ArgInfos.
pub trait SearchableArgInfo<T> {
    fn search(&self, key: &str) -> Option<&ArgInfo<T>>;

    #[cfg(debug_assertions)]
    fn check(&self) -> bool;
}

/// Allow to search over a sorted array of ArgInfo items associated with extra
/// data.
impl<T: ArgumentValue> SearchableArgInfo<T> for &'static [ArgInfo<T>] {
    fn search(&self, key: &str) -> Option<&ArgInfo<T>> {
        bsearch(key, self, |i, k| i.cmp(k))
    }

    #[cfg(debug_assertions)]
    fn check(&self) -> bool {
        self.windows(2).all(|w| {
            let a = w[0].flag_str();
            let b = w[1].flag_str();
            assert!(a < b, "{} can't precede {}", a, b);
            true
        })
    }
}

/// Allow to search over a couple of arrays of ArgInfo, where the second
/// complements or overrides the first one.
impl<T: ArgumentValue> SearchableArgInfo<T> for (&'static [ArgInfo<T>], &'static [ArgInfo<T>]) {
    fn search(&self, key: &str) -> Option<&ArgInfo<T>> {
        match (self.0.search(key), self.1.search(key)) {
            (None, None) => None,
            (Some(a), None) => Some(a),
            (None, Some(a)) => Some(a),
            (Some(a), Some(b)) => {
                if a.flag_str() > b.flag_str() {
                    Some(a)
                } else {
                    Some(b)
                }
            }
        }
    }

    #[cfg(debug_assertions)]
    fn check(&self) -> bool {
        self.0.check() && self.1.check()
    }
}

/// An `Iterator` for parsed arguments
pub struct ArgsIter<I, T, S>
where
    I: Iterator<Item = OsString>,
    S: SearchableArgInfo<T>,
{
    arguments: I,
    arg_info: S,
    phantom: PhantomData<T>,
}

impl<I, T, S> ArgsIter<I, T, S>
where
    I: Iterator<Item = OsString>,
    T: ArgumentValue,
    S: SearchableArgInfo<T>,
{
    /// Create an `Iterator` for parsed arguments, given an iterator of raw
    /// `OsString` arguments, and argument descriptions.
    pub fn new(arguments: I, arg_info: S) -> Self {
        #[cfg(debug_assertions)]
        debug_assert!(arg_info.check());
        ArgsIter {
            arguments: arguments,
            arg_info: arg_info,
            phantom: PhantomData,
        }
    }
}

impl<I, T, S> Iterator for ArgsIter<I, T, S>
where
    I: Iterator<Item = OsString>,
    T: ArgumentValue,
    S: SearchableArgInfo<T>,
{
    type Item = ArgResult<Argument<T>>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(arg) = self.arguments.next() {
            let s = arg.to_string_lossy();
            let arguments = &mut self.arguments;
            Some(match self.arg_info.search(&s[..]) {
                Some(i) => {
                    i.clone().process(&s[..], || arguments.next())
                }
                None => {
                    Ok(if s.starts_with("-") {
                        Argument::UnknownFlag(arg.clone())
                    } else {
                        Argument::Raw(arg.clone())
                    })
                }
            })
        } else {
            None
        }
    }
}

/// Helper macro used to define ArgInfo::Flag's.
/// Variant is an enum variant, e.g. enum ArgType { Variant(()) }
///     flag!("-foo")
///     flag!("-foo", Variant)
macro_rules! flag {
    ($s:expr, $variant:expr) => { ArgInfo::Flag($s, $variant(())) };
}

/// Helper macro used to define ArgInfo::TakeArg's.
/// Variant is an enum variant, e.g. enum ArgType { Variant(OsString) }
///     take_arg!("-foo", OsString, Separated, Variant)
///     take_arg!("-foo", OsString, Concatenated, Variant)
///     take_arg!("-foo", OsString, Concatenated('='), Variant)
macro_rules! take_arg {
    ($s:expr, $vtype:ident, Separated, $variant:expr) => {
        ArgInfo::TakeArg($s, |arg: OsString| $vtype::process(arg).map($variant), ArgDisposition::Separated)
    };
    ($s:expr, $vtype:ident, $d:ident, $variant:expr) => {
        ArgInfo::TakeArg($s, |arg: OsString| $vtype::process(arg).map($variant), ArgDisposition::$d(None))
    };
    ($s:expr, $vtype:ident, $d:ident($x:expr), $variant:expr) => {
        ArgInfo::TakeArg($s, |arg: OsString| $vtype::process(arg).map($variant), ArgDisposition::$d(Some($x as u8)))
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::iter::FromIterator;
    use itertools::{diff_with, Diff};

    macro_rules! arg {
        ($name:ident($x:expr)) => {
            Argument::$name($x.into())
        };

        ($name:ident($x:expr, $v:ident($y:expr))) => {
            Argument::$name($x.into(), $v($y.into()))
        };
        ($name:ident($x:expr, $v:ident($y:expr), Separated)) => {
            Argument::$name($x, $v($y.into()), ArgDisposition::Separated)
        };
        ($name:ident($x:expr, $v:ident($y:expr), $d:ident)) => {
            Argument::$name($x, $v($y.into()), ArgDisposition::$d(None))
        };
        ($name:ident($x:expr, $v:ident($y:expr), $d:ident($z:expr))) => {
            Argument::$name($x, $v($y.into()), ArgDisposition::$d(Some($z as u8)))
        };

        ($name:ident($x:expr, $v:ident::$w:ident($y:expr))) => {
            Argument::$name($x.into(), $v::$w($y.into()))
        };
        ($name:ident($x:expr, $v:ident::$w:ident($y:expr), Separated)) => {
            Argument::$name($x, $v::$w($y.into()), ArgDisposition::Separated)
        };
        ($name:ident($x:expr, $v:ident::$w:ident($y:expr), $d:ident)) => {
            Argument::$name($x, $v::$w($y.into()), ArgDisposition::$d(None))
        };
        ($name:ident($x:expr, $v:ident::$w:ident($y:expr), $d:ident($z:expr))) => {
            Argument::$name($x, $v::$w($y.into()), ArgDisposition::$d(Some($z as u8)))
        };
    }

    ArgData!{
        FooFlag(()),
        Foo(OsString),
        FooPath(PathBuf),
    }

    use self::ArgData::*;

    #[test]
    fn test_arginfo_cmp() {
        let info = flag!("-foo", FooFlag);
        assert_eq!(info.cmp("-foo"), Ordering::Equal);
        assert_eq!(info.cmp("bar"), Ordering::Less);
        assert_eq!(info.cmp("-bar"), Ordering::Greater);
        assert_eq!(info.cmp("-qux"), Ordering::Less);
        assert_eq!(info.cmp("-foobar"), Ordering::Less);
        assert_eq!(info.cmp("-foo="), Ordering::Less);
        assert_eq!(info.cmp("-foo=bar"), Ordering::Less);

        let info = take_arg!("-foo", OsString, Separated, Foo);
        assert_eq!(info.cmp("-foo"), Ordering::Equal);
        assert_eq!(info.cmp("bar"), Ordering::Less);
        assert_eq!(info.cmp("-bar"), Ordering::Greater);
        assert_eq!(info.cmp("-qux"), Ordering::Less);
        assert_eq!(info.cmp("-foobar"), Ordering::Less);
        assert_eq!(info.cmp("-foo="), Ordering::Less);
        assert_eq!(info.cmp("-foo=bar"), Ordering::Less);

        let info = take_arg!("-foo", OsString, Concatenated, Foo);
        assert_eq!(info.cmp("-foo"), Ordering::Equal);
        assert_eq!(info.cmp("bar"), Ordering::Less);
        assert_eq!(info.cmp("-bar"), Ordering::Greater);
        assert_eq!(info.cmp("-qux"), Ordering::Less);
        assert_eq!(info.cmp("-foobar"), Ordering::Equal);
        assert_eq!(info.cmp("-foo="), Ordering::Equal);
        assert_eq!(info.cmp("-foo=bar"), Ordering::Equal);

        let info = take_arg!("-foo", OsString, Concatenated('='), Foo);
        assert_eq!(info.cmp("-foo"), Ordering::Equal);
        assert_eq!(info.cmp("bar"), Ordering::Less);
        assert_eq!(info.cmp("-bar"), Ordering::Greater);
        assert_eq!(info.cmp("-qux"), Ordering::Less);
        assert_eq!(info.cmp("-foobar"), Ordering::Greater);
        assert_eq!(info.cmp("-foo="), Ordering::Equal);
        assert_eq!(info.cmp("-foo=bar"), Ordering::Equal);

        let info = take_arg!("-foo", OsString, CanBeSeparated, Foo);
        assert_eq!(info.cmp("-foo"), Ordering::Equal);
        assert_eq!(info.cmp("bar"), Ordering::Less);
        assert_eq!(info.cmp("-bar"), Ordering::Greater);
        assert_eq!(info.cmp("-qux"), Ordering::Less);
        assert_eq!(info.cmp("-foobar"), Ordering::Equal);
        assert_eq!(info.cmp("-foo="), Ordering::Equal);
        assert_eq!(info.cmp("-foo=bar"), Ordering::Equal);

        let info = take_arg!("-foo", OsString, CanBeSeparated('='), Foo);
        assert_eq!(info.cmp("-foo"), Ordering::Equal);
        assert_eq!(info.cmp("bar"), Ordering::Less);
        assert_eq!(info.cmp("-bar"), Ordering::Greater);
        assert_eq!(info.cmp("-qux"), Ordering::Less);
        assert_eq!(info.cmp("-foobar"), Ordering::Greater);
        assert_eq!(info.cmp("-foo="), Ordering::Equal);
        assert_eq!(info.cmp("-foo=bar"), Ordering::Equal);
    }

    #[test]
    fn test_arginfo_process() {
        let info = flag!("-foo", FooFlag);
        assert_eq!(info.process("-foo", || None).unwrap(), arg!(Flag("-foo", FooFlag(()))));

        let info = take_arg!("-foo", OsString, Separated, Foo);
        assert_eq!(info.clone().process("-foo", || None).unwrap_err(), ArgError::UnexpectedEndOfArgs);
        assert_eq!(
            info.clone().process("-foo", || Some("bar".into())).unwrap(),
            arg!(WithValue("-foo", Foo("bar"), Separated))
        );

        let info = take_arg!("-foo", OsString, Concatenated, Foo);
        assert_eq!(
            info.clone().process("-foo", || None).unwrap(),
            arg!(WithValue("-foo", Foo(""), Concatenated))
        );
        assert_eq!(
            info.clone().process("-foobar", || None).unwrap(),
            arg!(WithValue("-foo", Foo("bar"), Concatenated))
        );

        let info = take_arg!("-foo", OsString, Concatenated('='), Foo);
        assert_eq!(
            info.clone().process("-foo=", || None).unwrap(),
            arg!(WithValue("-foo", Foo(""), Concatenated('=')))
        );
        assert_eq!(
            info.clone().process("-foo=bar", || None).unwrap(),
            arg!(WithValue("-foo", Foo("bar"), Concatenated('=')))
        );

        let info = take_arg!("-foo", OsString, CanBeSeparated, Foo);
        assert_eq!(
            info.clone().process("-foo", || None).unwrap(),
            arg!(WithValue("-foo", Foo(""), Concatenated))
        );
        assert_eq!(
            info.clone().process("-foobar", || None).unwrap(),
            arg!(WithValue("-foo", Foo("bar"), CanBeSeparated))
        );
        assert_eq!(
            info.clone().process("-foo", || Some("bar".into())).unwrap(),
            arg!(WithValue("-foo", Foo("bar"), CanBeConcatenated))
        );

        let info = take_arg!("-foo", OsString, CanBeSeparated('='), Foo);
        assert_eq!(info.clone().process("-foo", || None).unwrap_err(), ArgError::UnexpectedEndOfArgs);
        assert_eq!(
            info.clone().process("-foo=", || None).unwrap(),
            arg!(WithValue("-foo", Foo(""), CanBeSeparated('=')))
        );
        assert_eq!(
            info.clone().process("-foo=bar", || None).unwrap(),
            arg!(WithValue("-foo", Foo("bar"), CanBeSeparated('=')))
        );
        assert_eq!(
            info.clone().process("-foo", || Some("bar".into())).unwrap(),
            arg!(WithValue("-foo", Foo("bar"), CanBeConcatenated('=')))
        );
    }

    #[test]
    fn test_bsearch() {
        let data = vec![
            ("bar", 1),
            ("foo", 2),
            ("fuga", 3),
            ("hoge", 4),
            ("plop", 5),
            ("qux", 6),
            ("zorglub", 7),
        ];
        for item in &data {
            assert_eq!(bsearch(item.0, &data, |i, k| i.0.cmp(k)), Some(item));
        }

        // Try again with an even number of items
        let data = &data[..6];
        for item in data {
            assert_eq!(bsearch(item.0, &data, |i, k| i.0.cmp(k)), Some(item));
        }

        // Once more, with prefix matches
        let data = vec![
            ("a", 1),
            ("ab", 2),
            ("abc", 3),
            ("abd", 4),
            ("abe", 5),
            ("abef", 6),
            ("abefg", 7),
        ];
        for item in &data {
            assert_eq!(
                bsearch(item.0, &data, |i, k| if k.starts_with(i.0) {
                    Ordering::Equal
                } else {
                    i.0.cmp(k)
                }),
                Some(item)
            );
        }

        // Try again with an even number of items
        let data = &data[..6];
        for item in data {
            assert_eq!(
                bsearch(item.0, &data, |i, k| if k.starts_with(i.0) {
                    Ordering::Equal
                } else {
                    i.0.cmp(k)
                }),
                Some(item)
            );
        }
    }

    #[test]
    fn test_multi_search() {
        static ARGS: [ArgInfo<ArgData>; 1] = [take_arg!("-include", OsString, Concatenated, Foo)];
        static ARGS2: [ArgInfo<ArgData>; 1] = [take_arg!("-include-pch", OsString, Concatenated, Foo)];
        static ARGS3: [ArgInfo<ArgData>; 1] = [take_arg!("-include", PathBuf, Concatenated, FooPath)];

        assert_eq!((&ARGS[..], &ARGS2[..]).search("-include"), Some(&ARGS[0]));
        assert_eq!(
            (&ARGS[..], &ARGS2[..]).search("-include-pch"),
            Some(&ARGS2[0])
        );
        assert_eq!((&ARGS2[..], &ARGS[..]).search("-include"), Some(&ARGS[0]));
        assert_eq!(
            (&ARGS2[..], &ARGS[..]).search("-include-pch"),
            Some(&ARGS2[0])
        );
        assert_eq!((&ARGS[..], &ARGS3[..]).search("-include"), Some(&ARGS3[0]));
    }

    #[test]
    fn test_argsiter() {
        ArgData!{
            Bar(()),
            Foo(OsString),
            Fuga(()),
            Hoge(PathBuf),
            Plop(()),
            Qux(OsString),
            Zorglub(()),
        }

        // Need to explicitly refer to enum because `use` doesn't work if it's in a module
        // https://internals.rust-lang.org/t/pre-rfc-support-use-enum-for-function-local-enums/3853/13
        static ARGS: [ArgInfo<ArgData>; 7] = [
            flag!("-bar", ArgData::Bar),
            take_arg!("-foo", OsString, Separated, ArgData::Foo),
            flag!("-fuga", ArgData::Fuga),
            take_arg!("-hoge", PathBuf, Concatenated, ArgData::Hoge),
            flag!("-plop", ArgData::Plop),
            take_arg!("-qux", OsString, CanBeSeparated('='), ArgData::Qux),
            flag!("-zorglub", ArgData::Zorglub),
        ];

        let args = [
            "-nomatch",
            "-foo",
            "value",
            "-hoge",
            "value", // -hoge doesn't take a separate value
            "-hoge=value", // = is not recognized as a separator
            "-hogevalue",
            "-zorglub",
            "-qux",
            "value",
            "-plop",
            "-quxbar", // -quxbar is not -qux with a value of bar
            "-qux=value",
        ];
        let iter = ArgsIter::new(args.into_iter().map(OsString::from), &ARGS[..]);
        let expected = vec![
            arg!(UnknownFlag("-nomatch")),
            arg!(WithValue("-foo", ArgData::Foo("value"), Separated)),
            arg!(WithValue("-hoge", ArgData::Hoge(""), Concatenated)),
            arg!(Raw("value")),
            arg!(WithValue("-hoge", ArgData::Hoge("=value"), Concatenated)),
            arg!(WithValue("-hoge", ArgData::Hoge("value"), Concatenated)),
            arg!(Flag("-zorglub", ArgData::Zorglub(()))),
            arg!(WithValue("-qux", ArgData::Qux("value"), CanBeConcatenated('='))),
            arg!(Flag("-plop", ArgData::Plop(()))),
            arg!(UnknownFlag("-quxbar")),
            arg!(WithValue("-qux", ArgData::Qux("value"), CanBeSeparated('='))),
        ];
        match diff_with(iter, expected, |ref a, ref b| {
            assert_eq!(a.as_ref().unwrap(), *b);
            true
        }) {
            None => {}
            Some(Diff::FirstMismatch(_, _, _)) => unreachable!(),
            Some(Diff::Shorter(_, i)) => assert_eq!(i.map(|a| a.unwrap()).collect::<Vec<_>>(), vec![]),
            Some(Diff::Longer(_, i)) => {
                assert_eq!(Vec::<Argument<ArgData>>::new(), i.collect::<Vec<_>>())
            }
        }
    }

    #[test]
    fn test_argument_into_iter() {
        // Needs type annotation or ascription
        let raw: Argument<ArgData> = arg!(Raw("value"));
        let unknown: Argument<ArgData> = arg!(UnknownFlag("-foo"));
        assert_eq!(Vec::from_iter(raw), ovec!["value"]);
        assert_eq!(Vec::from_iter(unknown), ovec!["-foo"]);
        assert_eq!(Vec::from_iter(arg!(Flag("-foo", FooFlag(())))), ovec!["-foo"]);

        let arg = arg!(WithValue("-foo", Foo("bar"), Concatenated));
        assert_eq!(Vec::from_iter(arg), ovec!["-foobar"]);

        let arg = arg!(WithValue("-foo", Foo("bar"), Concatenated('=')));
        assert_eq!(Vec::from_iter(arg), ovec!["-foo=bar"]);

        let arg = arg!(WithValue("-foo", Foo("bar"), CanBeSeparated));
        assert_eq!(Vec::from_iter(arg), ovec!["-foobar"]);

        let arg = arg!(WithValue("-foo", Foo("bar"), CanBeSeparated('=')));
        assert_eq!(Vec::from_iter(arg), ovec!["-foo=bar"]);

        let arg = arg!(WithValue("-foo", Foo("bar"), CanBeConcatenated));
        assert_eq!(Vec::from_iter(arg), ovec!["-foo", "bar"]);

        let arg = arg!(WithValue("-foo", Foo("bar"), CanBeConcatenated('=')));
        assert_eq!(Vec::from_iter(arg), ovec!["-foo", "bar"]);

        let arg = arg!(WithValue("-foo", Foo("bar"), Separated));
        assert_eq!(Vec::from_iter(arg), ovec!["-foo", "bar"]);
    }

    #[cfg(debug_assertions)]
    mod assert_tests {
        use super::*;

        #[test]
        #[should_panic]
        fn test_arginfo_process_flag() {
            flag!("-foo", FooFlag).process("-bar", || None).unwrap();
        }

        #[test]
        #[should_panic]
        fn test_arginfo_process_take_arg() {
            take_arg!("-foo", OsString, Separated, Foo).process("-bar", || None).unwrap();
        }

        #[test]
        #[should_panic]
        fn test_arginfo_process_take_concat_arg() {
            take_arg!("-foo", OsString, Concatenated, Foo).process("-bar", || None).unwrap();
        }

        #[test]
        #[should_panic]
        fn test_arginfo_process_take_concat_arg_delim() {
            take_arg!("-foo", OsString, Concatenated('='), Foo).process("-bar", || None).unwrap();
        }

        #[test]
        #[should_panic]
        fn test_arginfo_process_take_concat_arg_delim_same() {
            take_arg!("-foo", OsString, Concatenated('='), Foo).process("-foo", || None).unwrap();
        }

        #[test]
        #[should_panic]
        fn test_arginfo_process_take_maybe_concat_arg() {
            take_arg!("-foo", OsString, CanBeSeparated, Foo).process("-bar", || None).unwrap();
        }

        #[test]
        #[should_panic]
        fn test_arginfo_process_take_maybe_concat_arg_delim() {
            take_arg!("-foo", OsString, CanBeSeparated('='), Foo).process("-bar", || None).unwrap();
        }

        #[test]
        #[should_panic]
        fn test_args_iter_unsorted() {
            static ARGS: [ArgInfo<ArgData>; 2] = [flag!("-foo", FooFlag), flag!("-bar", FooFlag)];
            ArgsIter::new(Vec::<OsString>::new().into_iter(), &ARGS[..]);
        }

        #[test]
        #[should_panic]
        fn test_args_iter_unsorted_2() {
            static ARGS: [ArgInfo<ArgData>; 2] = [flag!("-foo", FooFlag), flag!("-foo", FooFlag)];
            ArgsIter::new(Vec::<OsString>::new().into_iter(), &ARGS[..]);
        }

        #[test]
        fn test_args_iter_no_conflict() {
            static ARGS: [ArgInfo<ArgData>; 2] = [flag!("-foo", FooFlag), flag!("-fooz", FooFlag)];
            ArgsIter::new(Vec::<OsString>::new().into_iter(), &ARGS[..]);
        }
    }
}
