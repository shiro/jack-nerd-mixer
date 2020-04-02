#![feature(type_ascription)]
extern crate clap;
extern crate dbus;
extern crate failure;
extern crate jack;

use crate::MixerCommandError::UnknownStrip;
use crossbeam_channel::unbounded;
use dbus::blocking::Connection;
use dbus::blocking::LocalConnection;
use dbus::tree::Factory;
use enclose::enclose;
use failure::{err_msg, format_err};
use failure::{Error, Fail};
use std::borrow::BorrowMut;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::atomic::AtomicBool;
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{sleep, JoinHandle};
use std::time::Duration;
use std::{io, thread};

fn generic_dbus_error() -> dbus::tree::MethodErr {
    ("org.freedesktop.DBus.Error.Failed", "Internal error").into()
}

#[derive(Debug, Fail)]
enum MixerCommandError {
    #[fail(display = "unknown strip: {}", name)]
    UnknownStrip { name: String },
}

const DBUS_PATH: &'static str = "com.jackAutoconnect.jackAutoconnect";

enum DbusRoute {
    InstanceRunning,
    AddStrip,
    SetGainFactor,
}

impl DbusRoute {
    fn to_string(&self) -> &'static str {
        match *self {
            DbusRoute::InstanceRunning => "InstanceRunning",
            DbusRoute::AddStrip => "AddStrip",
            DbusRoute::SetGainFactor => "SetGainFactor",
        }
    }
}

enum MixerCommand {
    AddStrip(String),
    SetGainFactor(String, f32),
}

fn connect_dbus(args: clap::ArgMatches) -> Result<(), Error> {
    let connection = Connection::new_session()?;

    let proxy = connection.with_proxy(DBUS_PATH, "/", Duration::from_millis(5000));

    proxy.method_call(DBUS_PATH, DbusRoute::InstanceRunning.to_string(), ())?;

    if let (Some(name), Ok(gain_factor)) = (
        args.value_of("strip name"),
        clap::value_t!(args, "gain factor", i32),
    ) {
        proxy
            .method_call(
                DBUS_PATH,
                DbusRoute::SetGainFactor.to_string(),
                (name, gain_factor),
            )
            .unwrap_or_else(|err| println!("error: {}", err));
    }

    if let (Some(name), true) = (args.value_of("strip name"), args.is_present("add strip")) {
        proxy
            .method_call(DBUS_PATH, DbusRoute::AddStrip.to_string(), (name,))
            .unwrap_or_else(|err| println!("error: {}", err));
    }

    Ok(())
}

struct CommandWorkerContext {
    join_handle: JoinHandle<()>,
    join_signal_tx: mpsc::Sender<()>,
    command_rx: mpsc::Receiver<MixerCommand>,
    response_tx: crossbeam_channel::Sender<Result<(), Error>>,
}

