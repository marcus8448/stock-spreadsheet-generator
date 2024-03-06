use rust_decimal::Decimal;

#[derive(thiserror::Error, Debug)]
pub enum YahooError {
    #[error("failed to connect to yahoo api: {0}")]
    ConnectionError(#[from] reqwest::Error),
    #[error("yahoo responded with http code: {0}")]
    HttpError(reqwest::StatusCode),
    #[error("failed to deserialize json: {0}")]
    InvalidResponse(&'static str),
    #[error("failed to deserialize json: {0}")]
    MalformedJson(#[from] serde_json::Error),
}

pub fn request_quote(
    client: &mut reqwest::blocking::Client,
    ticker: &str,
) -> Result<Quote, YahooError> {
    let response = client
        .get(format!("https://query1.finance.yahoo.com/v8/finance/chart/{ticker}?symbol={ticker}&interval=1m&range=1m", ticker = ticker))
        .header("Accept", "application/json")
        .send()?;

    if !response.status().is_success() {
        return Err(YahooError::HttpError(response.status()));
    }

    let string = response.text().unwrap();
    let mut response: YahooResponse = serde_json::from_str(&string)?;
    if response.chart.result.is_empty() {
        return Err(YahooError::InvalidResponse("No data provided"));
    }
    let quote = response.chart.result.remove(0).meta;

    assert_eq!(ticker, quote.symbol.as_str());

    Ok(Quote {
        price: quote.regular_market_price,
        change: quote.regular_market_price
            - quote.previous_close.or(quote.chart_previous_close).unwrap(),
        currency: quote.currency,
    })
}

#[derive(serde::Deserialize)]
struct YahooResponse {
    chart: YahooChart,
}

#[derive(serde::Deserialize)]
struct YahooChart {
    result: Vec<YahooQuote>,
}

#[derive(serde::Deserialize)]
struct YahooQuote {
    meta: YahooMeta,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct YahooMeta {
    symbol: String,
    currency: String,
    previous_close: Option<Decimal>,
    chart_previous_close: Option<Decimal>,
    regular_market_price: Decimal,
}

pub struct Quote {
    pub price: Decimal,
    pub change: Decimal,
    pub currency: String,
}
