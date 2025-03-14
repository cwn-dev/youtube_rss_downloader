use reqwest::Client;
use roxmltree::Document;
use std::collections::HashSet;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;
use url::Url;
use tokio::io::{AsyncBufReadExt, BufReader};
use glob::glob;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    delete_part_files("/home/craig/youtube/videos/")?;

    // 1. Define an array of YouTube RSS feed URLs.
    let rss_feeds = vec![
        "https://www.youtube.com/feeds/videos.xml?channel_id=UCE_M8A5yxnLfW0KghEeajjw",
    ];

    // Use a HashSet to avoid duplicate video URLs.
    let mut video_urls: HashSet<String> = HashSet::new();
    let client = Client::new();

    for feed_url in rss_feeds {
        println!("Processing feed: {}", feed_url);
        let response = client.get(feed_url).send().await?;
        let rss_content = response.text().await?;
        let doc = Document::parse(&rss_content)?;
        let atom_ns = "http://www.w3.org/2005/Atom";
        let yt_ns = "http://www.youtube.com/xml/schemas/2015";

        // Find all <entry> elements in the Atom namespace.
        let entries: Vec<_> = doc
            .descendants()
            .filter(|node| node.tag_name().name() == "entry" && node.tag_name().namespace() == Some(atom_ns))
            .collect();
        println!("Found {} <entry> elements", entries.len());

        for entry in entries {
            // Try to extract the video ID from the <yt:videoId> element.
            if let Some(video_id_node) = entry.descendants().find(|node| {
                node.tag_name().name() == "videoId" && node.tag_name().namespace() == Some(yt_ns)
            }) {
                let video_id = video_id_node.text().unwrap_or("").trim().to_string();
                println!("Extracted videoId: '{}'", video_id);
                if !video_id.is_empty() {
                    let url = format!("https://www.youtube.com/watch?v={}", video_id);
                    video_urls.insert(url);
                    continue;
                }
            }
            // Fallback: use the <link> element with rel="alternate"
            if let Some(link_node) = entry.descendants().find(|node| {
                node.tag_name().name() == "link"
                    && node.tag_name().namespace() == Some(atom_ns)
                    && node.attribute("rel") == Some("alternate")
            }) {
                if let Some(href) = link_node.attribute("href") {
                    if !href.trim().is_empty() {
                        video_urls.insert(href.to_string());
                    }
                }
            }
        }
    }

    println!("Found the following video URLs:");
    for url in &video_urls {
        println!("{}", url);
    }

    // 3. Load the archive file of already downloaded video IDs.
    let archive_file = "/home/craig/youtube/downloaded.txt";
    let mut downloaded_ids: HashSet<String> = HashSet::new();
    if Path::new(archive_file).exists() {
        let contents = fs::read_to_string(archive_file)?;
        for line in contents.lines() {
            if !line.trim().is_empty() {
                // Expecting format: "youtube VIDEO_ID"
                let tokens: Vec<&str> = line.split_whitespace().collect();
                if tokens.len() >= 2 {
                    downloaded_ids.insert(tokens[1].trim().to_string());
                }
            }
        }
    }

    // Specify the output directory for the downloaded videos.
    let output_directory = "/home/craig/youtube/videos";
    fs::create_dir_all(output_directory)?;
    let output_template = format!("{}/%(title)s.%(ext)s", output_directory);

    // Specify cookies option for age-restricted videos.
    let cookies_file = "/home/craig/youtube/cookies.txt";
    let ffmpeg_location = "/usr/bin";

    // Process each video URL.
    for video_url in &video_urls {
        // Use to seed downloaded.txt file
        // // Extract the video ID from the URL.
        // if let Some(video_id) = get_video_id(video_url) {
        //     if downloaded_ids.contains(&video_id) {
        //         //println!("Skipping {} (already downloaded).", video_url);
        //         continue;
        //     }
        // }

        // if let Some(video_id) = get_video_id(video_url) {
        //     println!("youtube {0}", video_id);
        // }

        // Extract the video ID from the URL.
        if let Some(video_id) = get_video_id(video_url) {
            if downloaded_ids.contains(&video_id) {
                println!("Skipping {} (already downloaded).", video_url);
                continue;
            }
        }

        println!("Downloading {} ...", video_url);

        // Build the process start info.
        let yt_dlp_executable = "/home/craig/youtube/yt-dlp_linux";
        let args = vec![
            "--download-archive", archive_file,
            "-f", "bestvideo[ext=mp4]+bestaudio[ext=m4a]/mp4",
            "--sleep-requests", "5",
            //"--min-sleep-interval", "60",
            //"--max-sleep-interval", "90",
            "--match-filters", "!is_live",
            "--ffmpeg-location", ffmpeg_location,
            "--cookies", cookies_file,
            "-o", &output_template,
            video_url,
        ];

        let mut cmd = Command::new(yt_dlp_executable);
        cmd.args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Spawn the process.
        let mut child = cmd.spawn()?;

        // Take stdout and stderr so we can read them asynchronously.
        let stdout = child.stdout.take().expect("failed to capture stdout");
        let stderr = child.stderr.take().expect("failed to capture stderr");

        // Create asynchronous readers for stdout and stderr.
        let stdout_reader = BufReader::new(stdout);
        let stderr_reader = BufReader::new(stderr);

        // Spawn tasks to read stdout and stderr concurrently.
        let stdout_task = tokio::spawn(async move {
            let mut lines = stdout_reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                println!("{}", line);
            }
        });

        let stderr_task = tokio::spawn(async move {
            let mut lines = stderr_reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                eprintln!("{}", line);
            }
        });

        // Wait for the process to complete.
        let status = child.wait().await?;
        // Ensure the output tasks finish.
        stdout_task.await?;
        stderr_task.await?;

        if !status.success() {
            println!("Error: yt-dlp exited with status: {:?}", status);
        } else {
            // If the download was successful, update our in-memory archive.
            if let Some(video_id) = get_video_id(video_url) {
                downloaded_ids.insert(video_id);
            }
        }
    }

    delete_part_files("/home/craig/youtube/videos/")?;

    Ok(())
}

/// Helper function to extract the YouTube video ID from a URL.
fn get_video_id(url: &str) -> Option<String> {
    if url.trim().is_empty() {
        return None;
    }
    match Url::parse(url) {
        Ok(parsed) => {
            if let Some(domain) = parsed.domain() {
                if domain.contains("youtu.be") {
                    return parsed
                        .path_segments()
                        .and_then(|mut segments| segments.next().map(|s| s.to_string()));
                } else {
                    for (key, value) in parsed.query_pairs() {
                        if key == "v" {
                            return Some(value.to_string());
                        }
                    }
                }
            }
        }
        Err(e) => eprintln!("Error parsing URL {}: {}", url, e),
    }
    None
}

/// Deletes all `.part` files in the specified directory.
///
/// # Arguments
/// * `directory` - The directory to search for `.part` files.
///
/// # Returns
/// A `Result` indicating success or failure.
fn delete_part_files(directory: &str) -> std::io::Result<()> {
    // Use OS-specific path separators
    let pattern = format!("{}/{}.part", directory, "*").replace("\\", "/"); 

    for entry in glob(&pattern).expect("Failed to read glob pattern") {
        match entry {
            Ok(path) => {
                if path.is_file() {
                    println!("Deleting: {}", path.display());
                    fs::remove_file(&path)?;
                }
            }
            Err(e) => eprintln!("Error: {}", e),
        }
    }

    Ok(())
}