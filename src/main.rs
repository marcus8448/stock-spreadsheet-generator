use clap::Arg;
use clap::Command;
use csv::ByteRecord;
use rust_decimal::prelude::Zero;
use rust_decimal::Decimal;
use serde::Deserialize;
use std::ffi::OsString;
use std::fs::File;
use std::io::stdin;
use std::io::Read;
use std::io::Write;
use std::ops::AddAssign;
use std::ops::Mul;
use std::ops::Sub;

static DEFAULT_CONFIG: &[u8] = r#"[[tickers]]
id = "GOOG"
volume = 0

[[tickers]]
id = "MSFT"
volume = 0
"#
.as_bytes();

static DEFAULT_CONFIG_NAME: &str = "config.toml";

#[tokio::main]
async fn main() -> Result<(), u8> {
    let matches = Command::new("Stock Spreadsheet Generator")
        .version(env!("CARGO_PKG_VERSION"))
        .author("marcus8448")
        .about("Creates a spreadsheet based on yahoo finance data")
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("the config file to read")
                .default_value(DEFAULT_CONFIG_NAME),
        )
        .get_matches();

    let filename: &String = matches
        .get_one("config")
        .expect("Missing config file option!");

    let path = std::path::Path::new(filename);
    if !path.exists() {
        println!("No config file found... creating the default one.");
        let mut conf = File::create(filename).expect("Failed to create configuration file!");
        conf.write_all(DEFAULT_CONFIG)
            .expect("Failed to write the default config to a file!");
    }
    let mut config = String::new();
    File::open(filename)
        .expect("Failed to open configuration file!")
        .read_to_string(&mut config)
        .expect("Failed to read configuration file!");

    let config: Config =
        toml::from_slice(config.as_bytes()).expect("Failed to parse configuration file!");

    let output_filename = format!("{}{}", filename.replace(".toml", ""), ".csv");
    let mut writer = csv::Writer::from_path(&output_filename).expect("Failed to open output file!");

    let mut total = Decimal::zero();
    writer.write_byte_record(&ByteRecord::from(&["Ticker", "Price", "Change", "Quantity", "Total", "Currency"][..])).expect("");
    for quote in query_tickers(&config.tickers).await {
        if let Some(tot) = quote.get_total() {
            total.add_assign(tot);
        }
        if let Err(error) = quote.write_line(&mut writer) {
            println!("Problem writing csv record: {:?}", error);
        }
    }

    writer
        .write_byte_record(&ByteRecord::from(&[""; 6][..]))
        .expect("Failed to write newline in csv!");
    writer
        .write_record(["", "", "", "", &total.to_string(), ""])
        .expect("Failed to write total!");

    if let Err(error) = writer.flush() {
        println!("Failed to save csv! {}", error);
        wait_for_input();
        return Err(2);
    }

    if let Err(error) = open::that(OsString::from(output_filename)) {
        println!("Failed to open output file! {}", error);
        wait_for_input();
        return Err(2);
    }
    Ok(())
}

async fn query_tickers(tickers: &Vec<Ticker>) -> Vec<Box<dyn FormattedQuote>> {
    let mut rows: Vec<Box<dyn FormattedQuote>> = Vec::new();
    for t in tickers {
        rows.push(match deserialize_yahoo(t).await {
            //todo: check rate limits?
            Ok(quote) => Box::new(quote),
            Err(error) => {
                println!("{}", error);
                Box::new(create_failed_row(t))
            }
        });
    }
    rows
}

