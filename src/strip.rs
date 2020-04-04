use crate::errors::StripError;
use crate::jack_internal::IgnoreNotifications;
use crate::ProcessorContext;
use failure::err_msg;
use failure::Error;
use std::sync::{Arc, Mutex};

pub struct Strip {
    client: jack::AsyncClient<IgnoreNotifications, ProcessorContext>,
    state: Arc<Mutex<StripState>>,
}

impl Strip {
    pub fn new(name: String) -> Result<Self, Error> {
        let (cli, ..) = jack::Client::new(
            &format!("rust-mixer/{}", name),
            jack::ClientOptions::NO_START_SERVER,
        )?;

        let state = Arc::new(Mutex::new(StripState::new(name)));

        let processor_context = ProcessorContext {
            state: state.clone(),
        };

        let active_cli = cli.activate_async(IgnoreNotifications, processor_context)?;

        let mut strip = Strip {
            client: active_cli,
            state,
        };

        strip.set_channels(2)?;

        Ok(strip)
    }

    pub fn destroy(self) -> Result<(), Error> {
        let _ = self.client.deactivate()?;

        Ok(())
    }

    pub fn get_channels(&self) -> Result<usize, Error> {
        let res = self
            .state
            .lock()
            .or(Err(StripError::Internal))?
            .channels
            .len();

        Ok(res)
    }

    pub fn get_gain_factor(&self) -> Result<f32, Error> {
        Ok(self.state.lock().or(Err(StripError::Internal))?.gain_factor)
    }

    pub fn add_channel(&mut self) -> Result<(), Error> {
        let id = self.get_channels()? + 1;
        let client = self.client.as_client();
        let state = &mut self.state.lock().map_err(|_| StripError::Internal)?;

        state.channels.push((
            client.register_port(format!("in-{}", &id).as_str(), jack::AudioIn::default())?,
            client.register_port(format!("out-{}", &id).as_str(), jack::AudioOut::default())?,
        ));

        Ok(())
    }

    pub(crate) fn remove_channel(&mut self) -> Result<(), Error> {
        let state = &mut self.state.lock().map_err(|_| StripError::Internal)?;

        if state.channels.len() == 0 {
            return Err(err_msg("no channels left to remove on strip"));
        }

        let client = self.client.as_client();

        let (in_port, out_port) = state.channels.pop().ok_or(StripError::Internal)?;

        client.unregister_port(in_port)?;
        client.unregister_port(out_port)?;

        Ok(())
    }

    pub(crate) fn set_channels(&mut self, num_channels: i32) -> Result<(), Error> {
        let num_channels = match num_channels {
            n @ 0..=100 => n as usize,
            _ => return Err(err_msg("a strip must have 0-100 channels")),
        };

        while self.get_channels()? > num_channels {
            self.remove_channel()?;
        }

        while self.get_channels()? < num_channels {
            self.add_channel()?;
        }

        Ok(())
    }

    pub(crate) fn set_gain_factor(&mut self, gain_factor: f32) -> Result<(), Error> {
        let state = &mut self.state.lock().map_err(|_| StripError::Internal)?;
        state.gain_factor = gain_factor;
        Ok(())
    }
}

pub(crate) struct StripState {
    pub(crate) name: String,
    pub(crate) gain_factor: f32,
    pub(crate) channels: Vec<(jack::Port<jack::AudioIn>, jack::Port<jack::AudioOut>)>,
}

impl StripState {
    pub fn new(name: String) -> Self {
        StripState {
            name: String::from(name),
            gain_factor: 1.0,
            channels: vec![],
        }
    }
}
