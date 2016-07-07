// Copyright 2016 Mozilla Foundation
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Traits and types for mocking process execution.
//!
//! This module provides a set of traits and types that can be used
//! to write code that expects to execute processes using `std::process::Command`
//! in a way that can be mocked for tests.
//!
//! Instead of using `Command::new()`, make your code generic using
//! `CommandCreator` as a trait bound, and use its `new_command` method.
//! `new_command` returns an object implementing `CommandChild`, which
//! mirrors the methods of `Command`.
//!
//! For production use, you can then instantiate your code with
//! `ProcessCommandCreator`, which simply returns `Command::new()` from
//! its `new_command` method.
//!
//! For testing, you can instantiate your code with `MockCommandCreator`,
//! which creates `MockCommand` objects which in turn spawn `MockChild`
//! objects. You can use `MockCommand::next_command_spawns` to provide
//! the result of `spawn` from the next `MockCommand` that it creates.
//! `MockCommandCreator::new_command` will fail an `assert` if it attempts
//! to create a command and does not have any pending `MockChild` objects
//! to hand out, so your tests must provide enough outputs for all
//! expected process executions in the test.
//!
//! If your code under test needs to spawn processes across threads, you
//! can use `CommandCreatorSync` as a trait bound, which is implemented for
//! `ProcessCommandCreator` (since it has no state), and also for
//! `Arc<Mutex<CommandCreator>>`. `CommandCreatorSync` provides a
//! `new_command_sync` method which your code can call to create new
//! objects implementing `CommandChild` in a thread-safe way. Your tests can
//! then create an `Arc<Mutex<MockCommandCreator>>` and safely provide
//! `MockChild` outputs.

#[cfg(unix)]
use libc;
use std::boxed::Box;
use std::ffi::OsStr;
use std::fmt;
use std::io::{
    self,
    Read,
    Write,
};
use std::path::Path;
use std::process::{
    Child,
    ChildStderr,
    ChildStdin,
    ChildStdout,
    Command,
    ExitStatus,
    Output,
    Stdio,
};
use std::sync::{Arc,Mutex};

/// A trait that provides a subset of the methods of `std::process::Child`.
pub trait CommandChild {
    type I: Write + Sync + Send + 'static;
    type O: Read + Sync + Send + 'static;
    type E: Read + Sync + Send + 'static;

    fn take_stdin(&mut self) -> Option<Self::I>;
    fn take_stdout(&mut self) -> Option<Self::O>;
    fn take_stderr(&mut self) -> Option<Self::E>;
    fn wait(&mut self) -> io::Result<ExitStatus>;
    fn wait_with_output(self) -> io::Result<Output>;
}

/// A trait that provides a subset of the methods of `std::process::Command`.
pub trait RunCommand : fmt::Debug {
    type C: CommandChild + 'static;

    fn arg<S: AsRef<OsStr>>(&mut self, arg: S) -> &mut Self;
    fn args<S: AsRef<OsStr>>(&mut self, args: &[S]) -> &mut Self;
    fn current_dir<P: AsRef<Path>>(&mut self, dir: P) -> &mut Self;
    fn stdin(&mut self, cfg: Stdio) -> &mut Self;
    fn stdout(&mut self, cfg: Stdio) -> &mut Self;
    fn stderr(&mut self, cfg: Stdio) -> &mut Self;
    fn spawn(&mut self) -> io::Result<Self::C>;
}

/// A trait that provides a means to create objects implementing `RunCommand`.
///
/// This is provided so that `MockCommandCreator` can have state for testing.
pub trait CommandCreator : Send {
    type Cmd: RunCommand;

    fn new() -> Self;
    fn new_command<S: AsRef<OsStr>>(&mut self, program: S) -> Self::Cmd;
}

/// A trait for simplifying the normal case while still allowing the mock case requiring mutability.
pub trait CommandCreatorSync : Clone + Send {
    type Cmd: RunCommand;

    fn new() -> Self;
    fn new_command_sync<S: AsRef<OsStr>>(&mut self, program: S) -> Self::Cmd;
}

/// Trivial implementation of `CommandChild` for `std::process::Child`.
impl CommandChild for Child {
    type I = ChildStdin;
    type O = ChildStdout;
    type E = ChildStderr;

