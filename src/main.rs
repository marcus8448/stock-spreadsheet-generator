use std::fs::File;
use std::ffi::OsString;
use std::io::Read;
use std::io::stdin;
use std::io::Write;
use std::process::exit;
use std::thread::sleep;
use std::thread::spawn;
use std::time::Duration;
use async_compat::CompatExt;
use chrono::Local;
use clap::Arg;
use clap::Command;
use futures::executor::block_on;
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
            print!("Error: Problem writing csv - {:?}", error);
            wait_for_input();
            return;
        }
    };

    for quote in block_on(query_tickers(&config.tickers)) {
        match writer.serialize(quote) {
            Ok(_) => {}
            Err(error) => println!("Warning: Problem writing csv record: - {:?}", error),
        };
    }

    match writer.flush() {
        Ok(_) => {}
        Err(error) => {
            print!("Error: Problem flushing csv - {:?}", error);
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
        close: f64,
        prev_close: f64,
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
                        close: quote.meta.regular_market_price,
                        prev_close: quote.meta.previous_close.unwrap_or(quote.meta.chart_previous_close),
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

    return FormattedQuote {
        id: t.id.to_string(),
        close: Some(format!("{:.2}", quote.close)),
        change: Some(format!("{:.2}", quote.close - quote.prev_close)),
        amount: t.volume,
        total: Some(format!("{:.2}", ((volume as f64) * quote.close))),
        currency: quote.currency
    };
}
fn create_failed_row(t: &Ticker) -> FormattedQuote {
    FormattedQuote {
        id: t.id.to_string(),
        close: None,
        change: None,
        amount: t.volume,
        total: None,
        currency: "".to_string()
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
    #[serde(rename(serialize = "Ticker"))]
    id: String,
    #[serde(rename(serialize = "Value"))]
    close: Option<String>,
    change: Option<String>,
    amount: Option<u32>,
    total: Option<String>,
    currency: String
}
