use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use id3::{Content, Frame, Tag, TagLike, Version};
use id3::frame::{Picture, PictureType};
use regex::Regex;

use crate::itunes::TrackData;

fn safe(s: &str) -> String {
    let re = Regex::new(r#"[<>:"/\\|?*\x00-\x1f]"#).unwrap();
    re.replace_all(s.trim(), "_").to_string()
}

pub fn download_track(data: &TrackData, output_dir: &str, quality: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(output_dir)?;

    let query = if data.artist.is_empty() {
        data.title.clone()
    } else {
        format!("{} - {}", data.artist, data.title)
    };

    let filename = if data.artist.is_empty() {
        safe(&data.title)
    } else {
        format!("{} - {}", safe(&data.artist), safe(&data.title))
    };

    let outtmpl = format!("{output_dir}/{filename}.%(ext)s");

    let status = std::process::Command::new("yt-dlp")
        .args([
            "-x",
            "--audio-format",
            "mp3",
            "--audio-quality",
            &format!("{quality}k"),
            "--match-filter",
            "duration < 1200",
            "--no-playlist",
            "--quiet",
            "--no-warnings",
            "-o",
            &outtmpl,
            &format!("ytsearch1:{query}"),
        ])
        .status()?;

    if !status.success() {
        return Err(anyhow!(
            "yt-dlp exited with code {:?}",
            status.code()
        ));
    }

    let mp3 = PathBuf::from(output_dir).join(format!("{filename}.mp3"));
    if !mp3.exists() {
        return Err(anyhow!("Expected MP3 not found: {}", mp3.display()));
    }

    Ok(mp3)
}

pub fn embed_metadata(path: &Path, data: &TrackData) -> Result<()> {
    let mut tag = Tag::new();

    tag.set_title(&data.title);
    tag.set_artist(&data.artist);
    tag.set_album(&data.album);

    if let Ok(y) = data.year.parse::<i32>() {
        tag.set_year(y);
    }
    if !data.genre.is_empty() {
        tag.set_genre(&data.genre);
    }
    if let Some(n) = data.track_number {
        tag.set_track(n);
    }
    if let Some(n) = data.track_count {
        tag.set_total_tracks(n);
    }
    if let Some(n) = data.disk_number {
        tag.set_disc(n);
    }
    if let Some(n) = data.disk_count {
        tag.set_total_discs(n);
    }
    if let Some(ms) = data.duration_ms {
        tag.set_duration(ms as u32);
    }

    for (id, val) in [
        ("TPE2", &data.album_artist),
        ("TCOM", &data.composer),
        ("TCOP", &data.copyright),
    ] {
        if !val.is_empty() {
            tag.add_frame(Frame::with_content(id, Content::Text(val.clone())));
        }
    }

    if let Some(url) = &data.artwork_url {
        if let Ok(bytes) = fetch_bytes(url) {
            tag.add_frame(Picture {
                mime_type: "image/jpeg".to_string(),
                picture_type: PictureType::CoverFront,
                description: "Cover".to_string(),
                data: bytes,
            });
        }
    }

    tag.write_to_path(path, Version::Id3v24)?;
    Ok(())
}

fn fetch_bytes(url: &str) -> Result<Vec<u8>> {
    let bytes = reqwest::blocking::get(url)?.bytes()?;
    Ok(bytes.to_vec())
}
