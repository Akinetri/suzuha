use std::fs;
use std::io::Write;
use std::path::Path;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use amane::{
    Center, Column, Media, Rectangle, Service, Size, Stack, Start, Text, Weight, Widget, children,
};

use super::card;
use crate::fonts;
use crate::integrations::home;
use crate::motion::{self, DEFAULT_SPATIAL};
use crate::theme::{self, Theme};

pub const WIDTH: f32 = 272.0;
pub const HEIGHT: f32 = 196.0;

// the widest the card grows for a long line, past it the line wraps onto a second row
const MAX_WIDTH: f32 = 448.0;

// the card's padding on both sides
const INSET: f32 = 36.0;

const LINE_HEIGHT: f32 = 26.0;

// lines shown before the current one, the rest come after it
const BEFORE: usize = 1;
const SHOWN: usize = 5;

const LRCLIB: &str = "https://lrclib.net/api";

// how often the playing line is looked for, the player itself is only polled every second
const TICK: Duration = Duration::from_millis(200);

// a little early reads better than late, the eye needs a moment to find the line
const LEAD: Duration = Duration::from_millis(300);

#[derive(Default, PartialEq, Clone)]
enum Status {
    #[default]
    Idle,
    Searching,
    Missing,
    // found, but without times, so there is no line to follow
    Unsynced,
    Synced,
}

// the timed lines for what plays now, fetched once per track
#[derive(Default, PartialEq)]
pub struct Lyrics {
    track: Option<Track>,
    status: Status,
    lines: Vec<(Duration, String)>,

    // each line's width as the sung line, measured once per track
    widths: Vec<f32>,

    // the line sung now, none before the first one starts
    current: Option<usize>,
}

#[derive(PartialEq, Clone, Hash)]
struct Track {
    title: String,
    artist: String,
    seconds: u64,
}

/*
 * polled fast so the line changes on time; the player's position only
 * moves once a second, so it is carried forward from when it last moved
 */
impl Service for Lyrics {
    fn new() -> Self {
        Self::default()
    }

    fn listen() {
        let mut anchor = (Duration::ZERO, Instant::now());

        loop {
            thread::sleep(TICK);

            let (track, playing, position) = {
                let media = Media::read();

                let track = media.active().filter(|player| !player.title().is_empty()).map(|player| Track {
                    title: String::from(player.title()),
                    artist: String::from(player.artist()),
                    seconds: media.length().as_secs(),
                });

                (track, media.playing(), media.position())
            };

            if position != anchor.0 {
                anchor = (position, Instant::now());
            }

            let position = if playing { anchor.0 + anchor.1.elapsed().min(Duration::from_secs(2)) } else { anchor.0 };

            if track != Self::read().track {
                load(track);
            }

            let current = Self::read().line_at(position + LEAD);

            if Self::read().current != current {
                Self::write().current = current;
            }
        }
    }
}

impl Lyrics {
    fn wanted_width(&self) -> f32 {
        if self.status != Status::Synced {
            return WIDTH;
        }

        let widest = self.widths.iter().copied().fold(0.0, f32::max);

        // a pixel more, so a line that just fits isn't cut by rounding
        (widest + INSET + 1.0).clamp(WIDTH, MAX_WIDTH)
    }

    /*
     * one row, or two for a line too long even for the widest card; told by
     * the width the card is heading to, so a line keeps its rows while it glides
     */
    fn rows(&self, index: usize) -> f32 {
        let room = self.wanted_width() - INSET;

        if self.widths.get(index).is_some_and(|width| *width > room) { 2.0 } else { 1.0 }
    }

    // how far down the column a line starts, in between two lines for a fractional one
    fn top(&self, at: f32) -> f32 {
        if at < 0.0 {
            return at * LINE_HEIGHT;
        }

        let whole = at.floor() as usize;

        let above: f32 = (0..whole).map(|index| self.rows(index)).sum();

        (above + at.fract() * self.rows(whole)) * LINE_HEIGHT
    }

