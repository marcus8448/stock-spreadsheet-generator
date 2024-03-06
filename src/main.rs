mod config;
mod yahoo;

use clap::{value_parser, ArgAction};
use log::{debug, error};
use rust_decimal::prelude::*;
use std::io::Read;
use std::time::{Duration, Instant};
use yahoo::Quote;

const USER_AGENT: &'static str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

fn main() -> Result<(), Error> {
    env_logger::init();
    let matches = clap::Command::new("Stock Spreadsheet Generator")
        .version(env!("CARGO_PKG_VERSION"))
        .author("marcus8448")
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .arg(
            clap::Arg::new("config")
                .index(1)
                .value_name("FILE")
                .help("the config file to read")
                .default_value("config.toml"),
        )
        .arg(
            clap::Arg::new("delay")
                .long("delay")
                .short('d')
                .value_name("ms")
                .value_parser(value_parser!(u16))
                .help("the delay between requests to the yahoo api (in ms)")
                .default_value("300"),
        )
        .arg(
            clap::Arg::new("no-open")
                .long("no-open")
                .short('n')
                .action(ArgAction::SetTrue)
                .help("disable opening the generated csv automatically"),
        )
        .get_matches();

    let filename: &String = matches.get_one("config").expect("missing argument: config");
    let delay: u16 = *matches.get_one("delay").expect("missing argument: delay");
    let delay = std::time::Duration::from_millis(delay as u64);
    let open: bool = !matches.get_flag("no-open");

    debug!(
        "config: `{}`, delay: {}, open: {}",
        filename,
        delay.as_millis(),
        open
    );

    let config = config::load_from_file(filename);

    let mut client = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .unwrap();

    let mut quotes: Vec<(config::Ticker, Option<Quote>)> = Vec::new();

    let mut last_call: Option<Instant> = None;

    for ticker in config.tickers {
        if let Some(instant) = last_call {
            std::thread::sleep(
                delay
                    - instant
                        .checked_duration_since(Instant::now())
                        .unwrap_or(Duration::from_millis(0)),
            );
        }

        match yahoo::request_quote(&mut client, &ticker.id) {
            Ok(quote) => quotes.push((ticker, Some(quote))),
            Err(err) => {
                error!("Failed to get ticker {}: {}", &ticker.id, err);
                quotes.push((ticker, None));
            }
        };
        last_call = Some(Instant::now());
    }

    let output_filename = config
        .output_file
        .unwrap_or_else(|| format!("{}.csv", filename.strip_suffix(".toml").unwrap_or(filename)));

    write_csv(&mut quotes, &output_filename)?;

    if open {
        if let Err(error) = open::that(output_filename) {
            log::error!("Failed to open output file! {}", error);
            std::io::stdin()
                .read_exact(&mut [0_u8; 1])
                .expect("Failed to block before exit");
            return Err(Error::OpeningError(error));
        }
    }
    Ok(())
}

fn write_csv(
    quotes: &Vec<(config::Ticker, Option<Quote>)>,
    output_filename: &str,
) -> Result<(), Error> {
    let mut writer = csv::Writer::from_path(&output_filename)?;
    let mut total = Decimal::zero();
    writer.write_record(&["Ticker", "Price", "Change", "Quantity", "Total", "Currency"])?;
    for (ticker, quote) in quotes {
        let quantity = ticker.quantity.unwrap_or(0);
        match quote {
            Some(quote) => {
                writer.write_field(&ticker.id)?;
                writer.write_field(quote.price.round_dp(2).to_string())?;
                writer.write_field(quote.change.round_dp(2).to_string())?;
                writer.write_field(quantity.to_string())?;
                writer.write_field(
                    (quote.price * Decimal::from(quantity))
                        .round_dp(2)
                        .to_string(),
                )?;
                writer.write_field(&quote.currency)?;
                writer.write_record(None::<&[u8]>)?;

                total += quote.price;
            }
            None => {
                writer.write_record(&[
                    &ticker.id,
                    "",
                    "",
                    quantity.to_string().as_str(),
                    "",
                    "",
                ])?;
            }
        }
    }

    writer
        .write_record(["", "", "", "", "", ""])
        .expect("Failed to write newline in csv!");

    writer
        .write_record(["", "", "", "", &total.to_string(), ""])
        .expect("Failed to write total!");

    writer.flush()?;
    Ok(())
}

#[derive(thiserror::Error, Debug)]
enum Error {
    #[error("failed to write csv data: {0}")]
    CsvError(#[from] csv::Error),
    #[error("failed to open file with default program: {0}")]
    OpeningError(#[from] std::io::Error),
}