    fn take_stdin(&mut self) -> Option<ChildStdin> { self.stdin.take() }
    fn take_stdout(&mut self) -> Option<ChildStdout> { self.stdout.take() }
    fn take_stderr(&mut self) -> Option<ChildStderr> { self.stderr.take() }

    fn wait(&mut self) -> io::Result<ExitStatus> {
        self.wait()
    }
    fn wait_with_output(self) -> io::Result<Output> {
        self.wait_with_output()
    }
}

/// Trivial implementation of `RunCommand` for `std::process::Command`.
impl RunCommand for Command {
    type C = Child;

    fn arg<S: AsRef<OsStr>>(&mut self, arg: S) -> &mut Command {
        self.arg(arg)
    }
    fn args<S: AsRef<OsStr>>(&mut self, args: &[S]) -> &mut Command {
        self.args(args)
    }
    fn current_dir<P: AsRef<Path>>(&mut self, dir: P) -> &mut Command {
        self.current_dir(dir)
    }
    fn stdin(&mut self, cfg: Stdio) -> &mut Command {
        self.stdin(cfg)
    }
    fn stdout(&mut self, cfg: Stdio) -> &mut Command {
        self.stdout(cfg)
    }
    fn stderr(&mut self, cfg: Stdio) -> &mut Command {
        self.stderr(cfg)
    }
    fn spawn(&mut self) -> io::Result<Child> {
        self.spawn()
    }
}

/// Unit struct to use `RunCommand` with `std::system::Command`.
#[derive(Clone)]
pub struct ProcessCommandCreator;

/// Trivial implementation of `CommandCreator` for `ProcessCommandCreator`.
impl CommandCreator for ProcessCommandCreator {
    type Cmd = Command;

    fn new() -> ProcessCommandCreator {
        ProcessCommandCreator
    }
    fn new_command<S: AsRef<OsStr>>(&mut self, program: S) -> Command {
        Command::new(program)
    }
}

/// Trivial implementation of `CommandCreatorSync` for `ProcessCommandCreator`.
impl CommandCreatorSync for ProcessCommandCreator {
    type Cmd = Command;

    fn new() -> ProcessCommandCreator {
        ProcessCommandCreator
    }
    fn new_command_sync<S: AsRef<OsStr>>(&mut self, program: S) -> Command {
        // This doesn't actually use any mutable state.
        Command::new(program)
    }
}

#[cfg(unix)]
pub type ExitStatusValue = libc::c_int;

#[cfg(windows)]
// DWORD
pub type ExitStatusValue = u32;

#[allow(dead_code)]
struct InnerExitStatus(ExitStatusValue);

/// Hack until `ExitStatus::from_raw()` is stable.
#[allow(dead_code)]
pub fn exit_status(v : ExitStatusValue) -> ExitStatus {
    use std::mem::transmute;
    unsafe { transmute(InnerExitStatus(v)) }
}

/// A struct that mocks `std::process::Child`.
#[allow(dead_code)]
#[derive(Debug)]
pub struct MockChild {
    //TODO: this doesn't work to actually track writes...
    /// A `Cursor` to hand out as stdin.
    pub stdin: Option<io::Cursor<Vec<u8>>>,
    /// A `Cursor` to hand out as stdout.
    pub stdout: Option<io::Cursor<Vec<u8>>>,
    /// A `Cursor` to hand out as stderr.
    pub stderr: Option<io::Cursor<Vec<u8>>>,
    /// The `Result` to be handed out when `wait` is called.
    pub wait_result: Option<io::Result<ExitStatus>>,
}

/// A mocked child process that simply returns stored values for its status and output.
impl MockChild {
    /// Create a `MockChild` that will return the specified `status`, `stdout`, and `stderr` when waited upon.
    #[allow(dead_code)]
    pub fn new<T: AsRef<[u8]>>(status: ExitStatus, stdout: T, stderr: T) -> MockChild {
        MockChild {
            stdin: Some(io::Cursor::new(vec!())),
            stdout: Some(io::Cursor::new(stdout.as_ref().to_vec())),
            stderr: Some(io::Cursor::new(stderr.as_ref().to_vec())),
            wait_result: Some(Ok(status)),
        }
    }

    /// Create a `MockChild` that will return the specified `err` when waited upon.
    #[allow(dead_code)]
    pub fn with_error(err: io::Error) -> MockChild {
        MockChild {
            stdin: None,
            stdout: None,
            stderr: None,
            wait_result: Some(Err(err)),
        }
    }
}

