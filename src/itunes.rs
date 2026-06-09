use anyhow::{anyhow, Result};
use regex::Regex;
use serde::Deserialize;
use url::Url;

#[derive(Debug, Clone, Default)]
pub struct TrackData {
    pub artist: String,
    pub title: String,
    pub album: String,
    pub album_artist: String,
    pub year: String,
    pub genre: String,
    pub composer: String,
    pub copyright: String,
    pub track_number: Option<u32>,
    pub track_count: Option<u32>,
    pub disk_number: Option<u32>,
    pub disk_count: Option<u32>,
    pub duration_ms: Option<u64>,
    pub artwork_url: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ItunesResponse {
    results: Vec<ItunesItem>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ItunesItem {
    #[serde(rename = "wrapperType")]
    wrapper_type: String,
    #[serde(rename = "artistName")]
    artist_name: String,
    #[serde(rename = "trackName")]
    track_name: String,
    #[serde(rename = "collectionName")]
    collection_name: String,
    #[serde(rename = "collectionArtistName")]
    collection_artist_name: Option<String>,
    #[serde(rename = "trackNumber")]
    track_number: Option<u32>,
    #[serde(rename = "trackCount")]
    track_count: Option<u32>,
    #[serde(rename = "discNumber")]
    disc_number: Option<u32>,
    #[serde(rename = "discCount")]
    disc_count: Option<u32>,
    #[serde(rename = "releaseDate")]
    release_date: Option<String>,
    #[serde(rename = "primaryGenreName")]
    primary_genre_name: Option<String>,
    #[serde(rename = "composerName")]
    composer_name: Option<String>,
    copyright: Option<String>,
    #[serde(rename = "trackTimeMillis")]
    track_time_millis: Option<u64>,
    #[serde(rename = "artworkUrl100")]
    artwork_url100: Option<String>,
}

pub fn resolve_url(raw_url: &str) -> Result<Vec<TrackData>> {
    let parsed = Url::parse(raw_url).map_err(|e| anyhow!("Invalid URL: {e}"))?;
    let path = parsed.path().trim_matches('/');
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();

    if parts.contains(&"playlist") {
        return fetch_playlist(raw_url);
    }

    if parts.contains(&"song") || parts.contains(&"music-video") {
        let track_id = parts
            .iter()
            .rev()
            .find(|&&p| p.chars().all(|c| c.is_ascii_digit()))
            .ok_or_else(|| anyhow!("No track ID found in URL"))?;
        return fetch_track(track_id);
    }

    if parts.contains(&"album") {
        let qs: std::collections::HashMap<String, String> = parsed
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();

        if let Some(track_id) = qs.get("i") {
            return fetch_track(track_id);
        }

        let album_id = parts
            .iter()
            .rev()
            .find(|&&p| p.chars().all(|c| c.is_ascii_digit()))
            .ok_or_else(|| anyhow!("No album ID found in URL"))?;
        return fetch_album(album_id);
    }

    Err(anyhow!(
        "Unrecognized URL — expected an Apple Music album, track, or playlist link"
    ))
}

fn fetch_track(track_id: &str) -> Result<Vec<TrackData>> {
    let url = format!("https://itunes.apple.com/lookup?id={track_id}");
    let resp: ItunesResponse = reqwest::blocking::get(&url)?.json()?;
    Ok(resp.results.iter().map(item_to_track).collect())
}

fn fetch_album(album_id: &str) -> Result<Vec<TrackData>> {
    let url = format!("https://itunes.apple.com/lookup?id={album_id}&entity=song");
    let resp: ItunesResponse = reqwest::blocking::get(&url)?.json()?;
    let mut tracks: Vec<TrackData> = resp
        .results
        .iter()
        .filter(|i| i.wrapper_type == "track")
        .map(item_to_track)
        .collect();
    tracks.sort_by_key(|t| {
        (
            t.disk_number.unwrap_or(0),
            t.track_number.unwrap_or(0),
        )
    });
    Ok(tracks)
}

fn itunes_search(artist: &str, title: &str) -> Option<TrackData> {
    let query = format!("{artist} {title}");
    let url = format!(
        "https://itunes.apple.com/search?term={}&media=music&limit=1",
        urlencoding::encode(&query),
    );
    let resp: ItunesResponse = reqwest::blocking::get(&url).ok()?.json().ok()?;
    resp.results.first().map(|item| item_to_track(item))
}

fn item_to_track(item: &ItunesItem) -> TrackData {
    let artwork = item.artwork_url100.as_ref().map(|u| {
        let re = Regex::new(r"\d+x\d+bb").unwrap();
        re.replace(u, "3000x3000bb").into_owned()
    });

    TrackData {
        artist: item.artist_name.clone(),
        title: item.track_name.clone(),
        album: item.collection_name.clone(),
        album_artist: item
            .collection_artist_name
            .clone()
            .unwrap_or_else(|| item.artist_name.clone()),
        year: item
            .release_date
            .as_deref()
            .and_then(|d| d.get(..4))
            .unwrap_or("")
            .to_string(),
        genre: item.primary_genre_name.clone().unwrap_or_default(),
        composer: item.composer_name.clone().unwrap_or_default(),
        copyright: item.copyright.clone().unwrap_or_default(),
        track_number: item.track_number,
        track_count: item.track_count,
        disk_number: item.disc_number,
        disk_count: item.disc_count,
        duration_ms: item.track_time_millis,
        artwork_url: artwork,
    }
}

fn fetch_playlist(url: &str) -> Result<Vec<TrackData>> {
    let client = reqwest::blocking::Client::new();
    let html = client
        .get(url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
             AppleWebKit/537.36 (KHTML, like Gecko) \
             Chrome/124.0.0.0 Safari/537.36",
        )
        .send()?
        .text()?;

    let raw = scrape_tracks_from_html(&html);

    let enriched = raw
        .into_iter()
        .map(|(artist, title, album)| {
            itunes_search(&artist, &title).unwrap_or(TrackData {
                artist,
                title,
                album,
                ..Default::default()
            })
        })
        .collect();

    Ok(enriched)
}

fn scrape_tracks_from_html(html: &str) -> Vec<(String, String, String)> {
    let re_serialized = Regex::new(
        r#"(?s)<script[^>]+id=["']serialized-server-data["'][^>]*>(.*?)</script>"#,
    )
    .unwrap();
    if let Some(cap) = re_serialized.captures(html) {
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&cap[1]) {
            let tracks = extract_serialized_tracks(&data);
            if !tracks.is_empty() {
                return tracks;
            }
        }
    }

    let re_jsonld = Regex::new(
        r#"(?s)<script[^>]+type=["']application/ld\+json["'][^>]*>(.*?)</script>"#,
    )
    .unwrap();
    for cap in re_jsonld.captures_iter(html) {
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&cap[1]) {
            let tracks = extract_jsonld_tracks(&data);
            if !tracks.is_empty() {
                return tracks;
            }
        }
    }

