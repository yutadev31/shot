mod backend;
mod capturer;
mod error;
mod frame;

pub use capturer::{Capturer, serve_clipboard};
use error::{Error, Result};
