use std::fs::File;
use std::ffi::OsString;
use std::io::Read;
use std::io::stdin;
use std::io::Write;
use std::ops::{AddAssign, Mul, Sub};
use std::process::exit;
use std::thread::sleep;
use std::thread::spawn;
use std::time::Duration;
use async_compat::CompatExt;
use chrono::Local;
use clap::Arg;
use clap::Command;
use futures::executor::block_on;
use rust_decimal::Decimal;
use rust_decimal::prelude::{FromPrimitive, Zero};
use serde::Deserialize;
use serde::Serialize;

static DEFAULT_CONFIG: &[u8] = "[[tickers]]\nid = \"GOOG\"\nvolume = 2\n\n[[tickers]]\nid = \"MSFT\"\nvolume = 5\n".as_bytes();
static DEFAULT_CONFIG_NAME: &str = "config.toml";

fn main() {
    let matches = Command::new("Stock Spreadsheet Generator")
        .version("0.5.1")
        .author("marcus8448")
        .about("Creates a simple spreadsheet based on yahoo finance data")
        .arg(Arg::new("config")
            .short('c')
            .long("config")
            .takes_value(true)
            .help("the config file to read")
            .default_value(DEFAULT_CONFIG_NAME)
        )
        .get_matches();

    let filename = matches.value_of("config").unwrap();

    let path = std::path::Path::new(filename);
    if !path.exists() {
        println!("No config file found... creating one.");
        let result = File::create(filename);
        let mut conf = match result {
            Ok(conf) => conf,
            Err(error) => panic!("Failed to create default config! {:}", error),
        };
        conf.write_all(DEFAULT_CONFIG).expect("Failed to write default config!");
    }
    let mut conf = String::new();
    let result = File::open(filename);
    let mut config_file = match result {
        Ok(file) => file,
        Err(error) => panic!("Failed to open config! {:}", error),
    };
    match config_file.read_to_string(&mut conf) {
        Ok(_) => {},
        Err(error) => panic!("Failed to read config! {:}", error),
    };

    let result: Result<Config, toml::de::Error> = toml::from_slice(conf.as_bytes());
    let config = match result {
        Ok(cfg) => cfg,
        Err(error) => panic!("Failed to parse config! {:}", error),
    };

    println!("Query time: {}", Local::now());

    let output_filename = format!("{}{}", filename.replace(".toml", ""), ".csv");

    let mut writer = match csv::Writer::from_path(&output_filename) {
        Ok(writer) => writer,
        Err(error) => {
            println!("Error: Problem writing csv - {:?}", error);
            wait_for_input();
            return;
        }
    };

    let mut valid = true;
    let mut total = Decimal::zero();
    for quote in block_on(query_tickers(&config.tickers)) {
        if valid && quote.total.is_some() {
            let res = Decimal::from_str_exact((&quote.total).as_deref().unwrap());
            if res.is_err() {
                valid = false;
            }
            total.add_assign(res.unwrap());
        }
        match writer.serialize(&quote) {
            Ok(_) => {}
            Err(error) => println!("Warning: Problem writing csv record: - {:?}", error),
        };
    }

    if valid {
        writer.write_record(&["", "", "", "", "", ""]).expect("Failed to write newline in csv!?"); //newline
        writer.write_record(&["", "", "", "", &total.to_string(), ""]).expect("Failed to write total");
    }


    match writer.flush() {
        Ok(_) => {}
        Err(error) => {
            println!("Error: Problem flushing csv - {:?}", error);
            wait_for_input();
            return;
        }
    };

    spawn(|| match open::that(OsString::from(output_filename)) {
        Ok(_) => {}
        Err(_) => {
            println!("Error: Failed to open output file!");
            wait_for_input();
        }
    });
    sleep(Duration::from_millis(1000));
    exit(0);
}

async fn query_tickers(tickers: &Vec<Ticker>) -> Vec<FormattedQuote> {
    let mut rows = Vec::new();
    for t in tickers {
        rows.push(deserialize_yahoo(t).await);
    }
    rows
}

async fn deserialize_yahoo(t: &Ticker) -> FormattedQuote {
    struct Values {
        close: Decimal,
        prev_close: Decimal,
        currency: String,
    }

    let volume = t.volume.unwrap_or(0);
    let quote: Values = match reqwest::get(
        format!("https://query1.finance.yahoo.com/v8/finance/chart/{symbol}?symbol={symbol}&interval=1m&range=1m&events=div|split", symbol = t.id)
    ).compat().await {
        Ok(response) => match response.json().await {
            Ok(json) => {
                let response: Response = match serde_json::from_value(json) {
                    Ok(value) => {
                        value
                    }
                    Err(error) => {
                        println!("Warning: invalid json! {}", error);
                        return create_failed_row(t);
                    }
                };
                match response.chart.result.get(0) {
                    None => {
                        println!("Warning: missing result!");
                        return create_failed_row(t);
                    }
                    Some(quote) => Values {
                        close: Decimal::from_f64(quote.meta.regular_market_price).unwrap(),
                        prev_close: Decimal::from_f64(quote.meta.previous_close.unwrap_or(quote.meta.chart_previous_close)).unwrap(),
                        currency: quote.meta.currency.clone()
                    }
                }
            }
            Err(error) => {
                println!("Warning: malformed json! {}", error);
                return create_failed_row(t);
            }
        }
        Err(error) => {
            println!("Warning: invalid response from yahoo! {}", error);
            return create_failed_row(t);
        }
    };

    FormattedQuote {
        ticker: t.id.to_string(),
        price: Some(quote.close.round_dp(2).to_string()),
        change: Some(quote.close.sub(quote.prev_close).round_dp(2).to_string()),
        quantity: t.volume,
        total: Some(quote.close.mul(Decimal::from(volume)).round_dp(2).to_string()),
        currency: Some(quote.currency)
    }
}
fn create_failed_row(t: &Ticker) -> FormattedQuote {
    FormattedQuote {
        ticker: t.id.to_string(),
        price: None,
        change: None,
        quantity: t.volume,
        total: None,
        currency: None
    }
}

fn wait_for_input() {
    println!("Please press enter to close the window...");
    match stdin().read_exact(&mut [0]) {
        Ok(_) => {}
        Err(error) => println!("Warning: Problem reading from stdin: {:?}", error),
    }
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

#[derive(Deserialize)]
struct Response {
    chart: Chart
}

#[derive(Deserialize)]
struct Chart {
    result: Vec<Quote>
}

#[derive(Deserialize)]
struct Quote {
    meta: Meta
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    currency: String,
    chart_previous_close: f64,
    previous_close: Option<f64>,
    regular_market_price: f64
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct FormattedQuote {
    ticker: String,
    price: Option<String>,
    change: Option<String>,
    quantity: Option<u32>,
    total: Option<String>,
    currency: Option<String>
}