impl CommandChild for MockChild {
    type I = io::Cursor<Vec<u8>>;
    type O = io::Cursor<Vec<u8>>;
    type E = io::Cursor<Vec<u8>>;

    fn take_stdin(&mut self) -> Option<io::Cursor<Vec<u8>>> { self.stdin.take() }
    fn take_stdout(&mut self) -> Option<io::Cursor<Vec<u8>>> { self.stdout.take() }
    fn take_stderr(&mut self) -> Option<io::Cursor<Vec<u8>>> { self.stderr.take() }

    fn wait(&mut self) -> io::Result<ExitStatus> {
        self.wait_result.take().unwrap()
    }

    fn wait_with_output(self) -> io::Result<Output> {
        let MockChild { stdout, stderr, wait_result, .. } = self;
        wait_result.unwrap().and_then(|status| {
            Ok(Output {
                status: status,
                stdout: stdout.map(|c| c.into_inner()).unwrap_or(vec!()),
                stderr: stderr.map(|c| c.into_inner()).unwrap_or(vec!()),
            })
        })
    }
}

pub enum ChildOrCall {
    Child(io::Result<MockChild>),
    Call(Box<Fn() -> io::Result<MockChild> + Send>),
}

impl fmt::Debug for ChildOrCall {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ChildOrCall::Child(ref r) => write!(f, "ChildOrCall::Child({:?}", r),
            ChildOrCall::Call(_) => write!(f, "ChildOrCall::Call(...)"),
        }
    }
}

/// A mocked command that simply returns its `child` from `spawn`.
#[allow(dead_code)]
#[derive(Debug)]
pub struct MockCommand {
    pub child : Option<ChildOrCall>,
}

impl RunCommand for MockCommand {
    type C = MockChild;

    fn arg<S: AsRef<OsStr>>(&mut self, _arg: S) -> &mut MockCommand {
        //TODO: assert value of args
        self
    }
    fn args<S: AsRef<OsStr>>(&mut self, _args: &[S]) -> &mut MockCommand {
        //TODO: assert value of args
        self
    }
    fn current_dir<P: AsRef<Path>>(&mut self, _dir: P) -> &mut MockCommand {
        //TODO: assert value of dir
        self
    }
    fn stdin(&mut self, _cfg: Stdio) -> &mut MockCommand {
        self
    }
    fn stdout(&mut self, _cfg: Stdio) -> &mut MockCommand {
        self
    }
    fn stderr(&mut self, _cfg: Stdio) -> &mut MockCommand {
        self
    }
    fn spawn(&mut self) -> io::Result<MockChild> {
        match self.child.take().unwrap() {
            ChildOrCall::Child(c) => c,
            ChildOrCall::Call(f) => f(),
        }
    }
}

/// `MockCommandCreator` allows mocking out process creation by providing `MockChild` instances to be used in advance.
#[allow(dead_code)]
pub struct MockCommandCreator {
    /// Data to be used as the return value of `MockCommand::spawn`.
    pub children : Vec<ChildOrCall>,
}

impl MockCommandCreator {
    /// The next `MockCommand` created will return `child` from `RunCommand::spawn`.
    #[allow(dead_code)]
    pub fn next_command_spawns(&mut self, child: io::Result<MockChild>) {
        self.children.push(ChildOrCall::Child(child));
    }

    #[allow(dead_code)]
    pub fn next_command_calls<C: Fn() -> io::Result<MockChild> + Send + 'static>(&mut self, call: C) {
        self.children.push(ChildOrCall::Call(Box::new(call)));
    }
}

impl CommandCreator for MockCommandCreator {
    type Cmd = MockCommand;

    fn new() -> MockCommandCreator {
        MockCommandCreator {
            children: vec!(),
        }
    }

    fn new_command<S: AsRef<OsStr>>(&mut self, _program: S) -> MockCommand {
        assert!(self.children.len() > 0, "Too many calls to MockCommandCreator::new_command, or not enough to MockCommandCreator::new_command_spawns!");
        //TODO: assert value of program
        MockCommand {
            child: Some(self.children.remove(0)),
        }
    }
}

/// To simplify life for using a `CommandCreator` across multiple threads.
impl<T : CommandCreator> CommandCreatorSync for Arc<Mutex<T>> {
    type Cmd = T::Cmd;

    fn new() -> Arc<Mutex<T>> {
        Arc::new(Mutex::new(T::new()))
    }

