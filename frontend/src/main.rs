
#[macro_use]
extern crate failure;
#[macro_use]
extern crate serde_derive;
extern crate os_pipe;
extern crate futures;
extern crate tokio;
extern crate base64;
extern crate liner;
extern crate serde;
extern crate serde_json;
extern crate ctrlc;
extern crate libc;
extern crate termion;
extern crate tempfile;
extern crate protocol;

use std::sync::mpsc;

use failure::Error;

use protocol::{Multiplex, RpcResponse, Command, Process};

mod parse;
mod edit;
mod prefs;
mod comm;

use prefs::Prefs;
use comm::{BackendEndpoint, launch_backend, EndpointExt};
use edit::{Reader, SimpleReader};

#[derive(Debug)]
pub enum Event {
    Remote(Multiplex<RpcResponse>),
    Key(termion::event::Key),
    CtrlC,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Shell {
    DoNothing,
    Run {
        cmd: Command,
        redirect: Option<String>,
    },
    BeginRemote(Command),
    Exit,
}

struct Exec {
    receiver: mpsc::Receiver<Event>,
    remote: BackendEndpoint,
    reader: Box<dyn Reader>,
}

impl Exec {
    fn one_loop(&mut self) -> Result<bool, Error> {
        match self.remote.handler.waiting_for {
            None => {
                let cmd = self.reader.get_command(&mut self.remote)?;

                match cmd {
                    Shell::Exit => {
                        if self.remote.handler.remotes.len() > 1 {
                            self.remote.end_remote()?;
                        } else {
                            return Ok(false)
                        }
                    }
                    Shell::DoNothing => {}
                    Shell::BeginRemote(cmd) => {
                        self.remote.begin_remote(cmd)?;
                    }
                    Shell::Run { cmd, redirect } => {
                        let res = self.remote.run(cmd, redirect)?;
                        self.remote.handler.waiting_for = Some(res);
                    }
                }
            }
            Some(Process { id, .. }) => {
                let msg = self.receiver.recv()?;

                match msg {
                    Event::Remote(msg) => {
                        self.remote.receive(msg.clone())?;
                    }
                    Event::CtrlC => {
                        self.remote.cancel(id)?;
                    }
                    Event::Key(_) => {
                        panic!();
                    }
                }
            }
        }
        Ok(true)
    }
}

fn remote_run(receiver: mpsc::Receiver<Event>, remote: BackendEndpoint, reader: Box<dyn Reader>)
    -> Result<(), Error>
{
    let mut exec = Exec {
        receiver,
        remote,
        reader,
    };

    while exec.one_loop()? {}

    exec.reader.save_history();

    Ok(())
}

fn main() -> Result<(), Error> {
    let prefs = Prefs::load()?;
    // let remote = Box::new(SimpleRemote);
    let (sender, receiver) = mpsc::channel();

    let sender_clone = sender.clone();

    ctrlc::set_handler(move || {
        sender_clone.send(Event::CtrlC).unwrap();
    }).expect("Error setting CtrlC handler");

    let remote = launch_backend(sender)?;

    remote_run(receiver, remote, Box::new(SimpleReader::new(prefs)?))?;

    Ok(())
}
