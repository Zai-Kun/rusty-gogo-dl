mod anime_downloader;
use anime_downloader::gogo::GogoAnime;
use error_stack::{Context, Report, ResultExt};
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Debug)]
struct ParseConfigError;

impl fmt::Display for ParseConfigError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.write_str("Could not parse configuration file")
    }
}

impl Context for ParseConfigError {}

#[derive(serde::Deserialize, Debug)]
struct Config {
    gogo_base_url: String,
    fetch_ep_list_api: String,
    registered_account_emails: Vec<String>,
    password: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config = parse_and_load_config()?;
    let gogo_anime = GogoAnime::new(
        &config.gogo_base_url,
        &config.fetch_ep_list_api,
        &config.password,
        config.registered_account_emails,
    );
    gogo_anime.init().await?;
    let anime = &gogo_anime.search_anime("death note").await?[0];
    println!(
        "{:?}",
        gogo_anime.fetch_detailed_anime_info(&anime.url).await?
    );
    Ok(())
}

fn parse_and_load_config() -> Result<Config, Report<ParseConfigError>> {
    let config_path = Path::new("config.json");
    let mut config_file = File::open(config_path)
        .change_context(ParseConfigError)
        .attach_printable_lazy(|| {
            format!("failed to open the config file {}", config_path.display())
        })?;

    let mut contents = String::new();
    config_file
        .read_to_string(&mut contents)
        .change_context(ParseConfigError)
        .attach_printable(format!(
            "failed to read the config file {}",
            config_path.display()
        ))?;
    let config: Config = serde_json::from_str(&contents)
        .change_context(ParseConfigError)
        .attach_printable(format!(
            "failed to parse the config file as json {}",
            config_path.display()
        ))?;
    Ok(config)
}
