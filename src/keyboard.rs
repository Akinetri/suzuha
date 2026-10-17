use std::thread;
use std::time::Duration;

use amane::Service;

// how long to wait before following niri again after its event stream ended
const RETRY: Duration = Duration::from_secs(3);

// short names for the layouts niri calls by their full xkb names
const SHORT: [(&str, &str); 16] = [
    ("English", "EN"),
    ("Russian", "RU"),
    ("Ukrainian", "UA"),
    ("Belarusian", "BY"),
    ("Kazakh", "KZ"),
    ("German", "DE"),
    ("French", "FR"),
    ("Spanish", "ES"),
    ("Italian", "IT"),
    ("Polish", "PL"),
    ("Czech", "CZ"),
    ("Japanese", "JP"),
    ("Korean", "KO"),
    ("Chinese", "ZH"),
    ("Greek", "GR"),
    ("Turkish", "TR"),
];

// niri's keyboard layouts and the one in use, followed through its event stream
#[derive(Default, PartialEq)]
pub struct Keyboard {
    names: Vec<String>,
    current: usize,
}

impl Service for Keyboard {
    fn new() -> Self {
        Self::default()
    }

    fn listen() {
        loop {
            for line in amane::lines("niri msg --json event-stream") {
                if let Some(names) = layouts(&line) {
                    let mut keyboard = Self::write();

                    keyboard.names = names;
                    keyboard.current = index(&line, "\"current_idx\":").unwrap_or(0);
                } else if line.contains("\"KeyboardLayoutSwitched\"") {
                    if let Some(current) = index(&line, "\"idx\":") {
                        Self::write().current = current;
                    }
                }
            }

            thread::sleep(RETRY);
        }
    }
}

impl Keyboard {
    // like "EN", none with a single layout or before niri said
    pub fn short_name(&self) -> Option<String> {
        if self.names.len() < 2 {
            return None;
        }

        let name = self.names.get(self.current)?;

        for (full, short) in SHORT {
            if name.starts_with(full) {
                return Some(String::from(short));
            }
        }

        Some(name.chars().take(2).collect::<String>().to_uppercase())
    }

    pub fn next() {
        amane::spawn("niri msg action switch-layout next");
    }
}

// {"KeyboardLayoutsChanged":{"keyboard_layouts":{"names":["English (US)","Russian"],"current_idx":1}}}
fn layouts(line: &str) -> Option<Vec<String>> {
    if !line.contains("\"KeyboardLayoutsChanged\"") {
        return None;
    }

    let start = line.find("\"names\":[")? + "\"names\":[".len();

    let end = start + line[start..].find(']')?;

    let names = line[start..end]
        .split("\",\"")
        .map(|name| name.trim_matches('"').to_string())
        .filter(|name| !name.is_empty())
        .collect();

    Some(names)
}

fn index(line: &str, key: &str) -> Option<usize> {
    let start = line.find(key)? + key.len();

    let digits: String = line[start..].chars().take_while(char::is_ascii_digit).collect();

    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_layout_events() {
        let changed = r#"{"KeyboardLayoutsChanged":{"keyboard_layouts":{"names":["English (US)","Russian"],"current_idx":1}}}"#;

        assert_eq!(layouts(changed), Some(vec![String::from("English (US)"), String::from("Russian")]));
        assert_eq!(index(changed, "\"current_idx\":"), Some(1));

        let switched = r#"{"KeyboardLayoutSwitched":{"idx":0}}"#;

        assert_eq!(layouts(switched), None);
        assert_eq!(index(switched, "\"idx\":"), Some(0));
    }
}
