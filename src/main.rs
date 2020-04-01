#![feature(type_ascription)]
extern crate clap;
extern crate dbus;
extern crate jack;

use dbus::blocking::Connection;
use dbus::blocking::LocalConnection;
use dbus::tree::Factory;
use std::error::Error;
use std::str::FromStr;
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
use std::{io, thread};

fn connect_dbus(
    dbus_path: &'static str,
    app_state: Arc<Mutex<AppState>>,
    args: clap::ArgMatches,
) -> Result<(), Box<dyn Error>> {
    let connection = Connection::new_session()?;

    let proxy = connection.with_proxy(
        "com.jackAutoconnect.jackAutoconnect",
        "/",
        Duration::from_millis(5000),
    );

    proxy.method_call(dbus_path, "Hello", ())?;

    if let Some(gain_factor) = args
        .value_of("gain factor")
        .and_then(|s| <i32 as FromStr>::from_str(s).ok())
    {
        proxy
            .method_call(dbus_path, "SetGain", (gain_factor,))
            .unwrap_or_else(|err| println!("error: {}", err));
    }

    Ok(())
}

fn host_dbus(
    dbus_path: &'static str,
    app_state: Arc<Mutex<AppState>>,
) -> Result<(JoinHandle<()>, mpsc::Sender<()>), Box<dyn Error>> {
    let (tx, rx) = mpsc::channel();
    let (stop_signal, stop_signal_consumer) = mpsc::channel();

    let foo = thread::spawn(move || {
        let mut c = match LocalConnection::new_session() {
            Ok(val) => val,
            Err(e) => {
                let _ = tx.send(Err(e));
                return;
            }
        };

        if let Err(e) = c.request_name("com.jackAutoconnect.jackAutoconnect", false, true, false) {
            let _ = tx.send(Err(e));
            return;
        }

        let f = Factory::new_fn::<()>();

        let tree = f.tree(()).add(
            f.object_path("/", ()).add(
                f.interface(dbus_path, ())
                    .add_m(f.method("Hello", (), move |m| {
                        let mret = m.msg.method_return();

                        Ok(vec![mret])
                    }))
                    .add_m(f.method("SetGain", (), move |m| {
                        let gain = match m.msg.read1()? {
                            n @ 0..=200 => n as f32 / 100.0,
                            n => return Err(dbus::tree::MethodErr::invalid_arg(&n)),
                        };

                        app_state
                            .lock()
                            .map_err(|_| dbus::tree::MethodErr::failed(&"internal error"))?
                            .gain_factor = gain;

                        Ok(vec![m.msg.method_return()])
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

    Ok((foo, stop_signal))
}

struct AppState {
    gain_factor: f32,
}

fn main() -> Result<(), Box<dyn Error>> {
    let matches = clap::App::new("jack-rust-mixer")
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
        .get_matches();

    let app_state = Arc::new(Mutex::new(AppState { gain_factor: 1.0 }));

    let dbus_path = "com.jackAutoconnect.jackAutoconnect";

    let (handle, stop_signal) = match connect_dbus(dbus_path, app_state.clone(), matches) {
        Ok(_) => return Ok(()),
        Err(_) => {
            host_dbus(dbus_path, app_state.clone()).expect("error: failed to start dbus service")
        }
    };

    let (client, _) =
        jack::Client::new("jack-rust-mixer", jack::ClientOptions::NO_START_SERVER).unwrap();

    let in_a = client
        .register_port("rust_in_1", jack::AudioIn::default())
        .unwrap();
    let in_b = client
        .register_port("rust_in_2", jack::AudioIn::default())
        .unwrap();
    let mut out_a = client
        .register_port("rust_out_1", jack::AudioOut::default())
        .unwrap();
    let mut out_b = client
        .register_port("rust_out_2", jack::AudioOut::default())
        .unwrap();

    let playback_callback = move |_: &jack::Client, ps: &jack::ProcessScope| -> jack::Control {
        let app_state = match app_state.lock() {
            Ok(state) => state,
            _ => return jack::Control::Continue,
        };

        for (from, to) in &mut [
            (in_a.as_slice(ps), out_a.as_mut_slice(ps)),
            (in_b.as_slice(ps), out_b.as_mut_slice(ps)),
        ] {
            let len = to.len();
            let src = &from[..len];

            for i in 0..len {
                to[i] = src[i].clone() * app_state.gain_factor;
            }
        }

        jack::Control::Continue
    };

    let jack_process_callback = jack::ClosureProcessHandler::new(playback_callback);

    let active_client = client
        .activate_async(Notifications, jack_process_callback)
        .unwrap();

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
