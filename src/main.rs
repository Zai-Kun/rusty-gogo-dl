mod anime_downloader;
use anime_downloader::gogo::{AnimeDetailedInfo, GogoAnime};
use console::style;
use error_stack::{Context, Report, ResultExt};
use inquire::{
    ui::{Attributes, Color, RenderConfig, StyleSheet, Styled},
    validator::Validation,
    CustomType, Select, Text,
};
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::{error::Error, path::PathBuf};

use console::Term;

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
    preferred_res: String,
    concurrent_downloads: usize,
    download_folder: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    clear_screen();
    inquire::set_global_render_config(get_render_config());
    let config = parse_and_load_config()?;
    let gogo_anime = GogoAnime::new(
        &config.gogo_base_url,
        &config.fetch_ep_list_api,
        &config.password,
        config.registered_account_emails,
    );
    gogo_anime.init().await?;
    loop {
        let query = Text::new(&make_bold("Search an anime:"))
            .with_help_message("Press enter to exit")
            .prompt()?;
        if query.is_empty() {
            break;
        }
        let results = gogo_anime.search_anime(&query).await?;
        if results.len() == 0 {
            print_err("No results found, please try again");
            continue;
        }

        let selected_anime = Select::new("Select an anime:", results)
            .with_page_size(10)
            .prompt()?;
        let detailed_anime_info = gogo_anime
            .fetch_detailed_anime_info(&selected_anime.url)
            .await?;
        clear_screen();
        print_details(&detailed_anime_info);
        let (start, end) = get_ep_start_and_ep_end(&detailed_anime_info);
    }
    Ok(())
}

fn print_details(anime: &AnimeDetailedInfo) {
    println!("{}{}", make_bold("Name: "), anime.name);
    println!("{}{}", make_bold("Thumbnail: "), anime.thumbnail);
    println!("{}", make_bold("About:"));
    for (key, value) in &anime.about {
        println!("  {}: {}", make_bold(key), value);
    }
    println!(
        "{}{}",
        make_bold("Number of episodes: "),
        anime.episode_links.len()
    );
}

fn get_ep_start_and_ep_end(anime: &AnimeDetailedInfo) -> (usize, usize) {
    let eps_len = anime.episode_links.len();
    let ep_start: usize = CustomType::new(&make_bold("Ending episode index:"))
        .with_default(1)
        .with_help_message("Enter the starting index from which to begin downloading episodes.")
        .with_validator(move |i: &usize| {
            if *i <= eps_len && *i != 0 {
                Ok(Validation::Valid)
            } else {
                Ok(Validation::Invalid(
                    format!(
                        "The starting index cannot be greater than the total number of episodes ({})",
                        eps_len
                    )
                    .into(),
                ))
            }
        })
        .prompt()
        .unwrap();

    let ep_end: usize = CustomType::new(&make_bold("Ending episode index:"))
        .with_default(eps_len)
        .with_help_message("Enter the ending index up to which to download episodes.")
        .with_validator(move |i: &usize| {
            if *i <= eps_len && *i >= ep_start {
                Ok(Validation::Valid)
            } else if *i < ep_start {
                Ok(Validation::Invalid(
                    "The ending index cannot be less than the starting index.".into(),
                ))
            } else {
                Ok(Validation::Invalid(
                    format!(
                        "The ending index cannot be greater than the total number of episodes ({})",
                        eps_len
                    )
                    .into(),
                ))
            }
        })
        .prompt()
        .unwrap();
    (ep_start, ep_end)
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

    if !config.download_folder.is_dir() {
        Err(Report::new(ParseConfigError).attach_printable(format!(
            "{} is not a valid path to a folder. Change the 'download_folder' path in the {} to a valid path to a folder.",
            config.download_folder.display(),
            config_path.display()
        )))?
    }

    Ok(config)
}

fn clear_screen() {
    let term = Term::stdout();
    term.clear_screen().unwrap();
}

fn print_err(msg: &str) {
    println!("{}", style(format!("❌{}", msg)).red());
}

fn make_bold(text: &str) -> String {
    style(text).bold().to_string()
}

fn get_render_config() -> RenderConfig<'static> {
    let mut render_config = RenderConfig::default();
    render_config.answered_prompt_prefix = Styled::new(">").with_fg(Color::DarkRed);
    render_config.prompt_prefix = Styled::new("?").with_fg(Color::DarkRed);
    render_config.highlighted_option_prefix = Styled::new("➤").with_fg(Color::DarkRed);
    render_config.selected_option = Some(StyleSheet::new().with_fg(Color::LightYellow));
    render_config.scroll_up_prefix = Styled::new("⬆").with_fg(Color::DarkRed);
    render_config.scroll_down_prefix = Styled::new("⬇").with_fg(Color::DarkRed);
    render_config.error_message = render_config
        .error_message
        .with_prefix(Styled::new("❌").with_fg(Color::DarkRed));

    render_config.answer = StyleSheet::new()
        .with_attr(Attributes::ITALIC)
        .with_fg(Color::LightGreen);

    render_config
}
