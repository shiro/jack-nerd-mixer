#![feature(type_ascription)]
extern crate clap;
extern crate dbus;
extern crate failure;
extern crate jack;

use dbus::blocking::Connection;
use dbus::blocking::LocalConnection;
use dbus::tree::Factory;
use enclose::enclose;
use failure::Error;
use failure::{err_msg, format_err};
use std::borrow::BorrowMut;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{sleep, JoinHandle};
use std::time::Duration;
use std::{io, thread};

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
    SetGainFactor(f32),
}

fn connect_dbus(args: clap::ArgMatches) -> Result<(), Error> {
    let connection = Connection::new_session()?;

    let proxy = connection.with_proxy(DBUS_PATH, "/", Duration::from_millis(5000));

    proxy.method_call(DBUS_PATH, DbusRoute::InstanceRunning.to_string(), ())?;

    if let Some(gain_factor) = args
        .value_of("gain factor")
        .and_then(|s| <i32 as FromStr>::from_str(s).ok())
    {
        proxy
            .method_call(
                DBUS_PATH,
                DbusRoute::SetGainFactor.to_string(),
                (gain_factor,),
            )
            .unwrap_or_else(|err| println!("error: {}", err));
    }

    if let Some(name) = args.value_of("add strip") {
        proxy
            .method_call(DBUS_PATH, DbusRoute::AddStrip.to_string(), (name,))
            .unwrap_or_else(|err| println!("error: {}", err));
    }

    Ok(())
}

fn start_command_worker() -> Result<
    (
        JoinHandle<()>,
        mpsc::Sender<()>,
        mpsc::Receiver<MixerCommand>,
    ),
    Error,
> {
    let (tx, rx) = mpsc::channel();
    let (stop_signal, stop_signal_consumer) = mpsc::channel();

    let (queue_sender, queue_receiver) = mpsc::channel::<MixerCommand>();

    let worker = thread::spawn(move || {
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
                        enclose!((queue_sender) move |m| {
                            let gain = match m.msg.read1()? {
                                n @ 0..=200 => n as f32 / 100.0,
                                n => return Err(dbus::tree::MethodErr::invalid_arg(&n)),
                            };

                            let _ = queue_sender.send(MixerCommand::SetGainFactor(gain));

                            Ok(vec![m.msg.method_return()])
                        })
                    }))
                    .add_m(f.method(DbusRoute::AddStrip.to_string(), (), {
                        move |m| {
                            let name: &str = m.msg.read1()?;

                            let _ = queue_sender.send(MixerCommand::AddStrip(name.to_owned()));

                            // app_state
                            //     .lock()
                            //     .map_err(|e| <Error>::from(e))
                            //     .and_then(|mut state| state.add_strip(name, client))
                            //     .map_err(|_| dbus::tree::MethodErr::failed(&"internal error"))?;

                            Ok(vec![m.msg.method_return()])
                        }
                    })),
            ),
        );
        tree.start_receive(&c);

        let _ = tx.send(Ok(()));

        loop {
            match stop_signal_consumer.try_recv() {
                Ok(_) | Err(mpsc::TryRecvError::Disconnected) => {
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    let _ = c.process(Duration::from_millis(1000));
                }
            }
        }
    });

    rx.recv()
        .ok()
        .and_then(|res| res.ok())
        .expect("error: failed to start dbus service");

    Ok((worker, stop_signal, queue_receiver))
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
            clap::Arg::with_name("gain factor")
                .short('g')
                .long("gain-factor")
                .value_name("FACTOR")
                .help("Sets the gain factor from 0 to 100")
                .takes_value(true),
        )
        .arg(
            clap::Arg::with_name("add strip")
                .short('s')
                .long("add-strip")
                .value_name("NAME")
                .help("Adds a new strip")
                .takes_value(true),
        )
        .get_matches();

    let (mut client, _) =
        jack::Client::new("jack-rust-mixer", jack::ClientOptions::NO_START_SERVER).unwrap();

    let app_state = Arc::new(Mutex::new(AppState {
        strips: HashMap::new(),
    }));

    app_state.lock().unwrap().strips.insert(
        String::from("music"),
        Strip::new(String::from("music"), client.borrow_mut()).unwrap(),
    );

    if let Ok(_) = connect_dbus(args) {
        return Ok(());
    }

    let (handle, stop_signal, command_queue) =
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

    // let _ = command_queue.recv().unwrap()(active_client.as_client());
    match command_queue.recv().unwrap() {
        MixerCommand::AddStrip(name) => {
            let _ = app_state
                .lock()
                .unwrap()
                .add_strip(name, active_client.as_client());
        }
        MixerCommand::SetGainFactor(gain_factor) => {
            app_state
                .lock()
                .unwrap()
                // .map_err(|_| dbus::tree::MethodErr::failed(&"internal error"))?
                .strips
                .get_mut("music")
                .unwrap()
                .gain_factor = gain_factor;
        }
    }

    println!("Press enter/return to quit...");
    let mut user_input = String::new();
    io::stdin().read_line(&mut user_input).ok();

    let _ = active_client.deactivate();

    let _ = stop_signal.send(());
    let _ = handle.join();
    return Ok(());
}

struct Notifications;

impl jack::NotificationHandler for Notifications {}
