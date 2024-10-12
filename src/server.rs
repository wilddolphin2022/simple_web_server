use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::fs::{File, read_dir};
use serde::{Serialize, Deserialize};
use serde_json::json;
use hound;
use regex::Regex;
use url::Url;
use mp3_duration;


#[derive(Serialize, Deserialize)]
struct FileMetadata {
    name: String,
    size: u64,
    duration: Option<f32>,
}

pub async fn handle_connection(mut stream: impl AsyncReadExt + AsyncWriteExt + Unpin) -> Result<(), Box<dyn std::error::Error>> {
    let mut buffer = [0; 1024];
    stream.read(&mut buffer).await?;

    let request = String::from_utf8_lossy(&buffer[..]);
    let mut request_line = request.lines().next().unwrap_or("");

    if request_line.starts_with("POST /files/") {
        handle_file_upload(&mut stream, request_line).await?;
    } else if request_line.starts_with("GET /files") {
        // clean up request line from stray code
        if request_line.contains(" HTTP/1.1") {
            request_line = request_line.split(" HTTP/1.1").nth(0).unwrap_or(request_line)
        }

        handle_file_list(&mut stream, request_line).await?;
    } else if request_line.starts_with("GET /content/") {
        handle_file_content(&mut stream, &request_line).await?;
    } else if request_line.starts_with("GET /metadata/") {
        handle_file_metadata(&mut stream, &request_line).await?;
    } else if request_line.starts_with("GET / HTTP/1.1") {
        let content = tokio::fs::read(&"hello.html").await?;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n"
        );
        stream.write_all(response.as_bytes()).await?;
        stream.write_all(&content).await?;
    } else {
        let content = tokio::fs::read(&"404.html").await?;
        let response = format!(
            "HTTP/1.1 404 NOT FOUND\r\nContent-Type: text/html\r\n\r\n"
        );
        stream.write_all(response.as_bytes()).await?;
        stream.write_all(&content).await?;
    }

    stream.flush().await?;
    Ok(())
}

async fn handle_file_upload(stream: &mut (impl AsyncReadExt + AsyncWriteExt + Unpin), request_line: &str) -> Result<(), Box<dyn std::error::Error>> {
    let re = Regex::new(r"POST /files/(.+) HTTP").unwrap();
    if let Some(captures) = re.captures(request_line) {
        let filename = captures.get(1).unwrap().as_str();
        let path = Path::new("files").join(filename);

        tokio::fs::create_dir_all("files").await?;
        let mut file = File::create(&path).await?;

        let mut buffer = vec![0; 1024];
        loop {
            let bytes_read = stream.read(&mut buffer).await?;
            if bytes_read == 0 {
                break;
            }
            file.write_all(&buffer[..bytes_read]).await?;

            if bytes_read < 1024 {
                break;
            }
        }

        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{}",
            json!({"message": "File uploaded successfully", "filename": filename})
        );
        stream.write_all(response.as_bytes()).await?;
    } else {
        let response = "HTTP/1.1 400 BAD REQUEST\r\n\r\nInvalid request";
        stream.write_all(response.as_bytes()).await?;
    }
    Ok(())
}

