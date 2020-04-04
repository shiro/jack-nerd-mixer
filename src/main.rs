#![feature(type_ascription)]
#![feature(bool_to_option)]
#![feature(type_alias_impl_trait)]

pub mod args;
pub mod dbus_worker;
mod errors;
pub mod jack_internal;
pub mod strip;

extern crate clap;
extern crate dbus;
extern crate failure;
extern crate jack;

use crate::jack_internal::IgnoreNotifications;
use crate::strip::Strip;
use errors::MixerCommandError;
use failure::err_msg;
use failure::Error;
use jack::{Client, Control, ProcessScope};
use std::borrow::BorrowMut;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use strip::StripState;

pub enum MixerCommand {
    SetGainFactor(String, f32),
    AddStrip(String),
    RemoveStrip(String),
    SetChannels(String, i32),
    GetState,
}

pub enum MixerResponse {
    EMPTY,
    STATE(Vec<String>),
}

// impl AppState {
//     pub fn add_strip(&mut self, name: String, client: &jack::Client) -> Result<(), Error> {
//         if self.strips.contains_key(&name) {
//             return Err(err_msg("strip already exists"));
//         }
//
//         let strip = StripState::new(name.clone(), client)?;
//         self.strips.insert(name, strip);
//
//         Ok(())
//     }
//
//     pub fn remove_strip(&mut self, name: String, client: &jack::Client) -> Result<(), Error> {
//         if !self.strips.contains_key(&name) {
//             return Err(err_msg(format!("strip '{}' does not exist exists", name)));
//         }
//
//         let (.., strip) = self.strips.remove_entry(&name).unwrap();
//
//         strip.destroy(client)?;
//
//         Ok(())
//     }
// }

struct ProcessorContext {
    state: Arc<Mutex<StripState>>,
}

impl jack::ProcessHandler for ProcessorContext {
    fn process(&mut self, _: &Client, ps: &ProcessScope) -> Control {
        let mut app_state = match self.state.lock() {
            Ok(state) => state,
            _ => return jack::Control::Continue,
        };

        let gain_factor = app_state.gain_factor;

        for (from, to) in app_state
            .channels
            .iter_mut()
            .map(|(from, to)| (from.as_slice(ps), to.as_mut_slice(ps)))
        {
            let len = to.len();
            let src = &from[..len];

            for i in 0..len {
                to[i] = src[i].clone() * gain_factor;
            }
        }

        jack::Control::Continue
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

    if let Some(_) = dbus_worker::connect_dbus(args)? {
        // another instance is running, we finished client work
        return Ok(());
    }

    let command_worker_context =
        dbus_worker::start_command_worker().expect("error: failed to start dbus service");

    let strip = Strip::new("music".to_owned())?;

    let mut strips = HashMap::new();
    let _ = strips.insert("music".to_owned(), strip);

    fn process_commands(
        cmd: &MixerCommand,
        strips: &mut HashMap<String, Strip>,
    ) -> Result<MixerResponse, Error> {
        match cmd {
            MixerCommand::SetGainFactor(name, gain_factor) => {
                strips
                    .get_mut(name)
                    .ok_or(MixerCommandError::UnknownStrip {
                        name: name.to_owned(),
                    })?
                    .set_gain_factor(*gain_factor)?;
            }
            MixerCommand::AddStrip(name) => {
                let strip = Strip::new(name.clone())?;
                strips.insert(name.clone(), strip);
            }
            MixerCommand::RemoveStrip(name) => {
                let strip = strips
                    .remove(name)
                    .ok_or(MixerCommandError::UnknownStrip { name: name.clone() })?;

                strip.destroy()?;
            }
            MixerCommand::GetState => {
                let mut strip_meta = vec![];

                for (ch_name, strip) in strips.iter() {
                    strip_meta.push(format!(
                        "{}: channels: {} gain-factor: {}",
                        ch_name,
                        strip.get_channels()?,
                        strip.get_gain_factor()?,
                    ));
                }
                return Ok(MixerResponse::STATE(strip_meta));
            }
            MixerCommand::SetChannels(name, count) => {
                strips
                    .get_mut(name)
                    .ok_or(MixerCommandError::UnknownStrip {
                        name: name.to_owned(),
                    })?
                    .set_channels(*count)?;
            }
        };
        Ok(MixerResponse::EMPTY)
    }

    while !stopped.load(std::sync::atomic::Ordering::SeqCst) {
        if let Ok(cmd) = command_worker_context.command_rx.try_recv() {
            let res = process_commands(&cmd, &mut strips);
            command_worker_context.response_tx.send(res)?;
        }
    }

    let names: Vec<String> = strips.keys().map(String::to_owned).collect();

    for name in names {
        if let Some(strip) = strips.remove(&name) {
            strip.destroy()?;
        }
    }

    command_worker_context.join_signal_tx.send(())?;
    command_worker_context.join_handle.join().unwrap();
    return Ok(());
}
