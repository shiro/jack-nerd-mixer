use failure::Fail;

#[derive(Debug, Fail)]
pub enum MixerCommandError {
    #[fail(display = "unknown strip: {}", name)]
    UnknownStrip { name: String },
}

#[derive(Debug, Fail)]
pub enum StripError {
    #[fail(display = "internal error")]
    Internal,
}