fn start_command_worker() -> Result<(CommandWorkerContext), Error> {
    let (command_tx, command_rx) = mpsc::channel();
    let (response_tx, response_rx) = crossbeam_channel::unbounded::<Result<(), Error>>();
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

                                if command_tx
                                    .send(MixerCommand::SetGainFactor(name.to_owned(), gain))
                                    .is_err()
                                {
                                    return Err(generic_dbus_error());
                                }

                                if let Err(err) = response_rx
                                    .recv()
                                    .map_err(|_| generic_dbus_error())
                                    .and_then(|m| {
                                        m.map_err(|e| dbus::tree::MethodErr::failed(&e.to_string()))
                                    })
                                {
                                    return Err(err);
                                }

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

                                if command_tx
                                    .send(MixerCommand::AddStrip(name.to_owned()))
                                    .is_err()
                                {
                                    return Err(generic_dbus_error());
                                }

                                if let Err(err) = response_rx
                                    .recv()
                                    .map_err(|_| generic_dbus_error())
                                    .and_then(|m| {
                                        m.map_err(|e| dbus::tree::MethodErr::failed(&e.to_string()))
                                    })
                                {
                                    return Err(err);
                                }

                                Ok(vec![m.msg.method_return()])
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

struct AppState {
    strips: HashMap<String, Strip>,
}

impl AppState {
    pub fn add_strip(&mut self, name: String, client: &jack::Client) -> Result<(), Error> {
        if self.strips.contains_key(&name) {
            return Err(err_msg("strip already exists"));
        }

        let strip = Strip::new(name.clone(), client)?;
        self.strips.insert(name, strip);

        Ok(())
    }
}

struct Strip {
    name: String,
    gain_factor: f32,
    channels: Vec<(jack::Port<jack::AudioIn>, jack::Port<jack::AudioOut>)>,
}

impl Strip {
    pub fn new(name: String, client: &jack::Client) -> Result<Self, Error> {
        let mut ret = Strip {
            name: String::from(name),
            gain_factor: 1.0,
            channels: vec![],
        };

        ret.set_channels(2, client)?;

        Ok(ret)
    }

    pub fn add_channel(&mut self, client: &jack::Client) -> Result<(), Error> {
        let id = self.channels.len() + 1;
        self.channels.push((
            client.register_port(
                format!("{}-in-{}", &self.name, &id).as_str(),
                jack::AudioIn::default(),
            )?,
            client.register_port(
                format!("{}-out-{}", &self.name, &id).as_str(),
                jack::AudioOut::default(),
            )?,
        ));

        Ok(())
    }

    pub fn remove_channel(&mut self, client: &jack::Client) -> Result<(), Error> {
        if self.channels.len() == 1 {
            return Err(err_msg("cannot unregister last channel"));
        }

        let (in_port, out_port) = self.channels.pop().unwrap();

        client.unregister_port(in_port)?;
        client.unregister_port(out_port)?;

        Ok(())
    }

    pub fn set_channels(&mut self, num_channels: i32, client: &jack::Client) -> Result<(), Error> {
        let num_channels = match num_channels {
            n @ 1..=100 => n as usize,
            _ => return Err(err_msg("a can have 1-100 channels")),
        };

        while self.channels.len() > num_channels {
            self.remove_channel(client)?;
        }

        while self.channels.len() < num_channels {
            self.add_channel(client)?;
        }

        Ok(())
    }
}

fn main() -> Result<(), Error> {
    let args = clap::App::new("jack-rust-mixer")
        .version("1.0")
        .author("shiro <shiro@usagi.io>")
        .about("A lightweight mixer for jack.")
        .arg(
            clap::Arg::with_name("strip name")
                .short('s')
                .long("strip")
                .value_name("NAME")
                .help("Specifies which strip to perform commands on")
                .takes_value(true),
        )
        .arg(
            clap::Arg::with_name("gain factor")
                .short('g')
                .long("gain-factor")
                .value_name("FACTOR")
                .help("Sets the gain factor from 0 to 100")
                .takes_value(true),
        )
        .arg(
            clap::Arg::with_name("add strip")
                .short('a')
                .long("add-strip")
                .help("Adds a new strip"),
        )
        .get_matches();

    let stopped = Arc::new(AtomicBool::new(false));
    {
        let stopped = stopped.clone();

        let _ = ctrlc::set_handler(move || {
            stopped.store(true, std::sync::atomic::Ordering::SeqCst);
        });
    }

    // TODO fail
    let (mut client, _) =
        jack::Client::new("jack-rust-mixer", jack::ClientOptions::NO_START_SERVER).unwrap();

    let app_state = Arc::new(Mutex::new(AppState {
        strips: HashMap::new(),
    }));

    // TODO fail
    app_state.lock().unwrap().strips.insert(
        String::from("music"),
        Strip::new(String::from("music"), client.borrow_mut()).unwrap(),
    );

    if let Ok(_) = connect_dbus(args) {
        return Ok(());
    }

    let command_worker_context =
        start_command_worker().expect("error: failed to start dbus service");

    let playback_callback = enclose!((app_state) move |_: &jack::Client, ps: &jack::ProcessScope| -> jack::Control {
        let mut app_state = match app_state.lock() {
            Ok(state) => state,
            _ => return jack::Control::Continue,
        };
        for strip in &mut app_state.strips.values_mut() {
            for (from, to) in strip
                .channels
                .iter_mut()
                .map(|(from, to)| (from.as_slice(ps), to.as_mut_slice(ps)))
            {
                let len = to.len();
                let src = &from[..len];

                for i in 0..len {
                    to[i] = src[i].clone() * strip.gain_factor;
                }
            }
        }

        jack::Control::Continue
    });

    let jack_process_callback = jack::ClosureProcessHandler::new(playback_callback);

    let active_client = client
        .activate_async(Notifications, jack_process_callback)
        .unwrap();

    fn process_commands(
        cmd: &MixerCommand,
        app_state: Arc<Mutex<AppState>>,
        client: &jack::Client,
    ) -> Result<(), Error> {
        match cmd {
            MixerCommand::AddStrip(name) => {
                app_state
                    .lock()
                    .map_err(|_| err_msg("internal"))?
                    .add_strip(name.to_owned(), client)?;
            }
            MixerCommand::SetGainFactor(name, gain_factor) => {
                app_state
                    .lock()
                    .map_err(|_| err_msg("internal"))?
                    .strips
                    .get_mut(name)
                    .ok_or(UnknownStrip {
                        name: String::from(name),
                    })?
                    .gain_factor = *gain_factor;
            }
        };
        Ok(())
    }

    while !stopped.load(std::sync::atomic::Ordering::SeqCst) {
        if let Ok(cmd) = command_worker_context.command_rx.try_recv() {
            let res = process_commands(&cmd, app_state.clone(), active_client.as_client());
            command_worker_context.response_tx.send(res)?;
        }
    }

    active_client.deactivate()?;

    command_worker_context.join_signal_tx.send(())?;
    command_worker_context.join_handle.join().unwrap();
    return Ok(());
}

struct Notifications;

impl jack::NotificationHandler for Notifications {}
