use log::warn;

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct Config {
    pub(crate) output_file: Option<String>,
    pub(crate) tickers: Vec<Ticker>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct Ticker {
    pub(crate) id: String,
    #[serde(alias = "volume")]
    pub(crate) quantity: Option<u32>,
}

pub(crate) fn load_from_file<P: AsRef<std::path::Path> + AsRef<std::ffi::OsStr>>(
    path: P,
) -> Config {
    if !std::path::Path::new(&path).exists() {
        let default_config = Config {
            output_file: Some(String::from("output.csv")),
            tickers: vec![
                Ticker {
                    id: String::from("GOOG"),
                    quantity: None,
                },
                Ticker {
                    id: String::from("MSFT"),
                    quantity: Some(7),
                },
            ],
        };

        warn!("No config file found... creating the default one.");
        std::fs::write(
            &path,
            toml::to_string(&default_config).expect("Failed to serialize default config"),
        )
        .expect("Failed to write default config file!");
    }
    let config_data = std::fs::read_to_string(&path).expect("Failed to read configuration file!");
    toml::from_str(&config_data).expect("Failed to parse configuration file!")
}