    fn line_at(&self, position: Duration) -> Option<usize> {
        let after = self.lines.partition_point(|(start, _)| *start <= position);

        after.checked_sub(1)
    }
}

// shows the search at once, then asks the cache or lrclib outside the lock
fn load(track: Option<Track>) {
    let Some(asked) = track.clone() else {
        *Lyrics::write() = Lyrics::default();
        return;
    };

    *Lyrics::write() = Lyrics {
        track: track.clone(),
        status: Status::Searching,
        ..Lyrics::default()
    };

    let found = cached(&asked).or_else(|| fetch(&asked));

    let (status, lines) = match found {
        Some(Found::Synced(text)) => {
            let lines = parse(&text);

            if lines.is_empty() { (Status::Unsynced, lines) } else { (Status::Synced, lines) }
        }
        Some(Found::Plain) => (Status::Unsynced, Vec::new()),
        Some(Found::Missing) => (Status::Missing, Vec::new()),
        // the network failed, it is asked again on the next track
        None => (Status::Missing, Vec::new()),
    };

    let mut lyrics = Lyrics::write();

    // the track changed while it was being asked about, so this answer is for nothing
    if lyrics.track != track {
        return;
    }

    lyrics.widths = lines.iter().map(|(_, words)| measure(words)).collect();
    lyrics.status = status;
    lyrics.lines = lines;
}

// how wide these words are drawn as the sung line, the widest they ever get
fn measure(words: &str) -> f32 {
    let text = Text::new(words).size(16.0).font(fonts::BODY).weight(Weight::Bold);

    match text.width() {
        Size::Fixed(width) => width,
        Size::Parent => 0.0,
    }
}

/*
 * as wide as the card needs for the song's longest line, never narrower
 * than it starts; it glides there when a new song comes in
 */
pub fn width() -> f32 {
    let wanted = Lyrics::read().wanted_width();

    motion::follow("floating:lyrics:width", wanted, DEFAULT_SPATIAL)
}

enum Found {
    Synced(String),
    Plain,
    Missing,
}

// "S" before timed lyrics, "P" for only plain ones, empty when lrclib has none
fn decode(text: &str) -> Found {
    match text.split_at_checked(1) {
        Some(("S", lyrics)) => Found::Synced(String::from(lyrics)),
        Some(("P", _)) => Found::Plain,
        _ => Found::Missing,
    }
}

fn cached(track: &Track) -> Option<Found> {
    let text = fs::read_to_string(cache_path(track)).ok()?;

    Some(decode(&text))
}

fn remember(track: &Track, found: &Found) {
    let path = cache_path(track);

    let text = match found {
        Found::Synced(lyrics) => format!("S{lyrics}"),
        Found::Plain => String::from("P"),
        Found::Missing => String::new(),
    };

    // ponytail: the cache is never cleaned, prune it if it grows
    if let Some(folder) = Path::new(&path).parent() {
        let _ = fs::create_dir_all(folder);
    }

    let _ = fs::write(&path, text);
}

fn cache_path(track: &Track) -> String {
    let mut hasher = DefaultHasher::new();

    track.hash(&mut hasher);

    format!("{}/.cache/amane/lyrics/{:x}", home(), hasher.finish())
}

