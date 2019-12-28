use essrpc::essrpc;
use essrpc::transports::{
    BincodeAsyncClientTransport, BincodeTransport, JSONAsyncClientTransport, JSONTransport,
    ReadWrite,
};
use essrpc::{AsyncRPCClient, BoxFuture, RPCError, RPCServer};
use futures::FutureExt;
use futures::{executor::block_on, future};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::Deref;
use std::result::Result;

type FutureBytes = BoxFuture<Vec<u8>, RPCError>;

#[derive(Debug, Deserialize, Serialize)]
pub struct TestError {
    msg: String,
}

impl fmt::Display for TestError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "error: {}", self.msg)
    }
}

impl std::error::Error for TestError {}
impl From<essrpc::RPCError> for TestError {
    fn from(error: essrpc::RPCError) -> Self {
        TestError {
            msg: format!("{}", error),
        }
    }
}

#[essrpc(async, sync)]
pub trait Foo {
    fn bar(&self, a: String, b: i32) -> Result<String, TestError>;
    fn expect_error(&self) -> Result<String, TestError>;
}

struct FooImpl;

impl FooImpl {
    fn new() -> Self {
        FooImpl {}
    }
}

impl Foo for FooImpl {
    fn bar(&self, a: String, b: i32) -> Result<String, TestError> {
        Ok(format!("{} is {}", a, b))
    }
    fn expect_error(&self) -> Result<String, TestError> {
        Err(TestError {
            msg: "iamerror".to_string(),
        })
    }
}

#[test]
fn basic_json() {
    let foo = json_foo();
    match block_on(foo.bar("the answer".to_string(), 42)) {
        Ok(result) => assert_eq!("the answer is 42", result),
        Err(e) => panic!("error: {:?}", e),
    }
}

#[test]
fn basic_bincode() {
    let foo = bincode_foo();
    match block_on(foo.bar("the answer".to_string(), 42)) {
        Ok(result) => assert_eq!("the answer is 42", result),
        Err(e) => panic!("error: {:?}", e),
    }
}

fn json_foo() -> impl FooAsync {
    let transact = |data: Vec<u8>| -> BoxFuture<Vec<u8>, RPCError> {
        future::lazy(move |_| -> Result<Vec<u8>, RPCError> {
            let mut response = Vec::new();
            let transport = JSONTransport::new(ReadWrite::new(data.deref(), &mut response));
            let mut serve = FooRPCServer::new(FooImpl::new(), transport);
            serve.serve_single_call()?;
            Ok(response)
        })
        .boxed()
    };
    FooAsyncRPCClient::new(JSONAsyncClientTransport::new(transact))
}

fn bincode_foo() -> impl FooAsync {
    let transact = |data: Vec<u8>| -> FutureBytes {
        future::lazy(move |_| {
            let mut response = Vec::new();
            let transport = BincodeTransport::new(ReadWrite::new(data.deref(), &mut response));
            let mut serve = FooRPCServer::new(FooImpl::new(), transport);
            serve.serve_single_call()?;
            Ok(response)
        })
        .boxed()
    };
    FooAsyncRPCClient::new(BincodeAsyncClientTransport::new(transact))
}
