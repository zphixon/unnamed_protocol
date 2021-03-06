use std::error::Error;
use std::fmt;
use std::io::{self, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::str;

#[cfg(feature = "layout")]
pub mod layout;
#[cfg(feature = "layout")]
pub extern crate druid;

pub mod markup;
pub mod request;
pub mod response;

pub const FROGGI_VERSION: u8 = 0;

/// Send a froggi request to a server and return its response.
pub fn send_request(to: impl ToSocketAddrs, path: &str) -> Result<response::Response, FroggiError> {
    let mut stream = TcpStream::connect(to)?;
    stream.write_all(&request::Request::new(path)?.into_bytes())?;

    Ok(response::Response::from_bytes(&mut stream)?)
}

/// Serialize a usize into a little-endian pair of bytes.
pub fn serialize_to_bytes(bytes: usize) -> (u8, u8) {
    assert!(bytes <= u16::MAX as usize);

    let high = (bytes >> 8) as u8;
    let low = (bytes & 0xff) as u8;

    (low, high)
}

/// Serialize a usize into a little-endian quartet of bytes.
pub fn serialize_to_four_bytes(bytes: usize) -> [u8; 4] {
    assert!(bytes <= u32::MAX as usize);

    let a: u8 = ((bytes & 0xff_00_00_00) >> 24) as u8;
    let b: u8 = ((bytes & 0x00_ff_00_00) >> 16) as u8;
    let c: u8 = ((bytes & 0x00_00_ff_00) >> 8) as u8;
    let d: u8 = bytes as u8;

    [d, c, b, a]
}

/// Deserialize a pair of bytes into a usize.
pub fn deserialize_bytes(bytes: &[u8]) -> usize {
    assert_eq!(bytes.len(), 2);
    let low = bytes[0];
    let high = bytes[1];
    ((high as usize) << 8) | (low as usize)
}

/// Deserialize a quartet of bytes into a usize.
pub fn deserialize_four_bytes(bytes: &[u8]) -> usize {
    assert_eq!(bytes.len(), 4);
    ((bytes[3] as usize) << 24)
        | ((bytes[2] as usize) << 16)
        | ((bytes[1] as usize) << 8)
        | (bytes[0] as usize)
}

/// FML document scan error.
#[derive(Debug)]
pub enum ScanError {
    UnknownEscapeCode { code: char },
    UnterminatedString { start_line: usize },
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ScanError::UnknownEscapeCode { code } => write!(f, "unknown escape code: {}", code),
            ScanError::UnterminatedString { start_line } => {
                write!(f, "unterminated string starting on line {}", start_line)
            }
        }
    }
}

use markup::scan::TokenKind;

#[derive(Debug)]
pub enum ParseError {
    UnexpectedToken { expected: TokenKind, got: String },
    UnbalancedParentheses,
    ExpectedStyle { got: String },
    ExpectedItem { got: String },
    UnknownStyle { style: String },
    RecursiveStyle { style: String },
    IncorrectNumberFormat { num: String, wanted: String },
}

#[rustfmt::skip]
impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseError::UnexpectedToken { expected, got }
                => write!(f, "unexpected token: expected {:?}, got {}", expected, got),
            ParseError::UnbalancedParentheses
                => write!(f, "unbalanced parentheses"),
            ParseError::ExpectedStyle { got }
                => write!(f, "expected style, got {:?}", got),
            ParseError::ExpectedItem { got }
                => write!(f, "expected item or page style, got {:?}", got),
            ParseError::UnknownStyle { style }
                => write!(f, "unknown style {:?}", style),
            ParseError::RecursiveStyle { style }
                => write!(f, "unknown style {:?}", style),
            ParseError::IncorrectNumberFormat { num, wanted }
                => write!(f, "incorrect number format: wanted {}, {:?}", wanted, num),
        }
    }
}

/// Errors that are possible in the froggi protocol.
#[derive(Debug)]
pub enum ErrorKind {
    EncodingError { error: str::Utf8Error },
    RequestFormatError,
    IOError { error: io::Error },
    ScanError { error: ScanError, line: usize },
    ParseError { error: ParseError, line: usize },
}

