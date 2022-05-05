use std::fs::File;
use std::ffi::OsString;
use std::io::Read;
use std::io::stdin;
use std::io::Write;
use std::process::exit;
use std::thread::sleep;
use std::thread::spawn;
use std::time::Duration;
use chrono::Local;
use clap::Arg;
use clap::Command;
use futures::executor::block_on;
use yahoo_finance_api::Quote;
use yahoo_finance_api::YahooConnector;
use yahoo_finance_api::YahooError;
use serde::Deserialize;
use serde::Serialize;

static DEFAULT_CONFIG: &[u8] = "[[tickers]]\nid = \"GOOG\"\nvolume = 2\n\n[[tickers]]\nid = \"MSFT\"\nvolume = 5\n".as_bytes();
static DEFAULT_CONFIG_NAME: &str = "config.toml";

fn main() {
    let matches = Command::new("Stock Spreadsheet Generator")
        .version("0.4.0")
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

    let provider = YahooConnector::new();
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

    let result: Result<Config, toml::de::Error> = toml::from_str(conf.as_str());
    let config = match result {
        Ok(cfg) => cfg,
        Err(error) => panic!("Failed to parse config! {:}", error),
    };

    println!("Query time: {}", Local::now());

    let output_filename: String = if filename == DEFAULT_CONFIG_NAME {
        "output.csv".to_string()
    } else {
        format!("{}{}", filename.replace(".toml", ""), ".csv")
    };

    let mut writer = match csv::Writer::from_path(&output_filename) {
        Ok(writer) => writer,
        Err(error) => {
            print!("Error: Problem writing csv - {:?}", error);
            wait_for_input();
            return;
        }
    };

    for quote in block_on(query_tickers(&config.tickers, &provider)) {
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
            println!("Warning: Failed to open output file!");
            wait_for_input();
        }
    });
    sleep(Duration::from_millis(1000));
    exit(0);
}

async fn query_tickers(tickers: &Vec<Ticker>, provider: &YahooConnector) -> Vec<ComparingQuote> {
    let mut rows = Vec::new();
    for t in tickers {
        rows.push(deserialize_yahoo(provider, t).await);
    }
    rows
}

async fn deserialize_yahoo(provider: &YahooConnector, t: &Ticker) -> ComparingQuote {
    let volume = t.volume.unwrap_or(0);
    let close = match provider.get_latest_quotes(t.id.as_str(), "1m").await {
        Ok(quotes) => match quotes.last_quote() {
            Ok(quote) => quote.close,
            Err(error) => {
                println!("Problem deserializing latest quote: {:?}", error);
                return create_failed_row(t);
            }
        },
        Err(error0) => {
            println!("Warning: Failed to obtain quotes: {:?}", error0);
            return create_failed_row(t);
        }
    };

    let prev_close = match get_yesterday_close(provider, t).await {
        Ok(quotes) => quotes.get(0).unwrap().close,
        Err(error) => {
            println!("Warning: Failed to obtain previous quote: {:?}", error);
            return ComparingQuote {
                id: t.id.to_string(),
                close: Some(format!("{:.2}", close)),
                change: None,
                volume: t.volume,
                total: Some(format!("{:.2}", ((volume as f64) * close)))
            };
        }
    };

    return ComparingQuote {
        id: t.id.to_string(),
        close: Some(format!("{:.2}", close)),
        change: Some(format!("{:.2}", close - prev_close)),
        volume: t.volume,
        total: Some(format!("{:.2}", ((volume as f64) * close)))
    };
}

async fn get_yesterday_close(provider: &YahooConnector, t: &Ticker) -> Result<Vec<Quote>, YahooError> {
    return match provider.get_quote_range(t.id.as_str(), "1d", "2d").await {
        Ok(data) => {
            data.quotes()
        },
        Err(err) => Err(err)
    };
}

fn create_failed_row(t: &Ticker) -> ComparingQuote {
    ComparingQuote {
        id: t.id.to_string(),
        close: None,
        change: None,
        volume: t.volume,
        total: None
    }
}

fn wait_for_input() {
    println!("please press enter to close the window...");
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

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct ComparingQuote {
    #[serde(rename(serialize = "Ticker"))]
    id: String,
    #[serde(rename(serialize = "Value"))]
    close: Option<String>,
    change: Option<String>,
    volume: Option<u32>,
    total: Option<String>,
}