    let re_shoebox = Regex::new(
        r#"(?s)<script[^>]+id=["']shoebox-media-api-cache-amp-music["'][^>]*>(.*?)</script>"#,
    )
    .unwrap();
    if let Some(cap) = re_shoebox.captures(html) {
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&cap[1]) {
            let tracks = extract_recursive_tracks(&data);
            if !tracks.is_empty() {
                return tracks;
            }
        }
    }

    Vec::new()
}

fn extract_serialized_tracks(v: &serde_json::Value) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    match v {
        serde_json::Value::Object(obj) => {
            let is_song = obj
                .get("contentDescriptor")
                .and_then(|c| c.get("kind"))
                .and_then(|k| k.as_str())
                == Some("song");
            if is_song {
                let title = obj.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let artist = obj.get("artistName").and_then(|v| v.as_str()).unwrap_or("").to_string();
                if !title.is_empty() && !artist.is_empty() {
                    let album = obj
                        .get("tertiaryLinks")
                        .and_then(|v| v.as_array())
                        .and_then(|a| a.first())
                        .and_then(|t| t.get("title"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    out.push((artist, title, album));
                    return out;
                }
            }
            for (_, val) in obj {
                out.extend(extract_serialized_tracks(val));
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                out.extend(extract_serialized_tracks(item));
            }
        }
        _ => {}
    }
    out
}

fn extract_jsonld_tracks(v: &serde_json::Value) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    match v {
        serde_json::Value::Array(arr) => {
            for item in arr {
                out.extend(extract_jsonld_tracks(item));
            }
        }
        serde_json::Value::Object(obj) => {
            let t = obj.get("@type").and_then(|v| v.as_str()).unwrap_or("");
            if matches!(t, "MusicPlaylist" | "MusicAlbum") {
                if let Some(serde_json::Value::Array(items)) = obj.get("track") {
                    let album = obj
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    for item in items {
                        let title = item
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let artist = match item.get("byArtist") {
                            Some(serde_json::Value::Object(a)) => {
                                a.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string()
                            }
                            Some(serde_json::Value::String(s)) => s.clone(),
                            _ => String::new(),
                        };
                        if !title.is_empty() {
                            out.push((artist, title, album.clone()));
                        }
                    }
                }
            }
        }
        _ => {}
    }
    out
}

fn extract_recursive_tracks(v: &serde_json::Value) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    match v {
        serde_json::Value::Object(obj) => {
            if let Some(attrs) = obj.get("attributes").and_then(|v| v.as_object()) {
                let artist = attrs
                    .get("artistName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let title = attrs
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let album = attrs
                    .get("albumName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if !artist.is_empty() && !title.is_empty() {
                    out.push((artist, title, album));
                    return out;
                }
            }
            for (_, val) in obj {
                match val {
                    serde_json::Value::String(s) => {
                        if let Ok(inner) = serde_json::from_str::<serde_json::Value>(s) {
                            out.extend(extract_recursive_tracks(&inner));
                        }
                    }
                    other => out.extend(extract_recursive_tracks(other)),
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                out.extend(extract_recursive_tracks(item));
            }
        }
        _ => {}
    }
    out
}

mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
                | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
                b' ' => out.push('+'),
                other => out.push_str(&format!("%{other:02X}")),
            }
        }
        out
    }
}
