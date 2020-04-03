use clap::crate_version;
use clap::value_t;
use failure::{err_msg, Error};

#[derive(Debug, Clone)]
pub(crate) struct Args {
    pub strip_name: Option<String>,
    pub gain_factor: Option<i32>,
    pub add_strip: bool,
}

enum ArgFields {
    StripName,
    GainFactor,
    AddStrip,
}

impl ArgFields {
    fn to_string(&self) -> &'static str {
        match *self {
            ArgFields::StripName => "StripName",
            ArgFields::GainFactor => "GainFactor",
            ArgFields::AddStrip => "AddStrip",
        }
    }
}

pub(crate) fn get_args() -> Result<Args, Error> {
    let matches = clap::App::new("jack-rust-mixer")
        .version(crate_version!())
        .author("shiro <shiro@usagi.io>")
        .about("A lightweight mixer for jack.")
        .arg(
            clap::Arg::with_name(ArgFields::StripName.to_string())
                .short('s')
                .long("strip")
                .value_name("NAME")
                .help("Specifies which strip to perform commands on")
                .takes_value(true),
        )
        .arg(
            clap::Arg::with_name(ArgFields::GainFactor.to_string())
                .short('g')
                .long("gain-factor")
                .value_name("FACTOR")
                .help("Sets the gain factor from 0 to 100")
                .takes_value(true),
        )
        .arg(
            clap::Arg::with_name(ArgFields::AddStrip.to_string())
                .short('a')
                .long("add-strip")
                .help("Adds a new strip"),
        )
        .get_matches();

    let mut args = Args {
        strip_name: matches
            .value_of(ArgFields::StripName.to_string())
            .map(String::from),
        add_strip: matches.is_present(ArgFields::AddStrip.to_string()),
        gain_factor: None,
    };

    if matches.is_present(ArgFields::GainFactor.to_string()) {
        args.gain_factor = value_t!(matches, ArgFields::GainFactor.to_string(), i32)
            .ok()
            .and_then(|n| if (0..200).contains(&n) { Some(n) } else { None })
            .map(Some)
            .ok_or_else(|| err_msg("gain factor needs to be a number (0..200)"))?;
    }

    Ok(args)
}
