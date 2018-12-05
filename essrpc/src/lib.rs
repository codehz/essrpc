//! Electron's Super Simple RPC (ESSRPC) is a lightweight RPC library
//! which aims to enable RPC calls as transparently as possible
//! through calls to ordinary trait methods.
//! 
//! The magic is performed by the `essrpc` attribute macro which may
//! be applied to any trait whose functions each meet the following conditions:
//! * Returns a `Result` whose error type implements `From<RPCError>`.
//! * Uses only parameter and returns types which implement `Serialize`
//! * Is not unsafe
//!
//! The `essrpc` macro generates for a trait an RPC client and a
//! server. For a trait named `Foo`, the macro will generate
//! `FooRPCClient` which implements both
//! [RPCClient](trait.RPCClient.html) and `Foo` as well as
//! `FooRPCServer` which implements [RPCServer](trait.RPCServer.html)
//! 
//! # Examples
//! A trait can apply the `essrpc` attribute like this.
//! ```ignore
//! #[essrpc]
//! pub trait Foo {
//!    fn bar(&self, a: String, b: i32) -> Result<String, SomeError>;
//! }
//! ```
//! For example purposes, assume we're using a unix socket to
//! communicate between a parent and child process. Anything else
//! implementing `Read+Write` would work just as well.
//! ```
//! # use std::os::unix::net::UnixStream;
//! let (s1, s2) = UnixStream::pair().unwrap();
//! ```
//! 
//! We can spin up a server like this
//! ```ignore
//! let mut s = FooRPCServer::new(FooImpl::new(), BincodeTransport::new(s2));
//! s.serve()
//! ```
//! Then, if we have some type `FooImpl` which implements `Foo`, we can do RPC like this.
//! ```ignore
//! let client = FooRPCClient::new(BincodeTransport::new(s1));
//! match client.bar("the answer".to_string(), 42) {
//!     Ok(result) => assert_eq!("the answer is 42", result),
//!     Err(e) => panic!("error: {:?}", e)
//! }
//! ```
// We do not do doctests on the examples above because with all the
// macros and generated code, it is simply too much effort to get things working.

extern crate bincode;
extern crate essrpc_macros;
extern crate serde;
extern crate serde_derive;
extern crate serde_json;
extern crate uuid;



// We would like to mark as #[doc(inline)] and define the
// on the macro definition site, but this does not work properly on macros
pub use essrpc_macros::essrpc;

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_derive::{Deserialize, Serialize};

pub mod transports;

type Result<T> = std::result::Result<T, RPCError>;


/// Identifies a method by both a name and an index. The Indices are
/// automatically generated in the order methods are listed on the trait.
/// Used when implementing [Transport](trait.Transport.html).
#[derive(Debug)]
pub struct MethodId {
    pub name: &'static str,
    pub num: u32
}

/// Identifies a method by either a name or an index.
/// Used when implementing [Transport](trait.Transport.html).
#[derive(Debug)]
pub enum PartialMethodId {
    Name(String),
    Num(u32)
}

/// Trait for RPC transport. ESSRPC attempts to make as few
/// assumptions about the transport as possible. A transport may work
/// across a network, via any IPC mechanism, or purely in memory within a single process.
pub trait Transport {
    /// Type of transport-internal state used when bulding a call for
    /// transmission on the client. May be unit if the transport does not need to track
    /// state or does so through member variables.
    type TXState;
    /// Type of transport-internal state used when receiving a call on
    /// the server. May be unit if the transport does not need to
    /// track state or does so through member variables.
    type RXState;

    /// Begin calling the given method. The transport may begin transmitting over the wire,
    /// or it may may wait until the call to `tx_finalize`. 
    fn tx_begin_call(&mut self, method: MethodId) -> Result<Self::TXState>;
    /// Add a parameter to a method call started with
    /// `tx_begin_call`. This method is guaranteed to be called only
    /// after `tx_begin_call` and to be called appropriately for each
    /// parameter of the method passed to `tx_begin_call`.  `state` is
    /// the object returned by `tx_begin_call`. Parameters are always
    /// added and read in order, so transmitting the name is not a requirement.
    fn tx_add_param(&mut self, name: &'static str, value: impl Serialize,
                    state: &mut Self::TXState) -> Result<()>;
    /// Finalize transmission of a method call. Called only after
    /// `tx_begin_call` and appropriate calls to `tx_add_param`. If
    /// the transport has not yet transmitted the method identifier
    /// and parameters over the wire, it should do so at this time.
    fn tx_finalize(&mut self, state: &mut Self::TXState) ->
        Result<()>;
    /// Transmit a response (from the server side) to a method call.
    fn tx_response(&mut self, value: impl Serialize) -> Result<()>;

