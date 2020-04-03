#![feature(type_ascription)]
#![feature(bool_to_option)]

pub mod args;
pub mod dbus_worker;
mod errors;
pub mod jack_internal;
pub mod strip;

extern crate clap;
extern crate dbus;
extern crate failure;
extern crate jack;

use errors::MixerCommandError;
use failure::err_msg;
use failure::{Error, Fail};
use std::borrow::BorrowMut;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use strip::Strip;

pub enum MixerCommand {
    AddStrip(String),
    RemoveStrip(String),
    SetGainFactor(String, f32),
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

    pub fn remove_strip(&mut self, name: String, client: &jack::Client) -> Result<(), Error> {
        if !self.strips.contains_key(&name) {
            return Err(err_msg(format!("strip '{}' does not exist exists", name)));
        }

        let (.., strip) = self.strips.remove_entry(&name).unwrap();

        strip.destroy(client)?;

        Ok(())
    }
}

fn main() -> Result<(), Error> {
    let args = args::get_args()?;

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

    if let Some(_) = dbus_worker::connect_dbus(args)? {
        // another instance is running, we finished client work
        return Ok(());
    }

    let command_worker_context =
        dbus_worker::start_command_worker().expect("error: failed to start dbus service");

    let jack_process_callback = {
        let app_state = app_state.clone();
        move |_: &jack::Client, ps: &jack::ProcessScope| -> jack::Control {
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
        }
    };

    let jack_process_callback = jack::ClosureProcessHandler::new(jack_process_callback);

    let active_client = client
        .activate_async(jack_internal::Notifications, jack_process_callback)
        .unwrap();

    fn process_commands(
        cmd: &MixerCommand,
        app_state: Arc<Mutex<AppState>>,
        client: &jack::Client,
    ) -> Result<(), Error> {
        match cmd {
            MixerCommand::SetGainFactor(name, gain_factor) => {
                app_state
                    .lock()
                    .map_err(|_| err_msg("internal"))?
                    .strips
                    .get_mut(name)
                    .ok_or(MixerCommandError::UnknownStrip {
                        name: String::from(name),
                    })?
                    .gain_factor = *gain_factor;
            }
            MixerCommand::AddStrip(name) => {
                app_state
                    .lock()
                    .map_err(|_| err_msg("internal"))?
                    .add_strip(name.to_owned(), client)?;
            }
            MixerCommand::RemoveStrip(name) => {
                app_state
                    .lock()
                    .map_err(|_| err_msg("internal"))?
                    .remove_strip(name.to_owned(), client)?;
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
