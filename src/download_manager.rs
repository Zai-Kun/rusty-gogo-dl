#![allow(dead_code)]

use error_stack::{Context, Report, ResultExt};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use reqwest::Client;
use std::cmp::min;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fmt;
use std::path::Path;
use std::sync::Arc;
use tokio::{
    fs::{create_dir_all, File, OpenOptions},
    io::AsyncWriteExt,
};
use tokio::{
    sync::Semaphore,
    task::{self, JoinHandle},
};
use tokio_stream::StreamExt;

use super::utils;
use crate::anime_downloader::gogo::GogoAnime;

#[derive(Debug)]
pub struct DownloadError;

impl fmt::Display for DownloadError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.write_str("Error while downloading the file")
    }
}

impl Context for DownloadError {}

#[derive(Debug)]
pub struct ConcurrentDownloadManager {
    sem: Arc<Semaphore>,
    client: Client,
    multi_progress: MultiProgress,
    retries: usize,
    pub tasks_results: HashMap<String, JoinHandle<Result<(), Report<DownloadError>>>>,
}

impl ConcurrentDownloadManager {
    pub fn new(concurrent_downloads: usize, retries: usize) -> Self {
        let client = reqwest::Client::builder().build().unwrap();
        let sem = Arc::new(Semaphore::new(concurrent_downloads));
        let multi_progress = MultiProgress::new();
        let tasks_results: HashMap<String, JoinHandle<Result<(), Report<DownloadError>>>> =
            HashMap::new();

        Self {
            sem,
            client,
            multi_progress,
            retries,
            tasks_results,
        }
    }

    pub fn add_gogo_download(
        &mut self,
        gogo_anime: Arc<GogoAnime>,
        pref_res: &str,
        ep_path: &str,
        ep_url: &str,
    ) {
        let pb = self.multi_progress.add(ProgressBar::new(0));
        pb.set_style(ProgressStyle::with_template("{msg} {spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
            .unwrap()
            .progress_chars("#>-"));
        let task = task::spawn(gogo_downloader_task(
            gogo_anime,
            pref_res.to_string(),
            self.retries,
            self.client.clone(),
            ep_path.to_string(),
            ep_url.to_string(),
            self.sem.clone(),
            pb,
        ));
        self.tasks_results.insert(ep_path.to_string(), task);
    }

    pub async fn await_results(&mut self) -> HashMap<String, Result<(), Report<DownloadError>>> {
        let mut results = HashMap::new();
        for (path, task) in self.tasks_results.drain() {
            let result = task.await.unwrap();
            results.insert(path, result);
        }
        results
    }
}

async fn gogo_downloader_task(
    gogo_anime: Arc<GogoAnime>,
    pref_res: String,
    mut retries: usize,
    client: Client,
    ep_path: String,
    ep_url: String,
    sem: Arc<Semaphore>,
    pb: ProgressBar,
) -> Result<(), Report<DownloadError>> {
    let _permit = sem.acquire().await.unwrap();
    loop {
        let download_links = match gogo_anime
            .fetch_ep_download_links(&ep_url)
            .await
            .change_context(DownloadError)
        {
            Ok(ok) => ok,
            Err(_) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => return Err(err),
        };
        let resolutions: Vec<&String> = download_links.keys().collect();
        let closest_res = utils::closest_resolution(&resolutions[..], &pref_res);
        match file_downloader_task(
            &client,
            &download_links.get(&closest_res).unwrap().to_string(),
            &ep_path,
            &pb,
        )
        .await
        {
            Ok(_) => break,
            Err(_) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(err) => return Err(err),
        };
    }
    Ok(())
}

async fn file_downloader_task(
    client: &Client,
    url: &str,
    path: &str,
    pb: &ProgressBar,
) -> Result<(), Report<DownloadError>> {
    let file_path = Path::new(&path);
    if let Some(parent) = file_path.parent() {
        create_dir_all(parent)
            .await
            .change_context(DownloadError)
            .attach_printable(format!("Error while directoire(s) {}", parent.display()))?;
    }

    let mut file = if file_path.exists() && file_path.is_file() {
        OpenOptions::new()
            .append(true)
            .open(&file_path)
            .await
            .change_context(DownloadError)
            .attach_printable(format!("Error while opening file {}", file_path.display()))?
    } else {
        File::create(&file_path)
            .await
            .change_context(DownloadError)
            .attach_printable(format!("Error while creating file {}", file_path.display()))?
    };

    let file_size = file
        .metadata()
        .await
        .change_context(DownloadError)
        .attach_printable(format!(
            "Error while trying to extract size of the {} file",
            file_path.display()
        ))?
        .len();

    let head_response = client
        .head(url)
        .send()
        .await
        .change_context(DownloadError)
        .attach_printable(format!("Error while sendiing a head request to {}", url))?;
    let content_length = head_response
        .headers()
        .get("Content-Length")
        .ok_or_else(|| Report::new(DownloadError))
        .attach_printable(format!("Content-Length header not found in {}", url))?
        .to_str()
        .map_err(|_| Report::new(DownloadError))?
        .parse::<u64>()
        .map_err(|_| Report::new(DownloadError))?;
    pb.set_length(content_length);

    if file_size >= content_length {
        return Ok(());
    }

    let request = client
        .get(url)
        .header("Range", format!("bytes={}-", file_size));
    let mut stream = request
        .send()
        .await
        .change_context(DownloadError)
        .attach_printable(format!("Error while sending a get request to {}", url))?
        .bytes_stream();

    let mut total_bytes = file_size;
    let file_name = OsStr::to_str(file_path.file_name().unwrap())
        .unwrap_or("Unkown")
        .to_string();

    pb.set_message(file_name.clone());
    pb.set_message(file_name.clone());
    pb.set_position(total_bytes);
    pb.reset_eta();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result
            .change_context(DownloadError)
            .attach_printable("Error occurred while processing a chunk of data")?;
        let new = min(total_bytes + chunk.len() as u64, content_length);
        total_bytes = new;
        pb.set_position(total_bytes);
        file.write_all(&chunk)
            .await
            .change_context(DownloadError)
            .attach_printable(format!(
                "Failed to write buffer to file {}",
                file_path.display()
            ))?;
    }

    file.flush()
        .await
        .change_context(DownloadError)
        .attach_printable(format!("Failed to flush file {}", file_path.display()))?;
    pb.finish_with_message(format!("Downloaded {}", file_name));
    Ok(())
}