    /// Begin reading a method cal on the server. Returns the method
    /// name or identifier and internal state.
    fn rx_begin_call(&mut self) -> Result<(PartialMethodId,
                                           Self::RXState)>;
    /// Read a method parameter after a an `rx_begin_call`. Parameters
    /// are always read in order, so some transports may choose to
    /// ignore the name.
    fn rx_read_param<T>(&mut self, name: &'static str, state: &mut Self::RXState) -> Result<T> where
        for<'de> T: serde::Deserialize<'de>;
    
    /// Read the return value of a method call. Always called after `tx_finalize`.
    fn rx_response<T>(&mut self) -> Result<T> where
        for<'de> T: Deserialize<'de>;
    
}

/// Trait implemented by all RPC clients generated by the `essrpc`
/// macro. For a trait named `Foo`, the macro will generate
/// `FooRPCClient` which implements both `RPCClient` and `Foo`.
pub trait RPCClient {
    /// Type of transport used by this client.
    type TR: Transport;
     fn new(transform: Self::TR) -> Self;
}

/// Trait implemented by all RPC servers generated by the `essrpc`
/// macro. For a trait named `Foo`, the macro will generate
/// `FooRPCServer` which implements `RPCServer`. An `RPCServer`
/// generated for a trait 'Foo' will have a `new` method
/// ```ignore
/// fn new(imp: impl Foo, transport: impl essrpc::Transport)
/// ```
/// Unfortunately, `new` is not specified as part of the RPC trait
/// as traits cannot be type parameters.
pub trait RPCServer {
    /// Serve a single RPC call.
    fn serve_single_call(&mut self) -> Result<()>;

    /// Serve RPC calls until cond() returns `false`. The condition is
    /// checked after serving a single call. It does not provide a
    /// mechanism to interrupt a server which is waiting for more data
    /// or for a connection to be established. If you need that
    /// capability, you must build it into a
    /// [Transport](trait.Transport.html)
    fn serve_until(&mut self, mut cond: impl FnMut()->bool) -> Result<()> {
        loop {
            self.serve_single_call()?;
            if !cond() {
                return Ok(());
            }
        }
    }
    
    /// Serve RPC calls indefinitely. The result will always be an
    /// error, as it attempts to serve forever.
    fn serve(&mut self) -> Result<()> {
        loop {
            self.serve_single_call()?;
        }
    }
}

/// Generic serializable error with a description and optional
/// cause. Used in conjunction with RPCError.
#[derive(Debug, Deserialize,Serialize)]
pub struct GenericSerializableError {
    description: String,
    cause: Option<Box<GenericSerializableError>>
}
impl GenericSerializableError {
    pub fn new(e: impl std::error::Error) -> Self {
        let cause = match e.source() {
            None => None,
            Some(ec) => Some(Box::new(GenericSerializableError::from_dyn(ec)))
        };
        GenericSerializableError{description: e.to_string(), cause: cause}
    }

    /// Create a `GenericSerializableError` from a trait object. This
    /// preserved the description and cause of the error (as another
    /// `GenericSerializableError`), but the specific type and
    /// backtrace of the error are lost.
    pub fn from_dyn(e: &dyn std::error::Error) -> Self {
        let cause = match e.source() {
            None => None,
            Some(ec) => Some(Box::new(GenericSerializableError::from_dyn(ec)))
        };
        GenericSerializableError{description: e.to_string(), cause: cause}
    }
}
impl std::error::Error for GenericSerializableError {
    fn source(&self) -> Option<&(std::error::Error + 'static)> {
        match self.cause {
            Some(ref e) => Some(e),
            None => None
        }
    }
}
impl fmt::Display for GenericSerializableError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.cause {
            Some(ref e) => write!(f, "{} caused by:\n {}", self.description, e),
            None => write!(f, "{}", self.description)
        }
    }
}

/// RPC error. All functions in RPC traits must return an error type
/// which implements `From<RPCError>`.
#[derive(Debug, Deserialize,Serialize)]
pub struct RPCError {
    pub kind: RPCErrorKind,
    msg: String,
    cause: Option<Box<GenericSerializableError>>
}

impl RPCError {
    /// New error without a cause.
    pub fn new(kind: RPCErrorKind, msg: impl Into<String>) -> Self {
        RPCError{kind: kind, msg: msg.into(), cause: None} 
    }
    
    /// New error with a cause. 
    pub fn with_cause(kind: RPCErrorKind, msg: impl Into<String>, cause: impl std::error::Error) -> Self {
        RPCError{kind: kind, msg: msg.into(), cause: Some(Box::new(GenericSerializableError::new(cause)))} 
    }

    /// Get the cause of the error (if any).
    pub fn cause<'a>(&'a self) -> Option<&'a GenericSerializableError> {
        match self.cause {
            None => None,
            Some(ref e) => Some(&e)
        }
    }
}

impl fmt::Display for RPCError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.cause {
            Some(ref e) => write!(f, "{} caused by:\n {}", self.msg, e),
            None => write!(f, "{}", self.msg)
        }
    }
}

impl std::error::Error for RPCError {
}

/// Types of [RPCError](trait.RPCError.html)
#[derive(Debug, Deserialize, Serialize)]
pub enum RPCErrorKind {
    /// Error caused by serialization or deserialization failure.
    SerializationError,
    /// RPC server was asked to handle an unknown method.
    UnknownMethod,
    /// Other error.
    Other,
}