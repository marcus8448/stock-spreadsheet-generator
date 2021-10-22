extern crate chrono;
extern crate clap;
extern crate prettytable;
extern crate yahoo_finance_api;
extern crate serde;
extern crate toml;

use std::io::{Read, Write};
use std::ops::Sub;
use chrono::{Duration, Timelike, TimeZone, Utc};
use clap::{App, Arg};
use futures::executor::block_on;
use prettytable::{Cell, Row};
use yahoo_finance_api::YahooConnector;
use serde::{Deserialize};

fn main() {
    let matches = App::new("Stock Spreadsheet Generator")
        .version("0.2.0")
        .author("marcus8448")//toml conf
        .about("Creates a simple spreadsheet based on yahoo finance data")//change from prev day
        .arg(Arg::with_name("config")
            .short("c")
            .long("config")
            .takes_value(true)
            .help("the config file to read")
            .default_value("config.toml")
        )
        .arg(Arg::with_name("run-once")
            .short("o")
            .long("run-once")
            .help("whether to query values once or overwrite old values every <wait-time> seconds (default 60)")
        )
        .arg(Arg::with_name("wait-time")
            .short("t")
            .long("wait-time")
            .takes_value(true)
            .default_value("60")
            .help("how long to wait before doing another query")
        )
        .get_matches();

    let provider = YahooConnector::new();
    let file = matches.value_of("config").unwrap();
    let wait_time_u64 = matches.value_of("wait-time").unwrap().parse::<u64>().unwrap();
    let wait_time = std::time::Duration::from_secs(wait_time_u64);
    let one_sec = std::time::Duration::from_secs(1);

    let path = std::path::Path::new(file);
    if !path.exists() {
        println!("No config file found... creating one.");
        let result = std::fs::File::create(file);
        let mut conf = match result {
            Ok(conf) => conf,
            Err(error) => panic!("Failed to create default config! {:}", error),
        };
        conf.write_all(
r#"[[tickers]]
id = "GOOG"
volume = 2

[[tickers]]
id = "MSFT"
volume = 5
"#
    .as_ref()).expect("Failed to write default config!");
    }
    let mut conf = String::new();
    let result = std::fs::File::open(file);
    let mut file = match result {
        Ok(file) => file,
        Err(error) => panic!("Failed to open config! {:}", error),
    };
    match file.read_to_string(&mut conf) {
        Ok(_) => {},
        Err(error) => panic!("Failed to read config! {:}", error),
    };

    let result: Result<Config, toml::de::Error> = toml::from_str(conf.as_str());
    let config = match result {
        Ok(cfg) => cfg,
        Err(error) => panic!("Failed to parse config! {:}", error),
    };

    loop {
        println!("Query time: {}", chrono::prelude::Local::now());

        let mut table = prettytable::Table::new();
        table.add_row(Row::new(vec!(Cell::new("Ticker"), Cell::new("Value"), Cell::new("Change"), Cell::new("Volume"), Cell::new("Total"))));

        for t in &config.tickers {
             table.add_row(deserialize_yahoo(&provider, t));
        }

        table.printstd();
        match std::fs::File::create("output.csv") {
            Ok(writer) => {
                match table.to_csv(writer) {
                    Ok(_) => {}
                    Err(error) => print!("Warning: Problem writing csv: {:?}", error)
                }
            }
            Err(error) => print!("Warning: Problem writing csv: {:?}", error)
        };

        let completion = std::time::Instant::now();
        let bar = indicatif::ProgressBar::new(wait_time_u64);
        while std::time::Instant::now().duration_since(completion) < wait_time {
            std::thread::sleep(one_sec);
            bar.inc(1);
        }
        bar.finish_and_clear();


        if matches.is_present("run-once") {
            break;
        }
    }
}

fn deserialize_yahoo(provider: &YahooConnector, t: &Ticker) -> Row {
    let future = provider.get_latest_quotes(t.id.as_str(), "1m");
    let result = block_on(future);
    let data = match result {
        Ok(data) => data,
        Err(error) => panic!("Problem communicating with Yahoo! API: {:?}", error)
    };
    let quote = match data.last_quote() {
        Ok(quote) => quote,
        Err(error) => panic!("Problem deserializing last quote: {:?}", error)
    };

    let yesterday_close_time = Utc.from_utc_datetime(&chrono::Utc::now().naive_utc().sub(Duration::days(1)).with_hour(12 + 10).unwrap().with_minute(1).unwrap().with_second(1).unwrap());
    let future = provider.get_quote_history(t.id.as_str(), yesterday_close_time, yesterday_close_time/* + Duration::minutes(30)*/);
    let result = block_on(future);
    let data = match result {
        Ok(data) => data,
        Err(error) => panic!("Problem communicating with Yahoo! API: {:?}", error)
    };
    let quotes = match data.quotes() {
        Ok(quotes) => quotes,
        Err(error) => panic!("Problem deserializing previous quotes: {:?}", error)
    };

    return Row::new(vec!(Cell::new(t.id.as_str()), Cell::new(format!("{:.2}", quote.close).as_str()), Cell::new(format!("{:.2}", quote.close - quotes.get(0).unwrap().close).as_str()), Cell::new(t.volume.unwrap_or(0).to_string().as_str()), Cell::new(&*format!("{:.2}", ((t.volume.unwrap_or(0) as f64) * quote.close)))));
}

#[derive(Deserialize)]
struct Config {
    tickers: Vec<Ticker>,
}

#[derive(Deserialize)]
struct Ticker {
    id: String,
    volume: Option<u32>,
}