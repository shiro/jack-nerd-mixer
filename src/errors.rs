use failure::Fail;

#[derive(Debug, Fail)]
pub enum MixerCommandError {
    #[fail(display = "unknown strip: {}", name)]
    UnknownStrip { name: String },

    #[fail(display = "internal error")]
    Internal,
}