async fn handle_file_list(stream: &mut (impl AsyncReadExt + AsyncWriteExt + Unpin), request_line: &str) -> Result<(), Box<dyn std::error::Error>> {
    let query = request_line.split('?').nth(1).unwrap_or("");
    let parsed_url = Url::parse(&format!("http://dummy.com?{}", query))?;
    let query_pairs: Vec<(String, String)> = parsed_url.query_pairs().into_owned().collect();

    let mut filter = query_pairs.iter().find(|(key, _)| key == "filter").map(|(_, value)| value.to_string());
    if filter == None && Some(query).is_some() {
        filter = Some(query.to_owned());
    }

    let max_duration: Option<f32> = query_pairs.iter()
        .find(|(key, _)| key == "maxduration")
        .and_then(|(_, value)| value.parse().ok());

    // println!("handle_file_list:: parsed_url: {:?}, filter {:?}, max_duration {:?}", parsed_url, filter, max_duration);

    let mut entries = Vec::new();
    let mut dir = read_dir("files").await?;

    while let Some(entry) = dir.next_entry().await? {
        let file_name = entry.file_name().to_string_lossy().to_lowercase();
        let path = entry.path();
        let duration = get_audio_duration(&path).await;

        if filter.as_ref().map_or(true, |f| file_name.contains(&f.to_lowercase())) &&
           max_duration.map_or(true, |max| duration.map_or(false, |d| d <= max)) {
            let metadata = entry.metadata().await?;
            entries.push(FileMetadata {
                name: entry.file_name().to_string_lossy().into_owned(),
                size: metadata.len(),
                duration,
            });
        }
    }

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{}\r\n",
        serde_json::to_string(&entries)?
    );

    stream.write_all(response.as_bytes()).await?;
    Ok(())
}

async fn handle_file_content(stream: &mut (impl AsyncReadExt + AsyncWriteExt + Unpin), request_line: &str) -> Result<(), Box<dyn std::error::Error>> {
    let re = Regex::new(r"GET /content/(.+) HTTP").unwrap();
    if let Some(captures) = re.captures(request_line) {
        let filename = captures.get(1).unwrap().as_str();
        let path = Path::new("files").join(filename);
        
        if path.exists() {
            let content = tokio::fs::read(&path).await?;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=\"{}\"\r\n\r\n",
                filename
            );
            stream.write_all(response.as_bytes()).await?;
            stream.write_all(&content).await?;
        } else {
            let response = "HTTP/1.1 404 NOT FOUND\r\n\r\nFile not found";
            stream.write_all(response.as_bytes()).await?;
        }
    } else {
        let response = "HTTP/1.1 400 BAD REQUEST\r\n\r\nInvalid request";
        stream.write_all(response.as_bytes()).await?;
    }
    Ok(())
}

async fn handle_file_metadata(stream: &mut (impl AsyncReadExt + AsyncWriteExt + Unpin), request_line: &str) -> Result<(), Box<dyn std::error::Error>> {
    let re = Regex::new(r"GET /metadata/(.+) HTTP").unwrap();
    if let Some(captures) = re.captures(request_line) {
        let filename = captures.get(1).unwrap().as_str();
        let path = Path::new("files").join(filename);
        
        if path.exists() {
            let metadata = tokio::fs::metadata(&path).await?;
            let file_metadata = FileMetadata {
                name: filename.to_string(),
                size: metadata.len(),
                duration: get_audio_duration(&path).await,
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{}",
                serde_json::to_string(&file_metadata)?
            );
            stream.write_all(response.as_bytes()).await?;
        } else {
            let response = "HTTP/1.1 404 NOT FOUND\r\n\r\nFile not found";
            stream.write_all(response.as_bytes()).await?;
        }
    } else {
        let response = "HTTP/1.1 400 BAD REQUEST\r\n\r\nInvalid request";
        stream.write_all(response.as_bytes()).await?;
    }
    Ok(())
}

async fn get_audio_duration(path: &PathBuf) -> Option<f32> {
    if let Some(extension) = path.extension() {
        match extension.to_str() {
            Some("wav") => get_wav_duration(path),
            Some("mp3") => get_mp3_duration(path),
            _ => None,
        }
    } else {
        None
    }
}

fn get_wav_duration(path: &PathBuf) -> Option<f32> {
    if let Ok(reader) = hound::WavReader::open(path) {
        let spec = reader.spec();
        let duration = reader.duration() as f32 / spec.sample_rate as f32;
        Some(duration)
    } else {
        None
    }
}

fn get_mp3_duration(path: &PathBuf) -> Option<f32> {
    // println!("get_mp3_duration:: mp3 path {:?}, duration {:?}", path, mp3_duration::from_path(path));
    match mp3_duration::from_path(path) {
        Ok(duration) => Some(duration.as_secs_f32()),
        Err(_) => None,
    }
}



