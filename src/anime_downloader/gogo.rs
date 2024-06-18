#![allow(dead_code)]

use super::errors::*;
use error_stack::{Report, ResultExt};
use rand::seq::SliceRandom;
use rand::thread_rng;
use reqwest;
use reqwest::cookie::{CookieStore, Jar};
use reqwest::header::HeaderValue;
use scraper::{Html, Selector};
use std::{collections::HashMap, sync::Arc};
use url::Url;

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
            let response = self
                .client
                .get(&login_url)
                .send()
                .await
                .change_context(GogoInitError)
                .attach_printable(format!("Failed to send a request to {}", login_url))?;

            if !response.status().is_success() {
                return Err(Report::new(GogoInitError).attach_printable(format!(
                    "Request to {} failed with status code {}. Expected return code 200.",
                    login_url,
                    response.status()
                )));
            }

            let page_content = response
                .text()
                .await
                .change_context(GogoInitError)
                .attach_printable(format!("Failed to fetch the text from {}", login_url))?;

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
                .change_context(GogoInitError)
                .attach_printable(format!("Failed to send a request to {}", login_url))?;

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

    pub async fn test(&self) -> Result<(), Report<GogoInitError>> {
        println!(
            "{}",
            self.client
                .get("https://anitaku.so/fairy-tail-episode-1")
                .send()
                .await
                .expect("lmao")
                .text()
                .await
                .expect("lmao2")
        );
        Ok(())
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
