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
    client: Arc<Client>,
    multi_progress: MultiProgress,
    pub tasks_results: HashMap<String, JoinHandle<Result<(), Report<DownloadError>>>>,
}

impl ConcurrentDownloadManager {
    pub fn new(concurrent_downloads: usize) -> Self {
        let client = Arc::new(reqwest::Client::builder().build().unwrap());
        let sem = Arc::new(Semaphore::new(concurrent_downloads));
        let multi_progress = MultiProgress::new();
        let tasks_results: HashMap<String, JoinHandle<Result<(), Report<DownloadError>>>> =
            HashMap::new();

        Self {
            sem,
            client,
            multi_progress,
            tasks_results,
        }
    }

    pub fn add_download(&mut self, url: &str, path: &str) {
        let pb = self.multi_progress.add(ProgressBar::new(0));
        pb.set_style(ProgressStyle::with_template("{msg} {spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
            .unwrap()
            .progress_chars("#>-"));
        let task = task::spawn(file_downloader_task(
            self.client.clone(),
            url.to_string(),
            path.to_string(),
            self.sem.clone(),
            pb,
        ));
        self.tasks_results.insert(path.to_string(), task);
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

async fn file_downloader_task(
    client: Arc<Client>,
    url: String,
    path: String,
    sem: Arc<Semaphore>,
    pb: ProgressBar,
) -> Result<(), Report<DownloadError>> {
    let _permit = sem.acquire().await.unwrap();

    let file_path = Path::new(&path);
    if let Some(parent) = file_path.parent() {
        create_dir_all(parent).await.change_context(DownloadError)?;
    }

    let mut file = if file_path.exists() && file_path.is_file() {
        OpenOptions::new()
            .append(true)
            .open(&file_path)
            .await
            .change_context(DownloadError)?
    } else {
        File::create(&file_path)
            .await
            .change_context(DownloadError)?
    };

    let file_size = file.metadata().await.change_context(DownloadError)?.len();

    let head_response = client
        .head(&url)
        .send()
        .await
        .change_context(DownloadError)?;
    let content_length = head_response
        .headers()
        .get("Content-Length")
        .ok_or_else(|| Report::new(DownloadError))?
        .to_str()
        .map_err(|_| Report::new(DownloadError))?
        .parse::<u64>()
        .map_err(|_| Report::new(DownloadError))?;
    pb.set_length(content_length);

    if file_size >= content_length {
        return Ok(());
    }

    let request = client
        .get(&url)
        .header("Range", format!("bytes={}-", file_size));
    let mut stream = request
        .send()
        .await
        .change_context(DownloadError)?
        .bytes_stream();

    let mut total_bytes = file_size;
    let file_name = OsStr::to_str(file_path.file_name().unwrap())
        .unwrap_or("Unkown")
        .to_string();

    pb.set_message(file_name.clone());

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.change_context(DownloadError)?;
        let new = min(total_bytes + chunk.len() as u64, content_length);
        total_bytes = new;
        pb.set_position(total_bytes);
        file.write_all(&chunk).await.change_context(DownloadError)?;
    }

    file.flush().await.change_context(DownloadError)?;
    pb.finish_with_message(format!("Downloaded {}", file_name));
    Ok(())
}
