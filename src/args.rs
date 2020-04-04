use clap::crate_version;
use clap::value_t;
use failure::{err_msg, Error};

#[derive(Debug, Clone)]
pub(crate) struct Args {
    pub strip_name: Option<String>,
    pub gain_factor: Option<i32>,
    pub add_strip: bool,
    pub remove_strip: bool,
    pub set_strips: Option<i32>,
}

enum ArgFields {
    StripName,
    GainFactor,
    AddStrip,
    RemoveStrip,
    SetStrips,
}

impl ArgFields {
    fn to_string(&self) -> &'static str {
        match *self {
            ArgFields::StripName => "StripName",
            ArgFields::GainFactor => "GainFactor",
            ArgFields::AddStrip => "AddStrip",
            ArgFields::RemoveStrip => "RemoveStrip",
            ArgFields::SetStrips => "SetStrips",
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
        .arg(
            clap::Arg::with_name(ArgFields::RemoveStrip.to_string())
                .short('r')
                .long("remove-strip")
                .help("Removes an existing strip"),
        )
        .arg(
            clap::Arg::with_name(ArgFields::SetStrips.to_string())
                .short('c')
                .long("set-strips")
                .help("Set the number of strips")
                .takes_value(true)
                .value_name("COUNT"),
        )
        .get_matches();

    let mut args = Args {
        strip_name: matches
            .value_of(ArgFields::StripName.to_string())
            .map(String::from),
        add_strip: matches.is_present(ArgFields::AddStrip.to_string()),
        remove_strip: matches.is_present(ArgFields::RemoveStrip.to_string()),
        gain_factor: None,
        set_strips: None,
    };

    if matches.is_present(ArgFields::GainFactor.to_string()) {
        args.gain_factor = value_t!(matches, ArgFields::GainFactor.to_string(), i32)
            .ok()
            .and_then(|n| {
                if (0..=200).contains(&n) {
                    Some(n)
                } else {
                    None
                }
            })
            .map(Some)
            .ok_or_else(|| err_msg("gain factor needs to be a number (0..200)"))?;
    }

    if matches.is_present(ArgFields::SetStrips.to_string()) {
        args.set_strips = value_t!(matches, ArgFields::SetStrips.to_string(), i32)
            .ok()
            .and_then(|n| if n > 0 { Some(n) } else { None })
            .map(Some)
            .ok_or_else(|| err_msg("set strips requires a numeric argument greater than 1"))?;
    }

    Ok(args)
}
