use failure::err_msg;
use failure::Error;

pub(crate) struct Strip {
    name: String,
    pub(crate) gain_factor: f32,
    pub(crate) channels: Vec<(jack::Port<jack::AudioIn>, jack::Port<jack::AudioOut>)>,
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

    pub fn destroy(mut self, client: &jack::Client) -> Result<(), Error> {
        &mut self.set_channels(0, client)?;
        Ok(())
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

    pub(crate) fn remove_channel(&mut self, client: &jack::Client) -> Result<(), Error> {
        if self.channels.len() == 0 {
            return Err(err_msg("no channels left to remove on strip"));
        }

        let (in_port, out_port) = self.channels.pop().unwrap();

        client.unregister_port(in_port)?;
        client.unregister_port(out_port)?;

        Ok(())
    }

    pub(crate) fn set_channels(
        &mut self,
        num_channels: i32,
        client: &jack::Client,
    ) -> Result<(), Error> {
        let num_channels = match num_channels {
            n @ 0..=100 => n as usize,
            _ => return Err(err_msg("a strip must have 0-100 channels")),
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
