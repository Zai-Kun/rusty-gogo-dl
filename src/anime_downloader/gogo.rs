#![allow(dead_code)]

use super::gogo_errors::*;
use error_stack::{Report, ResultExt};
use rand::seq::SliceRandom;
use rand::thread_rng;
use reqwest::cookie::{CookieStore, Jar};
use reqwest::header::HeaderValue;
use reqwest::{self, Error};
use scraper::selectable::Selectable;
use scraper::{Html, Selector};
use std::fmt;
use std::{collections::HashMap, sync::Arc};
use url::Url;
use urlencoding::encode;

#[derive(Debug)]
pub struct Anime {
    pub name: String,
    pub released: String,
    pub thumbnail: String,
    pub url: String,
}

impl fmt::Display for Anime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl Anime {
    fn new(name: &str, released: &str, thumbnail: &str, url: &str) -> Self {
        Self {
            name: name.to_string(),
            released: released.to_string(),
            thumbnail: thumbnail.to_string(),
            url: url.to_string(),
        }
    }
}

#[derive(Debug)]
pub struct AnimeDetailedInfo {
    pub name: String,
    pub thumbnail: String,
    pub about: HashMap<String, String>,
    pub episode_links: Vec<String>,
}

impl fmt::Display for AnimeDetailedInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Name: {}", self.name)?;
        writeln!(f, "Thumbnail: {}", self.thumbnail)?;
        writeln!(f, "About:")?;
        for (key, value) in &self.about {
            writeln!(f, "  {}: {}", key, value)?;
        }
        writeln!(f, "Number of episodes: {}", self.episode_links.len())
    }
}

impl AnimeDetailedInfo {
    fn new(
        name: &str,
        thumbnail: &str,
        about: &HashMap<String, String>,
        episode_links: &Vec<String>,
    ) -> Self {
        Self {
            name: name.to_string(),
            thumbnail: thumbnail.to_string(),
            about: about.to_owned(),
            episode_links: episode_links.to_owned(),
        }
    }
}

#[derive(Debug)]
pub struct GogoAnime {
    client: reqwest::Client,
    gogo_base_url: String,
    fetch_ep_list_api: String,
    password: String,
    registered_account_emails: Vec<String>,
    cookie_store: Arc<Jar>,
}

impl GogoAnime {
    pub fn new(
        gogo_base_url: &str,
        fetch_ep_list_api: &str,
        password: &str,
        registered_account_emails: Vec<String>,
    ) -> Self {
        let cookie_store = Arc::new(Jar::default());
        let client = reqwest::Client::builder()
            .cookie_provider(cookie_store.clone())
            .build()
            .unwrap();
        Self {
            client,
            gogo_base_url: gogo_base_url.to_string(),
            fetch_ep_list_api: fetch_ep_list_api.to_string(),
            password: password.to_string(),
            registered_account_emails,
            cookie_store,
        }
    }
    pub async fn init(&self) -> Result<(), Report<GogoInitError>> {
        let login_url = format!("{}/login.html", self.gogo_base_url);

        loop {
            let page_content = self
                .fetch_content(&login_url)
                .await
                .change_context(GogoInitError)?;

            let document = Html::parse_document(&page_content);

            let selector =
                Selector::parse(".form-login > form:nth-child(3) > input:nth-child(1)").unwrap();

            let csrf_token = document
                .select(&selector)
                .next()
                .and_then(|node| node.attr("value"))
                .ok_or_else(|| {
                    Report::new(GogoInitError)
                        .attach_printable(format!("CSRF token not found in {}", login_url))
                })?;

            let mut rng = thread_rng();
            let email = self.registered_account_emails.choose(&mut rng).unwrap();
            let mut params = HashMap::new();
            params.insert("email", email.as_str());
            params.insert("password", self.password.as_str());
            params.insert("_csrf", csrf_token);

            self.client
                .post(&login_url)
                .form(&params)
                .send()
                .await
                .change_context(GogoInitError)?;

            let cookies = if let Some(c) = self
                .cookie_store
                .cookies(&Url::parse(&self.gogo_base_url).unwrap())
            {
                c
            } else {
                continue;
            };

            if cookie_in(&cookies, "auth") {
                break;
            }
        }

        Ok(())
    }

