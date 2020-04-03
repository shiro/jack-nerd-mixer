use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

use dbus::blocking::Connection;
use dbus::blocking::LocalConnection;
use dbus::tree::Factory;
use failure::Error;

use crate::args::Args;
use crate::{args, MixerCommand, MixerResponse};

fn generic_dbus_error() -> dbus::tree::MethodErr {
    ("org.freedesktop.DBus.Error.Failed", "Internal error").into()
}

const DBUS_PATH: &'static str = "com.jackAutoconnect.jackAutoconnect";

enum DbusRoute {
    InstanceRunning,
    AddStrip,
    RemoveStrip,
    SetGainFactor,
    GetState,
}

impl DbusRoute {
    fn to_string(&self) -> &'static str {
        match *self {
            DbusRoute::InstanceRunning => "InstanceRunning",
            DbusRoute::SetGainFactor => "SetGainFactor",
            DbusRoute::AddStrip => "AddStrip",
            DbusRoute::RemoveStrip => "RemoveStrip",
            DbusRoute::GetState => "GetState",
        }
    }
}

pub(crate) fn connect_dbus(args: args::Args) -> Result<Option<()>, Error> {
    let connection = Connection::new_session()?;

    let proxy = connection.with_proxy(DBUS_PATH, "/", Duration::from_millis(5000));

    if let Err(_) =
        proxy.method_call(DBUS_PATH, DbusRoute::InstanceRunning.to_string(), ()): Result<(), _>
    {
        return Ok(None);
    }

    if let Args {
        strip_name: Some(ref name),
        gain_factor: Some(ref gain_factor),
        ..
    } = args
    {
        proxy.method_call(
            DBUS_PATH,
            DbusRoute::SetGainFactor.to_string(),
            (name, gain_factor),
        )?;
        return Ok(Some(()));
    }

    if let Args {
        strip_name: Some(ref name),
        add_strip: true,
        ..
    } = args
    {
        proxy.method_call(DBUS_PATH, DbusRoute::AddStrip.to_string(), (name,))?;
        return Ok(Some(()));
    }

    if let Args {
        strip_name: Some(ref name),
        remove_strip: true,
        ..
    } = args
    {
        proxy.method_call(DBUS_PATH, DbusRoute::RemoveStrip.to_string(), (name,))?;
        return Ok(Some(()));
    }

    let (res,): (Vec<String>,) =
        proxy.method_call(DBUS_PATH, DbusRoute::GetState.to_string(), ())?;

    for line in res {
        println!("{}", line);
    }

    Ok(Some(()))
}

pub struct CommandWorkerContext {
    pub join_handle: JoinHandle<()>,
    pub join_signal_tx: mpsc::Sender<()>,
    pub command_rx: mpsc::Receiver<MixerCommand>,
    pub response_tx: crossbeam_channel::Sender<Result<MixerResponse, Error>>,
}

fn handle_mixer_command(
    command_tx: &mpsc::Sender<MixerCommand>,
    response_rx: &crossbeam_channel::Receiver<Result<MixerResponse, Error>>,
    command: MixerCommand,
) -> Result<MixerResponse, dbus::tree::MethodErr> {
    if command_tx.send(command).is_err() {
        return Err(generic_dbus_error());
    }

    let res = response_rx
        .recv()
        .map_err(|_| generic_dbus_error())
        .and_then(|m| m.map_err(|e| dbus::tree::MethodErr::failed(&e.to_string())))?;

    Ok(res)
}

pub(crate) fn start_command_worker() -> Result<CommandWorkerContext, Error> {
    let (command_tx, command_rx) = mpsc::channel();
    let (response_tx, response_rx) = crossbeam_channel::unbounded::<Result<MixerResponse, Error>>();
    let (join_signal_tx, join_signal_rx) = mpsc::channel();

    let (tx, worker_status_rx) = mpsc::channel();

    let join_handle = thread::spawn(move || {
        let mut c = match LocalConnection::new_session() {
            Ok(val) => val,
            Err(e) => {
                let _ = tx.send(Err(e));
                return;
            }
        };

        if let Err(e) = c.request_name(DBUS_PATH, false, true, false) {
            let _ = tx.send(Err(e));
            return;
        }

        let f = Factory::new_fn::<()>();

        let tree = f.tree(()).add(
            f.object_path("/", ()).add(
                f.interface(DBUS_PATH, ())
                    .add_m(
                        f.method(DbusRoute::InstanceRunning.to_string(), (), move |m| {
                            let mret = m.msg.method_return();

                            Ok(vec![mret])
                        }),
                    )
                    .add_m(f.method(DbusRoute::SetGainFactor.to_string(), (), {
                        {
                            let command_tx = command_tx.clone();
                            let response_rx = response_rx.clone();
                            move |m| {
                                let (name, gain): (&str, i32) = m.msg.read2()?;

                                let gain = match gain {
                                    n @ 0..=200 => n as f32 / 100.0,
                                    n => return Err(dbus::tree::MethodErr::invalid_arg(&n)),
                                };

                                handle_mixer_command(
                                    &command_tx,
                                    &response_rx,
                                    MixerCommand::SetGainFactor(name.to_owned(), gain),
                                )?;

                                Ok(vec![m.msg.method_return()])
                            }
                        }
                    }))
                    .add_m(f.method(DbusRoute::AddStrip.to_string(), (), {
                        {
                            let command_tx = command_tx.clone();
                            let response_rx = response_rx.clone();
                            move |m| {
                                let name: &str = m.msg.read1()?;

                                handle_mixer_command(
                                    &command_tx,
                                    &response_rx,
                                    MixerCommand::AddStrip(name.to_owned()),
                                )?;

                                Ok(vec![m.msg.method_return()])
                            }
                        }
                    }))
                    .add_m(f.method(DbusRoute::RemoveStrip.to_string(), (), {
                        {
                            let command_tx = command_tx.clone();
                            let response_rx = response_rx.clone();
                            move |m| {
                                let name: &str = m.msg.read1()?;

                                handle_mixer_command(
                                    &command_tx,
                                    &response_rx,
                                    MixerCommand::RemoveStrip(name.to_owned()),
                                )?;

                                Ok(vec![m.msg.method_return()])
                            }
                        }
                    }))
                    .add_m(f.method(DbusRoute::GetState.to_string(), (), {
                        {
                            let command_tx = command_tx.clone();
                            let response_rx = response_rx.clone();
                            move |m| {
                                let meta = match handle_mixer_command(
                                    &command_tx,
                                    &response_rx,
                                    MixerCommand::GetState,
                                )? {
                                    MixerResponse::STATE(data) => data,
                                    _ => return Err(generic_dbus_error()),
                                };

                                let msg = m.msg.method_return().append1(&meta);

                                Ok(vec![msg])
                            }
                        }
                    })),
            ),
        );
        tree.start_receive(&c);

        let _ = tx.send(Ok(()));

        loop {
            match join_signal_rx.try_recv() {
                Ok(_) | Err(mpsc::TryRecvError::Disconnected) => {
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    let _ = c.process(Duration::from_millis(1000));
                }
            }
        }
    });

    worker_status_rx
        .recv()
        .ok()
        .and_then(|res| res.ok())
        .expect("error: failed to start dbus service");

    // Ok((worker, join_signal_tx, command_rx))
    Ok(CommandWorkerContext {
        join_handle,
        join_signal_tx,
        command_rx,
        response_tx,
    })
}
