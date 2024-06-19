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
use std::{collections::HashMap, sync::Arc};
use url::Url;
use urlencoding::encode;

#[derive(Debug)]
pub struct Anime {
    name: String,
    released: String,
    thumbnail: String,
    url: String,
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
                .get_content(&login_url)
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

        let page_content = self.get_content(&search_url).await?;
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

    async fn get_content(&self, url: &str) -> Result<String, Report<Error>> {
        let response = self.client.get(url).send().await?;

        Ok(response.text().await?)
    }
}

fn format_str<S: AsRef<str> + std::fmt::Display>(mut text: &str, args: &Vec<S>) -> String {
    let mut new = String::new();
    let mut idx = 0;
    loop {
        if let Some(index) = text.find("{}") {
            if idx >= args.len() {
                new.push_str(text);
                break;
            }
            let before = &text[..index];
            let after = &text[index + 2..];
            new.push_str(&format!("{}{}", before, args[idx]));
            text = after;
        } else {
            break;
        }
        idx += 1;
    }
    new
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
