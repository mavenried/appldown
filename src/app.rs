use std::sync::mpsc::Sender;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::itunes::{self, TrackData};
use crate::download;

#[derive(Debug, Clone, PartialEq)]
pub enum TrackStatus {
    Pending,
    Downloading,
    Embedding,
    Done,
    Failed(String),
}

#[derive(Debug, Clone)]
pub enum LogEntry {
    Info(String),
    Success(String),
    Error(String),
    Dim(String),
}

#[derive(Debug, Clone)]
pub struct Track {
    pub id: usize,
    pub data: TrackData,
    pub status: TrackStatus,
}

pub enum AppMsg {
    TrackAdded(TrackData),
    SetStatus(usize, TrackStatus),
    Log(LogEntry),
    Progress(usize, usize),
    FetchDone,
    DownloadDone,
}

pub struct App {
    pub urls: Vec<String>,

    pub tracks: Vec<Track>,
    pub track_offset: usize,

    pub input: String,
    pub cursor: usize,

    pub log: Vec<LogEntry>,
    pub log_scroll: u16,

    pub fetching: bool,
    pub downloading: bool,
    pub progress: (usize, usize),

    pub tick: u8,

    pub output_dir: String,
    pub quality: String,

    tx: Sender<AppMsg>,
    next_id: usize,
}

impl App {
    pub fn new(tx: Sender<AppMsg>) -> Self {
        let mut app = Self {
            urls: Vec::new(),
            tracks: Vec::new(),
            track_offset: 0,
            input: String::new(),
            cursor: 0,
            log: Vec::new(),
            log_scroll: 0,
            fetching: false,
            downloading: false,
            progress: (0, 0),
            tick: 0,
            output_dir: "downloads".to_string(),
            quality: "320".to_string(),
            tx,
            next_id: 0,
        };
        app.log(LogEntry::Dim(
            "appldown — paste a URL and press Enter, then F5 to fetch, F6 to download.".into(),
        ));
        app
    }

    pub fn log(&mut self, entry: LogEntry) {
        self.log.push(entry);
        if self.log.len() > 1000 {
            self.log.drain(..200);
        }
        self.log_scroll = 0;
    }

    fn alloc_id(&mut self) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return true,
            KeyCode::Char('q') if key.modifiers.is_empty() && self.input.is_empty() => return true,

            KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => self.clear(),
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.start_download()
            }
            KeyCode::F(5) => self.start_fetch(),
            KeyCode::F(6) => self.start_download(),

            KeyCode::Enter => {
                let url = self.input.trim().to_string();
                if !url.is_empty() {
                    self.enqueue_url(url);
                    self.input.clear();
                    self.cursor = 0;
                }
            }

            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete_forward(),
            KeyCode::Left => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
            }
            KeyCode::Right => {
                if self.cursor < self.input.chars().count() {
                    self.cursor += 1;
                }
            }
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.input.chars().count(),
            KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.cursor = 0
            }
            KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.cursor = self.input.chars().count()
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let tail: String = self.input.chars().skip(self.cursor).collect();
                self.input = tail;
                self.cursor = 0;
            }
            KeyCode::Char(c) => self.insert_char(c),

            KeyCode::Up => self.track_offset = self.track_offset.saturating_sub(1),
            KeyCode::Down => {
                if self.track_offset + 1 < self.tracks.len() {
                    self.track_offset += 1;
                }
            }

            KeyCode::PageUp => self.log_scroll += 5,
            KeyCode::PageDown => self.log_scroll = self.log_scroll.saturating_sub(5),

            _ => {}
        }
        false
    }

    fn insert_char(&mut self, c: char) {
        let byte = self.char_to_byte(self.cursor);
        self.input.insert(byte, c);
        self.cursor += 1;
    }

    fn backspace(&mut self) {
        if self.cursor > 0 {
            let start = self.char_to_byte(self.cursor - 1);
            let end = self.char_to_byte(self.cursor);
            self.input.drain(start..end);
            self.cursor -= 1;
        }
    }

    fn delete_forward(&mut self) {
        let char_len = self.input.chars().count();
        if self.cursor < char_len {
            let start = self.char_to_byte(self.cursor);
            let end = self.char_to_byte(self.cursor + 1);
            self.input.drain(start..end);
        }
    }

    fn char_to_byte(&self, char_idx: usize) -> usize {
        self.input
            .char_indices()
            .nth(char_idx)
            .map(|(b, _)| b)
            .unwrap_or(self.input.len())
    }

    pub fn handle_msg(&mut self, msg: AppMsg) {
        match msg {
            AppMsg::TrackAdded(data) => {
                let id = self.alloc_id();
                self.tracks.push(Track {
                    id,
                    data,
                    status: TrackStatus::Pending,
                });
            }
            AppMsg::SetStatus(id, status) => {
                if let Some(t) = self.tracks.iter_mut().find(|t| t.id == id) {
                    t.status = status;
                }
            }
            AppMsg::Log(entry) => self.log(entry),
            AppMsg::Progress(done, total) => self.progress = (done, total),
            AppMsg::FetchDone => self.fetching = false,
            AppMsg::DownloadDone => self.downloading = false,
        }
    }

    pub fn enqueue_url(&mut self, url: String) {
        self.log(LogEntry::Dim(format!("+ queued: {url}")));
        self.urls.push(url);
    }

    pub fn clear(&mut self) {
        self.tracks.clear();
        self.urls.clear();
        self.log.clear();
        self.progress = (0, 0);
        self.fetching = false;
        self.downloading = false;
        self.track_offset = 0;
        self.log(LogEntry::Dim("Cleared.".into()));
    }

    pub fn start_fetch(&mut self) {
        let url = self.input.trim().to_string();
        if !url.is_empty() {
            self.enqueue_url(url);
            self.input.clear();
            self.cursor = 0;
        }

        if self.urls.is_empty() {
            self.log(LogEntry::Info("No URLs queued.".into()));
            return;
        }
        if self.fetching {
            self.log(LogEntry::Info("Already fetching.".into()));
            return;
        }

        self.fetching = true;
        let urls = self.urls.clone();
        let tx = self.tx.clone();

        std::thread::spawn(move || {
            for url in &urls {
                let _ = tx.send(AppMsg::Log(LogEntry::Info(format!("Fetching {url}…"))));
                match itunes::resolve_url(url) {
                    Ok(tracks) => {
                        let n = tracks.len();
                        for t in tracks {
                            let _ = tx.send(AppMsg::TrackAdded(t));
                        }
                        let _ = tx.send(AppMsg::Log(LogEntry::Success(format!(
                            "  ✓ {n} track(s) found"
                        ))));
                    }
                    Err(e) => {
                        let _ = tx.send(AppMsg::Log(LogEntry::Error(format!("  ✗ {e}"))));
                    }
                }
            }
            let _ = tx.send(AppMsg::FetchDone);
        });
    }

    pub fn start_download(&mut self) {
        let pending: Vec<(usize, TrackData)> = self
            .tracks
            .iter()
            .filter(|t| t.status == TrackStatus::Pending)
            .map(|t| (t.id, t.data.clone()))
            .collect();

        if pending.is_empty() {
            self.log(LogEntry::Info("No pending tracks.".into()));
            return;
        }
        if self.downloading {
            self.log(LogEntry::Info("Already downloading.".into()));
            return;
        }

        self.downloading = true;
        self.progress = (0, pending.len());

        let tx = self.tx.clone();
        let output_dir = self.output_dir.clone();
        let quality = self.quality.clone();
        let total = pending.len();

        std::thread::spawn(move || {
            let mut done = 0usize;

            for (id, data) in pending {
                let query = if data.artist.is_empty() {
                    data.title.clone()
                } else {
                    format!("{} — {}", data.artist, data.title)
                };

                let _ = tx.send(AppMsg::Log(LogEntry::Info(format!("⟳  {query}"))));
                let _ = tx.send(AppMsg::SetStatus(id, TrackStatus::Downloading));

                match download::download_track(&data, &output_dir, &quality) {
                    Ok(path) => {
                        let _ = tx.send(AppMsg::SetStatus(id, TrackStatus::Embedding));
                        if let Err(e) = download::embed_metadata(&path, &data) {
                            let _ = tx.send(AppMsg::Log(LogEntry::Error(format!(
                                "   ⚠ metadata: {e}"
                            ))));
                        }
                        done += 1;
                        let _ = tx.send(AppMsg::SetStatus(id, TrackStatus::Done));
                        let _ = tx.send(AppMsg::Log(LogEntry::Success("   ✓ saved".into())));
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        let _ = tx.send(AppMsg::SetStatus(
                            id,
                            TrackStatus::Failed(msg.clone()),
                        ));
                        let _ = tx.send(AppMsg::Log(LogEntry::Error(format!("   ✗ {msg}"))));
                    }
                }

                let _ = tx.send(AppMsg::Progress(done, total));
            }

            let _ = tx.send(AppMsg::Log(LogEntry::Success(format!(
                "{done}/{total} tracks downloaded."
            ))));
            let _ = tx.send(AppMsg::DownloadDone);
        });
    }
}