// the exact answer from /get, told in the cache's own letters
const EXACT: &str = r#"if (.syncedLyrics // "") != "" then "S" + .syncedLyrics
    elif (.plainLyrics // "") != "" then "P" else "" end"#;

// of the timed answers from /search, the one closest in length
const CLOSEST: &str = r#"[.[] | select((.syncedLyrics // "") != "")]
    | if length == 0 then "" else min_by((.duration // 0) - $length | fabs) | "S" + .syncedLyrics end"#;

/*
 * the exact match by title, artist and length first, then a search that
 * tolerates a different album or a slightly different length
 */
fn fetch(track: &Track) -> Option<Found> {
    let mut exact = vec![("track_name", track.title.clone()), ("artist_name", track.artist.clone())];

    if track.seconds > 0 {
        exact.push(("duration", track.seconds.to_string()));
    }

    let first = match get("get", &exact)? {
        Some(body) => decode(&pick(&body, EXACT, track.seconds)?),
        None => Found::Missing,
    };

    let found = match first {
        Found::Synced(_) => first,
        other => {
            let search = [("track_name", track.title.clone()), ("artist_name", track.artist.clone())];

            let closest = match get("search", &search)? {
                Some(body) => decode(&pick(&body, CLOSEST, track.seconds)?),
                None => Found::Missing,
            };

            match closest {
                Found::Synced(_) => closest,
                _ => other,
            }
        }
    };

    remember(track, &found);

    Some(found)
}

/*
 * the body, none for a 404, and nothing at all when the network failed;
 * the words go to curl as separate arguments, so no title reaches a shell
 */
fn get(endpoint: &str, query: &[(&str, String)]) -> Option<Option<String>> {
    let mut command = Command::new("curl");

    command
        .args(["-s", "-G", "--max-time", "15", "-A", "amane-shell (github.com/MystiaFin/suzuha)"])
        .args(["-w", "\n%{http_code}"]);

    for (key, value) in query {
        command.arg("--data-urlencode").arg(format!("{key}={value}"));
    }

    let output = command.arg(format!("{LRCLIB}/{endpoint}")).output().ok()?;

    let text = String::from_utf8_lossy(&output.stdout);

    let (body, code) = text.rsplit_once('\n')?;

    match code.trim() {
        "200" => Some(Some(String::from(body))),
        "404" => Some(None),
        _ => None,
    }
}

// runs a jq filter over the body, which goes in on stdin
fn pick(body: &str, filter: &str, length: u64) -> Option<String> {
    let mut jq = Command::new("jq")
        .args(["-r", "--argjson", "length", &length.to_string(), filter])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    jq.stdin.take()?.write_all(body.as_bytes()).ok()?;

    let output = jq.wait_with_output().ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);

    Some(String::from(text.strip_suffix('\n').unwrap_or(&text)))
}

// "[01:23.45]words" lines, a line can carry several times; tags like [ar:...] are skipped
fn parse(text: &str) -> Vec<(Duration, String)> {
    let mut lines = Vec::new();

    for line in text.lines() {
        let mut rest = line.trim();
        let mut times = Vec::new();

        while let Some(inside) = rest.strip_prefix('[') {
            let Some((stamp, after)) = inside.split_once(']') else {
                break;
            };

            let Some(time) = timestamp(stamp) else {
                break;
            };

            times.push(time);
            rest = after;
        }

        let words = rest.trim();

        for time in times {
            lines.push((time, String::from(words)));
        }
    }

    lines.sort_by_key(|(time, _)| *time);

    lines
}

// "mm:ss.xx" or "mm:ss"
fn timestamp(stamp: &str) -> Option<Duration> {
    let (minutes, seconds) = stamp.split_once(':')?;

    let minutes: u64 = minutes.parse().ok()?;
    let seconds: f64 = seconds.parse().ok()?;

    if !(0.0..60.0).contains(&seconds) {
        return None;
    }

    Some(Duration::from_secs(minutes * 60) + Duration::from_secs_f64(seconds))
}

/*
 * the sung line in the middle, bold, with the one before and the next ones
 * fading out around it; the column glides up as the song moves on
 */
pub fn view(theme: &Theme, width: f32, top: f32, bottom: f32) -> Rectangle {
    let lyrics = Lyrics::read();

    let inner_width = width - INSET;

    let label = Text::new("LYRICS")
        .size(13.0)
        .font(fonts::BODY)
        .weight(Weight::SemiBold)
        .color(theme.secondary_text);

    let note = |text: &str| {
        Rectangle::new()
            .width(inner_width)
            .height(LINE_HEIGHT * SHOWN as f32)
            .align_child(Start, Center)
            .child(Text::new(text).size(14.0).font(fonts::BODY).color(theme.muted_text))
    };

    let body = match lyrics.status {
        Status::Idle => note("Nothing playing"),
        Status::Searching => note("Looking for lyrics…"),
        Status::Missing => note("No lyrics found"),
        Status::Unsynced => note("Only unsynced lyrics found"),
        Status::Synced => lines(theme, &lyrics, inner_width),
    };

    card::joined(theme, width, HEIGHT, top, bottom).child(Column::new(children![label, body]).gap(8.0))
}

fn lines(theme: &Theme, lyrics: &Lyrics, width: f32) -> Rectangle {
    // before the first line the column waits one row lower, with nothing above
    let index = lyrics.current.map_or(-1.0, |index| index as f32);

    let offset = motion::follow("floating:lyrics:scroll", index, DEFAULT_SPATIAL);

    let first = (offset.floor() as isize - BEFORE as isize - 1).max(0) as usize;
    let last = (first + SHOWN + 3).min(lyrics.lines.len());

    let height = LINE_HEIGHT * SHOWN as f32;

    let scroll = lyrics.top(offset - BEFORE as f32);

    let mut column: Vec<Box<dyn Widget>> = Vec::new();

    for (index, (_, words)) in lyrics.lines.iter().enumerate().take(last).skip(first) {
        let distance = (index as f32 - offset).abs();

        let current = Some(index) == lyrics.current;

        let color = if current {
            theme.text
        } else {
            theme::with_opacity(theme.secondary_text, (1.0 - distance * 0.25).clamp(0.2, 0.9))
        };

        // instrumental gaps come as empty lines
        let words = if words.is_empty() { "♪" } else { words.as_str() };

        let rows = lyrics.rows(index);

        let text = Text::new(words)
            .size(if current { 16.0 } else { 14.0 })
            .font(fonts::BODY)
            .weight(if current { Weight::Bold } else { Weight::Medium })
            .color(color);

        // past two rows the line is still cut short
        let text = if rows > 1.0 { text.wrap().max_lines(2).elide() } else { text.elide() };

        // the line before the sung one sits whole at the top, however many rows it takes
        let y = lyrics.top(index as f32) - scroll;

        column.push(Box::new(
            Rectangle::new()
                .width(width)
                .height(LINE_HEIGHT * rows)
                .align_child(Start, Center)
                .child(text)
                .translate(0.0, y),
        ));
    }

    Rectangle::new()
        .width(width)
        .height(height)
        .clip()
        .child(Stack::new(column).width(width).height(height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_times_and_skips_tags() {
        let lines = parse("[ar:someone]\n[00:12.50]one\n[01:02]two\n[00:05.00][00:30.00]again\nnoise");

        let times: Vec<f64> = lines.iter().map(|(time, _)| time.as_secs_f64()).collect();

        assert_eq!(times, [5.0, 12.5, 30.0, 62.0]);
        assert_eq!(lines[1].1, "one");
    }

    #[test]
    fn finds_the_sung_line() {
        let lyrics = Lyrics {
            lines: parse("[00:10.00]one\n[00:20.00]two"),
            ..Lyrics::default()
        };

        assert_eq!(lyrics.line_at(Duration::from_secs(5)), None);
        assert_eq!(lyrics.line_at(Duration::from_secs(10)), Some(0));
        assert_eq!(lyrics.line_at(Duration::from_secs(25)), Some(1));
    }

    #[test]
    fn long_lines_take_two_rows() {
        let lyrics = Lyrics {
            status: Status::Synced,
            lines: parse("[00:01.00]a\n[00:02.00]b\n[00:03.00]c"),
            widths: vec![100.0, 900.0, 100.0],
            ..Lyrics::default()
        };

        assert_eq!(lyrics.top(-1.0), -LINE_HEIGHT);
        assert_eq!(lyrics.top(1.0), LINE_HEIGHT);
        assert_eq!(lyrics.top(1.5), LINE_HEIGHT * 2.0);
        assert_eq!(lyrics.top(2.0), LINE_HEIGHT * 3.0);
    }
}
