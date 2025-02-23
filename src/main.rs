use reqwest::Client;
use roxmltree::Document;
use std::collections::HashSet;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;
use url::Url;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
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
    // Under Debian, we use /tmp instead of Windows paths.
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
    // Use a cookies file (adjust as needed). Make sure this file exists.
    let cookies_file = "/home/craig/youtube/cookies.txt";
    // For Debian, we assume yt-dlp is in your PATH.
    // Specify the location of ffmpeg (if necessary). Adjust if needed.
    let ffmpeg_location = "/usr/bin";

    // Process each video URL.
    for video_url in &video_urls {
        // Extract the video ID from the URL.
        if let Some(video_id) = get_video_id(video_url) {
            if downloaded_ids.contains(&video_id) {
                println!("Skipping {} (already downloaded).", video_url);
                continue;
            }
        }

        println!("Downloading {} ...", video_url);

        // Build the process start info.
        // This command mirrors the C# command-line arguments.
        let yt_dlp_executable = "/home/craig/youtube/yt-dlp_linux";
        let args = vec![
            "--download-archive", archive_file,
            "-f", "bestvideo+bestaudio/best",
            "--ffmpeg-location", ffmpeg_location,
            "--cookies", cookies_file,
            "-o", &output_template,
            video_url,
        ];

        let mut cmd = Command::new(yt_dlp_executable);
        cmd.args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = cmd.spawn()?.wait_with_output().await?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        println!("{}", stdout);
        if !stderr.trim().is_empty() {
            println!("Error: {}", stderr);
        }
        // If the download was successful, update our in-memory archive.
        if output.status.success() {
            if let Some(video_id) = get_video_id(video_url) {
                downloaded_ids.insert(video_id);
            }
        }
    }

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