#[rustfmt::skip]
impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ErrorKind::EncodingError { error }
                => write!(f, "encoding error - {}", error),
            ErrorKind::RequestFormatError
                => write!(f, "request format error - {:?}", self),
            ErrorKind::IOError { error }
                => write!(f, "io error - {}", error),
            ErrorKind::ScanError { error, line }
                => write!(f, "scan error on line {} - {}", line, error),
            ErrorKind::ParseError { error, line }
                => write!(f, "parse error on line {} - {}", line, error),
        }
    }
}

#[derive(Debug)]
pub struct FroggiError {
    error: ErrorKind,
    msg: Option<String>,
}

impl FroggiError {
    pub fn new(error: ErrorKind) -> FroggiError {
        FroggiError { error, msg: None }
    }

    pub fn kind(&self) -> &ErrorKind {
        &self.error
    }

    pub fn scan(error: ScanError, line: usize) -> FroggiError {
        FroggiError {
            error: ErrorKind::ScanError { error, line },
            msg: None,
        }
    }

    pub fn io(error: io::Error) -> FroggiError {
        FroggiError {
            error: ErrorKind::IOError { error },
            msg: None,
        }
    }

    pub fn parse(error: ParseError, line: usize) -> FroggiError {
        FroggiError {
            error: ErrorKind::ParseError { error, line },
            msg: None,
        }
    }
}

impl AddMsg for FroggiError {
    fn msg(mut self, msg: String) -> FroggiError {
        match self.msg {
            Some(ref mut message) => {
                message.push_str(&format!(", {}", msg));
            }
            None => self.msg = Some(msg),
        }

        FroggiError { ..self }
    }

    fn msg_str(self, msg: &str) -> FroggiError {
        self.msg(String::from(msg))
    }
}

impl Error for FroggiError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.error {
            ErrorKind::EncodingError { error } => error.source(),
            ErrorKind::RequestFormatError => None,
            ErrorKind::IOError { error } => error.source(),
            ErrorKind::ScanError { .. } => None,
            ErrorKind::ParseError { .. } => None,
        }
    }
}

impl fmt::Display for FroggiError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}{}{}{}",
            self.error,
            if self.msg.is_some() { " (" } else { "" },
            self.msg.clone().unwrap_or(String::new()),
            if self.msg.is_some() { ")" } else { "" },
        )
    }
}

trait AddMsg {
    fn msg(self, msg: String) -> Self;
    fn msg_str(self, msg: &str) -> Self;
}

impl<T> AddMsg for Result<T, FroggiError> {
    fn msg(self, msg: String) -> Self {
        self.map_err(|e| e.msg(msg))
    }

    fn msg_str(self, msg: &str) -> Self {
        self.map_err(|e| e.msg_str(msg))
    }
}

impl From<FroggiError> for Vec<FroggiError> {
    fn from(error: FroggiError) -> Vec<FroggiError> {
        vec![error]
    }
}

impl From<str::Utf8Error> for FroggiError {
    fn from(error: str::Utf8Error) -> FroggiError {
        FroggiError {
            error: ErrorKind::EncodingError { error },
            msg: Some(String::from("could not decode text from utf8 to &str")),
        }
    }
}

impl From<std::string::FromUtf8Error> for FroggiError {
    fn from(error: std::string::FromUtf8Error) -> FroggiError {
        FroggiError {
            error: ErrorKind::EncodingError {
                error: error.utf8_error(),
            },
            msg: Some(String::from("could not decode text from utf8 to String")),
        }
    }
}

impl From<io::Error> for FroggiError {
    fn from(error: io::Error) -> FroggiError {
        FroggiError::io(error)
    }
}

#[cfg(test)]
mod test {
    #[derive(Debug)]
    pub struct TestByteError {
        pub real: u8,
        pub test: u8,
        pub i: usize,
    }

    pub fn test_bytes(real: &[u8], test: &[u8]) -> Result<(), TestByteError> {
        for (i, (test, real)) in test.iter().cloned().zip(real.iter().cloned()).enumerate() {
            if test != real {
                return Err(TestByteError { real, test, i });
            }
        }

        Ok(())
    }
}