    fn new_command_sync<S: AsRef<OsStr>>(&mut self, program: S) -> T::Cmd {
        self.lock().unwrap().new_command(program)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::error::Error;
    use std::ffi::OsStr;
    use std::io;
    use std::process::{
        ExitStatus,
        Output,
    };
    use std::sync::{Arc,Mutex};
    use std::thread;
    use test::utils::*;

    fn spawn_command<T : CommandCreator, S: AsRef<OsStr>>(creator : &mut T, program: S) -> io::Result<<<T as CommandCreator>::Cmd as RunCommand>::C> {
        creator.new_command(program).spawn()
    }

    fn spawn_wait_command<T : CommandCreator, S: AsRef<OsStr>>(creator : &mut T, program: S) -> io::Result<ExitStatus> {
        spawn_command(creator, program).and_then(|mut c| c.wait())
    }

    fn spawn_output_command<T : CommandCreator, S: AsRef<OsStr>>(creator : &mut T, program: S) -> io::Result<Output> {
        spawn_command(creator, program).and_then(|c| c.wait_with_output())
    }

    fn spawn_on_thread<T : CommandCreatorSync + Send + 'static>(mut t : T, really : bool) -> ExitStatus {
        thread::spawn(move || {
            if really {
                t.new_command_sync("foo").spawn().and_then(|mut c| c.wait()).unwrap()
            } else {
                exit_status(1)
            }
        }).join().unwrap()
    }

    #[test]
    fn test_mock_command_wait() {
        let mut creator = MockCommandCreator::new();
        creator.next_command_spawns(Ok(MockChild::new(exit_status(0), "hello", "error")));
        assert_eq!(0, spawn_wait_command(&mut creator, "foo").unwrap().code().unwrap());
    }

    #[test]
    #[should_panic]
    fn test_unexpected_new_command() {
        // If next_command_spawns hasn't been called enough times,
        // new_command should panic.
        let mut creator = MockCommandCreator::new();
        creator.new_command("foo").spawn().unwrap();
    }

    #[test]
    fn test_mock_command_output() {
        let mut creator = MockCommandCreator::new();
        creator.next_command_spawns(Ok(MockChild::new(exit_status(0), "hello", "error")));
        let output = spawn_output_command(&mut creator, "foo").unwrap();
        assert_eq!(0, output.status.code().unwrap());
        assert_eq!("hello".as_bytes().to_vec(), output.stdout);
        assert_eq!("error".as_bytes().to_vec(), output.stderr);
    }

    #[test]
    fn test_mock_command_calls() {
        let mut creator = MockCommandCreator::new();
        creator.next_command_calls(|| {
            Ok(MockChild::new(exit_status(0), "hello", "error"))
        });
        let output = spawn_output_command(&mut creator, "foo").unwrap();
        assert_eq!(0, output.status.code().unwrap());
        assert_eq!("hello".as_bytes().to_vec(), output.stdout);
        assert_eq!("error".as_bytes().to_vec(), output.stderr);
    }

    #[test]
    fn test_mock_spawn_error() {
        let mut creator = MockCommandCreator::new();
        creator.next_command_spawns(Err(io::Error::new(io::ErrorKind::Other, "error")));
        let e = spawn_command(&mut creator, "foo").err().unwrap();
        assert_eq!(io::ErrorKind::Other, e.kind());
        assert_eq!("error", e.description());
    }

    #[test]
    fn test_mock_wait_error() {
        let mut creator = MockCommandCreator::new();
        creator.next_command_spawns(Ok(MockChild::with_error(io::Error::new(io::ErrorKind::Other, "error"))));
        let e = spawn_wait_command(&mut creator, "foo").err().unwrap();
        assert_eq!(io::ErrorKind::Other, e.kind());
        assert_eq!("error", e.description());
    }

    #[test]
    fn test_mock_command_sync() {
        let creator = Arc::new(Mutex::new(MockCommandCreator::new()));
        next_command(&creator, Ok(MockChild::new(exit_status(0), "hello", "error")));
        assert_eq!(exit_status(0), spawn_on_thread(creator.clone(), true));
    }

    #[test]
    fn test_real_command_sync() {
        let creator = ProcessCommandCreator;
        // Don't *really* spawn a command, but ensure that the code compiles.
        assert_eq!(exit_status(1), spawn_on_thread(creator.clone(), false));
    }
}