    pub async fn search_anime(
        &self,
        query: &str,
    ) -> Result<Vec<Anime>, Box<dyn std::error::Error>> {
        let search_url = format!(
            "{}/search.html?keyword={}",
            self.gogo_base_url,
            encode(query)
        );
        // Selectors
        let ul_selector_str = "ul.items";
        let li_selector_str = "li";
        let name_selector_str = "p.name a";
        let released_selector_str = "p.released";
        let thumbnail_selector_str = "div.img a img";
        let url_selector_str = "div.img a";

        let ul_selector = Selector::parse(ul_selector_str).unwrap();
        let li_selector = Selector::parse(li_selector_str).unwrap();
        let name_selector = Selector::parse(name_selector_str).unwrap();
        let released_selector = Selector::parse(released_selector_str).unwrap();
        let thumbnail_selector = Selector::parse(thumbnail_selector_str).unwrap();
        let url_selector = Selector::parse(url_selector_str).unwrap();

        let page_content = self.fetch_content(&search_url).await?;
        let document = Html::parse_document(&page_content);

        let ul_items = document.select(&ul_selector).next().ok_or_else(|| {
            Report::new(GogoInitError)
                .attach_printable(format!("{} not found in {}", ul_selector_str, search_url))
        })?;
        let mut search_results: Vec<Anime> = Vec::new();
        for li_element in ul_items.select(&li_selector) {
            let anime_name = li_element
                .select(&name_selector)
                .next()
                .ok_or_else(|| {
                    Report::new(GogoSearchFailedError).attach_printable(format!(
                        "Failed to locate anime name with {}",
                        name_selector_str
                    ))
                })?
                .text()
                .collect::<Vec<_>>()
                .concat();
            let released = li_element
                .select(&released_selector)
                .next()
                .ok_or_else(|| {
                    Report::new(GogoSearchFailedError).attach_printable(format!(
                        "Failed to locate anime release date with {}",
                        released_selector_str
                    ))
                })?
                .text()
                .collect::<Vec<_>>()
                .concat();
            let released = released.trim().to_string();
            let thumbnail = li_element
                .select(&thumbnail_selector)
                .next()
                .ok_or_else(|| {
                    Report::new(GogoSearchFailedError).attach_printable(format!(
                        "Failed to locate anime thumbnail with {}",
                        thumbnail_selector_str
                    ))
                })?
                .attr("src")
                .unwrap();
            let anime_url = li_element
                .select(&url_selector)
                .next()
                .ok_or_else(|| {
                    Report::new(GogoSearchFailedError).attach_printable(format!(
                        "Failed to locate anime url with {}",
                        url_selector_str
                    ))
                })?
                .attr("href")
                .unwrap();

            let anime = Anime::new(
                &anime_name,
                released
                    .split_once(": ")
                    .unwrap_or(("", released.as_str()))
                    .1,
                &thumbnail,
                &format!("{}{}", self.gogo_base_url, anime_url),
            );
            search_results.push(anime);
        }
        Ok(search_results)
    }

    async fn fetch_content(&self, url: &str) -> Result<String, Report<Error>> {
        let response = self.client.get(url).send().await?;

        Ok(response.text().await?)
    }

