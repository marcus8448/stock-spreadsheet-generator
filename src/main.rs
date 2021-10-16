extern crate chrono;
extern crate clap;
extern crate prettytable;
extern crate yahoo_finance_api;

use clap::{App, Arg};
use futures::executor::block_on;
use yahoo_finance_api::YahooConnector;

fn main() {
    let matches = App::new("Stock Spreadsheet Generator")
        .version("0.1.0")
        .author("marcus8448")
        .about("Creates a simple spreadsheet based on yahoo finance data")

        .arg(Arg::with_name("file")
            .short("f")
            .long("file")
            .takes_value(true)
            .help("the csv file to output data to")
            .default_value("output.csv")
        )
        .arg(Arg::with_name("disable-file")
            .short("d")
            .long("disable-file")
            .help("disables csv file output")
        )
        .arg(Arg::with_name("disable-console-output")
            .short("c")
            .long("disable-console-output")
            .help("disables console table output")
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
        .arg(Arg::with_name("tickers")
            .default_value("GOOG")
            .multiple(true)
            .min_values(1)
            .require_delimiter(false)
            .takes_value(true)
        )
        .get_matches();

    let provider = YahooConnector::new();
    let file = matches.value_of("file").unwrap();
    let wait_time_u64 = matches.value_of("wait-time").unwrap().parse::<u64>().unwrap();
    let wait_time = std::time::Duration::from_secs(wait_time_u64);
    let one_sec = std::time::Duration::from_secs(1);
    let disable_file = matches.is_present("disable-file");
    let console_output = !matches.is_present("disable-console-output");

    loop {
        println!("Query time: {}", chrono::prelude::Local::now());
        let tickers_str = matches.values_of("tickers").unwrap();
        let mut output: String = String::new();
        output.push_str("Ticker,Value,Volume,Total\n");

        for t in tickers_str {
            println!("{}", t);
            let out = deserialize_yahoo(&provider, t);
            output.push_str(out.as_str());
            output.push('\n');
        }

        if !disable_file {
            let result = std::fs::write(file, output.as_str());
            if result.is_err() {
                println!("Warning: unable to write csv - {:?}", result.err().unwrap());
            }
        }

        if console_output {
            let mut table = prettytable::Table::new();
            let mut reader = prettytable::csv::ReaderBuilder::new().has_headers(false).flexible(true).from_reader(output.as_str().as_bytes());
            for record in reader.records() {
                let mut row = prettytable::Row::empty();
                for v in record.unwrap().iter() {
                    row.add_cell(prettytable::Cell::new(v));
                }
                table.add_row(row);
            }
            table.printstd();
        }

        if matches.is_present("run_once") {
            break;
        }
        let completion = std::time::Instant::now();
        let bar = indicatif::ProgressBar::new(wait_time_u64);
        while std::time::Instant::now().duration_since(completion) < wait_time {
            std::thread::sleep(one_sec);
            bar.inc(1);
        }
        bar.finish_and_clear();
    }
}

fn deserialize_yahoo(provider: &YahooConnector, t: &str) -> String {
    let future = provider.get_latest_quotes(t, "1m");
    let result = block_on(future);
    let data = match result {
        Ok(data) => data,
        Err(error) => panic!("Warning: Problem communicating with Yahoo! API: {:?}", error)
    };
    let quote = match data.last_quote() {
        Ok(quote) => quote,
        Err(error) => panic!("Warning: Problem deserializing last quote: {:?}", error)
    };

    format!("{},{:.2},{},{:.2}", t, quote.close, quote.volume, quote.close * (quote.volume as f64))
}
