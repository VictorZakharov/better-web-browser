use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::engine::script) enum JsErrorKind {
    Error,
    Type,
    Range,
}

#[derive(Debug, Clone)]
pub(in crate::engine::script) struct JsError {
    pub(in crate::engine::script) kind: JsErrorKind,
    pub(in crate::engine::script) message: String,
}

impl fmt::Display for JsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self.kind {
            JsErrorKind::Error => "Error",
            JsErrorKind::Type => "TypeError",
            JsErrorKind::Range => "RangeError",
        };
        write!(formatter, "{name}: {}", self.message)
    }
}

impl std::error::Error for JsError {}

pub(in crate::engine::script) type JsResult<T> = Result<T, JsError>;

#[derive(Debug, Clone)]
pub(in crate::engine::script) struct JsNativeError {
    kind: JsErrorKind,
    message: String,
}

impl JsNativeError {
    pub(in crate::engine::script) fn error() -> Self {
        Self::new(JsErrorKind::Error)
    }

    pub(in crate::engine::script) fn typ() -> Self {
        Self::new(JsErrorKind::Type)
    }

    pub(in crate::engine::script) fn range() -> Self {
        Self::new(JsErrorKind::Range)
    }

    fn new(kind: JsErrorKind) -> Self {
        Self {
            kind,
            message: String::new(),
        }
    }

    pub(in crate::engine::script) fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }
}

impl From<JsNativeError> for JsError {
    fn from(error: JsNativeError) -> Self {
        Self {
            kind: error.kind,
            message: error.message,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::engine::script) enum JsValue {
    Undefined,
    Null,
    Boolean(bool),
    Number(f64),
    String(String),
    Bytes(Vec<u8>),
    Array(Vec<JsValue>),
    Object(Vec<(String, JsValue)>),
}

impl JsValue {
    pub(in crate::engine::script) const fn undefined() -> Self {
        Self::Undefined
    }

    pub(in crate::engine::script) const fn null() -> Self {
        Self::Null
    }

    pub(in crate::engine::script) fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            _ => None,
        }
    }

    pub(in crate::engine::script) fn as_boolean(&self) -> Option<bool> {
        match self {
            Self::Boolean(value) => Some(*value),
            _ => None,
        }
    }

    pub(in crate::engine::script) fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Bytes(value) => Some(value),
            _ => None,
        }
    }

    pub(in crate::engine::script) fn to_boolean(&self) -> bool {
        match self {
            Self::Undefined | Self::Null => false,
            Self::Boolean(value) => *value,
            Self::Number(value) => *value != 0.0 && !value.is_nan(),
            Self::String(value) => !value.is_empty(),
            Self::Bytes(_) | Self::Array(_) | Self::Object(_) => true,
        }
    }

    pub(in crate::engine::script) fn to_string(
        &self,
        _context: &mut super::Context,
    ) -> JsResult<JsString> {
        Ok(JsString(self.string_value()))
    }

    pub(in crate::engine::script) fn string_value(&self) -> String {
        match self {
            Self::Undefined => "undefined".into(),
            Self::Null => "null".into(),
            Self::Boolean(value) => value.to_string(),
            Self::Number(value) => value.to_string(),
            Self::String(value) => value.clone(),
            Self::Bytes(_) => "[object Uint8Array]".into(),
            Self::Array(values) => values
                .iter()
                .map(Self::string_value)
                .collect::<Vec<_>>()
                .join(","),
            Self::Object(_) => "[object Object]".into(),
        }
    }
}

impl fmt::Display for JsValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.string_value())
    }
}

impl From<bool> for JsValue {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}

impl From<u32> for JsValue {
    fn from(value: u32) -> Self {
        Self::Number(f64::from(value))
    }
}

impl From<u8> for JsValue {
    fn from(value: u8) -> Self {
        Self::Number(f64::from(value))
    }
}

impl From<i32> for JsValue {
    fn from(value: i32) -> Self {
        Self::Number(f64::from(value))
    }
}

impl From<f64> for JsValue {
    fn from(value: f64) -> Self {
        Self::Number(value)
    }
}

impl From<String> for JsValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<JsString> for JsValue {
    fn from(value: JsString) -> Self {
        Self::String(value.0)
    }
}

#[derive(Debug, Clone)]
pub(in crate::engine::script) struct JsString(pub(in crate::engine::script) String);

impl JsString {
    pub(in crate::engine::script) fn from(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub(in crate::engine::script) fn to_std_string_escaped(&self) -> String {
        self.0.clone()
    }
}

pub(in crate::engine::script) struct Source {
    pub(in crate::engine::script) code: String,
    pub(in crate::engine::script) path: Option<PathBuf>,
}

impl Source {
    pub(in crate::engine::script) fn from_bytes(source: impl AsRef<str>) -> Self {
        Self {
            code: source.as_ref().to_string(),
            path: None,
        }
    }

    pub(in crate::engine::script) fn from_reader(
        reader: &mut impl std::io::Read,
        path: Option<&Path>,
    ) -> Self {
        let mut code = String::new();
        let _ = reader.read_to_string(&mut code);
        Self {
            code,
            path: path.map(Path::to_path_buf),
        }
    }

    pub(in crate::engine::script) fn with_path(mut self, path: &Path) -> Self {
        self.path = Some(path.to_path_buf());
        self
    }
}