    pub async fn fetch_detailed_anime_info(
        &self,
        anime_url: &str,
    ) -> Result<AnimeDetailedInfo, Report<GogoFetchingDetailsFailed>> {
        let page_content = self
            .fetch_content(anime_url)
            .await
            .change_context(GogoFetchingDetailsFailed)?;
        let document = Html::parse_document(&page_content);

        let anime_info_body_selector_str = "div.anime_info_body";
        let anime_thumbnail_selector_str = "img";
        let anime_name_selector_str = "h1";
        let p_selector_str = "p.type";
        let description_selector_str = ".description";
        let total_eps_selector_str = ".active";
        let movie_id_selector_str = "input#movie_id";

        let anime_info_body_selector = Selector::parse(anime_info_body_selector_str).unwrap();
        let anime_thumbnail_selector = Selector::parse(anime_thumbnail_selector_str).unwrap();
        let anime_name_selector = Selector::parse(anime_name_selector_str).unwrap();
        let p_selector = Selector::parse(p_selector_str).unwrap();
        let description_selector = Selector::parse(description_selector_str).unwrap();
        let end_ep_selector = Selector::parse(total_eps_selector_str).unwrap();
        let movie_id_selector = Selector::parse(movie_id_selector_str).unwrap();

        let failed_to_locate_msg = |target: &str, selector: &str| -> String {
            format!(
                "Failed to locate {} with {} from {}",
                target, selector, anime_url
            )
        };
        let anime_info_body = document
            .select(&anime_info_body_selector)
            .next()
            .ok_or_else(|| {
                Report::new(GogoFetchingDetailsFailed).attach_printable(failed_to_locate_msg(
                    "anime_info_body",
                    anime_info_body_selector_str,
                ))
            })?;

        let thumbnail_url = anime_info_body
            .select(&anime_thumbnail_selector)
            .next()
            .ok_or_else(|| {
                Report::new(GogoFetchingDetailsFailed).attach_printable(failed_to_locate_msg(
                    "thumbnail_url",
                    anime_thumbnail_selector_str,
                ))
            })?
            .value()
            .attr("src")
            .unwrap()
            .to_string();
        let title = anime_info_body
            .select(&anime_name_selector)
            .next()
            .ok_or_else(|| {
                Report::new(GogoFetchingDetailsFailed)
                    .attach_printable(failed_to_locate_msg("anime title", anime_name_selector_str))
            })?
            .inner_html();
        let mut about_anime: HashMap<String, String> = HashMap::new();

        for p_tag in anime_info_body.select(&p_selector) {
            let raw_text = p_tag.text().collect::<Vec<_>>().concat().to_string();

            let (key, value) = raw_text.split_once(":").unwrap();
            let (key, mut value) = (
                key.to_lowercase().replace(" ", "_").to_string(),
                value.trim().to_string(),
            );

            if key == "plot_summary" {
                let raw_desc = anime_info_body
                    .select(&description_selector)
                    .next()
                    .ok_or_else(|| {
                        Report::new(GogoFetchingDetailsFailed).attach_printable(
                            failed_to_locate_msg("description", &description_selector_str),
                        )
                    })?;
                value = raw_desc
                    .text()
                    .collect::<Vec<_>>()
                    .concat()
                    .replace("\n\n ", "\n\n")
                    .to_string();
            }

            about_anime.insert(key, value);
        }
        let end_ep = document
            .select(&end_ep_selector)
            .next()
            .ok_or_else(|| {
                Report::new(GogoFetchingDetailsFailed)
                    .attach_printable(failed_to_locate_msg("total eps", total_eps_selector_str))
            })?
            .attr("ep_end")
            .unwrap();
        let anime_id = document
            .select(&movie_id_selector)
            .next()
            .unwrap()
            .attr("value")
            .unwrap();
        let episode_links = self
            .fetch_anime_ep_links(anime_id, end_ep)
            .await
            .change_context(GogoFetchingDetailsFailed)?;

        Ok(AnimeDetailedInfo::new(
            &title,
            &thumbnail_url,
            &about_anime,
            &episode_links,
        ))
    }
    async fn fetch_anime_ep_links(
        &self,
        anime_id: &str,
        end_ep: &str,
    ) -> Result<Vec<String>, Report<Error>> {
        let url = self
            .fetch_ep_list_api
            .replace("{END_EP}", end_ep)
            .replace("{ANIME_ID}", anime_id);
        let page_content = self.fetch_content(&url).await?;
        let document = Html::parse_document(&page_content);
        let episodes_selector = Selector::parse("#episode_related").unwrap();
        let mut episodes = Vec::new();
        for li in document
            .select(&episodes_selector)
            .next()
            .unwrap()
            .select(&Selector::parse("li").unwrap())
            .collect::<Vec<_>>()
            .iter()
            .rev()
        {
            let url = format!(
                "{}{}",
                self.gogo_base_url,
                li.select(&Selector::parse("a").unwrap())
                    .next()
                    .unwrap()
                    .value()
                    .attr("href")
                    .unwrap()
                    .trim()
            );
            episodes.push(url);
        }

        Ok(episodes)
    }
    pub async fn fetch_ep_download_links(
        &self,
        anime_url: &str,
    ) -> Result<HashMap<String, String>, Report<GogoFailedToFetchDownloadLinks>> {
        let page_content = self
            .fetch_content(anime_url)
            .await
            .change_context(GogoFailedToFetchDownloadLinks)?;
        let document = Html::parse_document(&page_content);
        let cf_download_selector_str = ".cf-download";
        let cf_download_selector = Selector::parse(cf_download_selector_str).unwrap();
        let cf_download = document
            .select(&cf_download_selector)
            .next()
            .ok_or_else(|| {
                Report::new(GogoFailedToFetchDownloadLinks)
                    .attach_printable(format!(
                        "Download links not find using {} from {}",
                        cf_download_selector_str, anime_url
                    ))
                    .attach_printable("Did you initalize gogoanime?")
            })?;

        let mut download_links = HashMap::new();
        for a in cf_download.select(&Selector::parse("a").unwrap()) {
            let res = a.text().collect::<Vec<_>>().concat().trim().to_string();
            let link = a.value().attr("href").unwrap().to_string();

            download_links.insert(res, link);
        }

        Ok(download_links)
    }
}

fn cookie_in(header_value: &HeaderValue, cookie_name: &str) -> bool {
    let header_str = header_value.to_str().unwrap();

    let cookies: Vec<&str> = header_str.split("; ").collect();

    for cookie in cookies {
        if let Some(pos) = cookie.find('=') {
            let (name, _) = cookie.split_at(pos);
            if name == cookie_name {
                return true;
            }
        }
    }

    false
}