async fn deserialize_yahoo(t: &Ticker) -> Result<RealQuote, String> {
    struct Values {
        close: Decimal,
        prev_close: Decimal,
        currency: String,
    }

    let volume = t.quantity.unwrap_or(0);
    let res = reqwest::get(format!(
        "https://query1.finance.yahoo.com/v8/finance/chart/{symbol}?symbol={symbol}&interval=1m&range=1m&events=div|split", symbol = t.id)
    ).await;

    let Ok(response) = res else {
        return Err(format!("Warning: invalid response from yahoo! {}", res.err().unwrap()));
    };
    let res: Result<_, reqwest::Error> = response.json().await;
    let Ok(json) = res else {
        return Err(format!("Encountered malformed json. {}", res.err().unwrap()));
    };

    let res: Result<Response, serde_json::error::Error> = serde_json::from_value::<Response>(json);
    let Ok(response) = res else {
        return Err(format!("Warning: invalid json! {:}", res.err().unwrap()));
    };

    let Some(quote) = response.chart.result.get(0) else {
        return Err("Warning: missing result!".to_string());
    };

    Ok(RealQuote {
        ticker: t.id.to_string(),
        price: quote.meta.regular_market_price,
        change:
            quote.meta.regular_market_price.sub(
                quote
                    .meta
                    .previous_close
                    .unwrap_or(quote.meta.chart_previous_close),
            ),
        quantity: t.quantity.unwrap_or(0),
        total: quote.meta.regular_market_price.mul(Decimal::from(volume)),
        currency: quote.meta.currency.clone()
    })
}

fn create_failed_row(t: &Ticker) -> FailedQuote {
    FailedQuote {
        ticker: t.id.to_string(),
        quantity: t.quantity.unwrap_or(0),
    }
}

fn wait_for_input() {
    println!("Please press enter to close the window...");
    let _ = stdin().read_exact(&mut [0]);
}

#[derive(Deserialize)]
struct Config {
    tickers: Vec<Ticker>,
}

#[derive(Deserialize)]
struct Ticker {
    id: String,
    #[serde(rename = "volume")]
    quantity: Option<u32>,
}

#[derive(Deserialize)]
struct Response {
    chart: Chart,
}

#[derive(Deserialize)]
struct Chart {
    result: Vec<Quote>,
}

#[derive(Deserialize)]
struct Quote {
    meta: Meta,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    currency: String,
    chart_previous_close: Decimal,
    previous_close: Option<Decimal>,
    regular_market_price: Decimal,
}

trait FormattedQuote {
    fn get_ticker(&self) -> &String;
    fn get_price(&self) -> Option<&Decimal>;
    fn get_change(&self) -> Option<&Decimal>;
    fn get_quantity(&self) -> u32;
    fn get_total(&self) -> Option<&Decimal>;
    fn get_currency(&self) -> Option<&String>;

    fn write_line(&self, writer: &mut csv::Writer<File>) -> Result<(), csv::Error>;
}


struct RealQuote {
    ticker: String,
    price: Decimal,
    change: Decimal,
    quantity: u32,
    total: Decimal,
    currency: String
}

impl FormattedQuote for RealQuote {
    fn get_ticker(&self) -> &String {
        &self.ticker
    }

    fn get_price(&self) -> Option<&Decimal> {
        Some(&self.price)
    }

    fn get_change(&self) -> Option<&Decimal> {
        Some(&self.change)
    }

    fn get_quantity(&self) -> u32 {
        self.quantity
    }

    fn get_total(&self) -> Option<&Decimal> {
        Some(&self.total)
    }

    fn get_currency(&self) -> Option<&String> {
        Some(&self.currency)
    }

    fn write_line(&self, writer: &mut csv::Writer<File>) -> Result<(), csv::Error> {
        writer.write_field(&self.ticker)?;
        writer.write_field(&self.price.round_dp(2).to_string())?;
        writer.write_field(&self.change.round_dp(2).to_string())?;
        writer.write_field(&self.quantity.to_string())?;
        writer.write_field(&self.total.round_dp(2).to_string())?;
        writer.write_field(&self.currency)?;
        writer.write_record(None::<&[u8]>)
    }
}

struct FailedQuote {
    ticker: String,
    quantity: u32
}

impl FormattedQuote for FailedQuote {
    fn get_ticker(&self) -> &String {
        &self.ticker
    }

    fn get_price(&self) -> Option<&Decimal> {
        None
    }

    fn get_change(&self) -> Option<&Decimal> {
        None
    }

    fn get_quantity(&self) -> u32 {
        self.quantity
    }

    fn get_total(&self) -> Option<&Decimal> {
        None
    }

    fn get_currency(&self) -> Option<&String> {
        None
    }

    fn write_line(&self, writer: &mut csv::Writer<File>) -> Result<(), csv::Error> {
        writer.write_byte_record(&ByteRecord::from(&[&self.ticker, "", "", self.quantity.to_string().as_str(), "", ""][..]))
    }
}
