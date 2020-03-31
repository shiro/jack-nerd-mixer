extern crate clap;
extern crate dbus;
extern crate jack;

use dbus::blocking::Connection;
use dbus::blocking::LocalConnection;
use dbus::tree::Factory;
use std::error::Error;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;
use std::{io, thread};

fn connect_dbus(dbus_path: &'static str) -> Result<(), Box<dyn Error>> {
    let connection = Connection::new_session()?;

    let proxy = connection.with_proxy(
        "com.jackAutoconnect.jackAutoconnect",
        "/",
        Duration::from_millis(5000),
    );

    let (ret,): (i32,) = proxy.method_call(dbus_path, "Hello", ())?;

    println!("got ret: {}", ret);

    Ok(())
}

fn host_dbus(
    dbus_path: &'static str,
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
                f.interface(dbus_path, ()).add_m(
                    f.method("Hello", (), move |m| {
                        let mret = m.msg.method_return().append1(33);

                        Ok(vec![mret])
                    })
                    .outarg::<&str, _>("reply"),
                ),
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

fn main() -> Result<(), Box<dyn Error>> {
    let _matches = clap::App::new("jack-rust-mixer")
        .version("1.0")
        .author("shiro <shiro@usagi.io>")
        .about("A lightweight mixer for jack.")
        .get_matches();

    let dbus_path = "com.jackAutoconnect.jackAutoconnect";

    let (handle, stop_signal) = match connect_dbus(dbus_path) {
        Ok(_) => return Ok(()),
        Err(_) => host_dbus(dbus_path).expect("error: failed to start dbus service"),
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

    let gain_factor = 0.2;

    let playback_callback = move |_: &jack::Client, ps: &jack::ProcessScope| -> jack::Control {
        if gain_factor == 1.0 {
            return jack::Control::Continue;
        }

        for (from, to) in &mut [
            (in_a.as_slice(ps), out_a.as_mut_slice(ps)),
            (in_b.as_slice(ps), out_b.as_mut_slice(ps)),
        ] {
            let len = to.len();
            let src = &from[..len];

            for i in 0..len {
                to[i] = src[i].clone() * gain_factor;
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
