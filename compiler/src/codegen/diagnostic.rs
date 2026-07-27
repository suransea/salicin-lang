use std::fmt;

use crate::ast::ItemOrigin;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub message: String,
    pub origin: Option<ItemOrigin>,
}

impl Diagnostic {
    pub(super) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            origin: None,
        }
    }

    pub(super) fn at_origin(message: impl Into<String>, origin: Option<ItemOrigin>) -> Self {
        Self {
            message: message.into(),
            origin,
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
