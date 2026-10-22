use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};

use amane::{Apps, Bus, Value};

use super::png;
use crate::integrations::home;

// the interface items speak, and the older name a few programs still use
pub const INTERFACES: [&str; 2] = ["org.kde.StatusNotifierItem", "org.freedesktop.StatusNotifierItem"];

// a pixmap this size or a little bigger is picked, the bar shows icons at about 18px
const WANTED_SIZE: i64 = 48;

// one icon in the tray, as the program showing it describes it
#[derive(Clone, PartialEq)]
pub struct Item {
    // the owner's unique name like :1.42, and the object path on it
    pub owner: String,
    pub path: String,

    pub interface: &'static str,

    pub title: String,

    // the file to draw, none until it was found or when the program gave none
    pub icon: Option<PathBuf>,

    // where its dbusmenu lives, if it has one
    pub menu: Option<String>,

    // a click should open the menu instead of activating the program
    pub only_menu: bool,

    // "Passive" items want to stay out of sight
    pub passive: bool,
}

// every property at once; none when the program doesn't answer on either interface
pub fn read(owner: &str, path: &str) -> Option<Item> {
    let bus = Bus::session();

    for interface in INTERFACES {
        let properties = bus.call(
            owner,
            path,
            "org.freedesktop.DBus.Properties",
            "GetAll",
            &[amane::Argument::from(interface)],
        );

        if !matches!(properties, Value::Map(_)) {
            continue;
        }

        let text = |name: &str| String::from(properties.get(name).text());

        let status = text("Status");

        let attention = status == "NeedsAttention";

        // a program asking for attention shows its attention icon, when it has one
        let mut icon = None;

        if attention {
            icon = named(&text("AttentionIconName"), &text("IconThemePath"))
                .or_else(|| pixmap(properties.get("AttentionIconPixmap"), owner, path));
        }

        let icon = icon
            .or_else(|| named(&text("IconName"), &text("IconThemePath")))
            .or_else(|| pixmap(properties.get("IconPixmap"), owner, path));

        let menu = Some(text("Menu")).filter(|menu| menu.starts_with('/') && menu != "/");

        let mut title = text("Title");

        if title.is_empty() {
            title = text("Id");
        }

        return Some(Item {
            owner: String::from(owner),
            path: String::from(path),
            interface,
            title,
            icon,
            menu,
            only_menu: properties.get("ItemIsMenu").bool(),
            passive: status == "Passive",
        });
    }

    None
}

/*
 * an icon by name: a full path, a file in the folder the program gave,
 * or from the icon theme
 */
fn named(name: &str, theme_path: &str) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }

    if name.starts_with('/') {
        let path = PathBuf::from(name);

        return path.is_file().then_some(path);
    }

    if !theme_path.is_empty() {
        if let Some(found) = search(Path::new(theme_path), name, 4) {
            return Some(found);
        }
    }

    Apps::icon_path(name)
}

// a png or svg with this name somewhere under the folder, like hicolor/22x22/apps/name.png
fn search(folder: &Path, name: &str, depth: u32) -> Option<PathBuf> {
    for extension in ["svg", "png"] {
        let file = folder.join(format!("{name}.{extension}"));

        if file.is_file() {
            return Some(file);
        }
    }

    if depth == 0 {
        return None;
    }

    let mut folders: Vec<PathBuf> = fs::read_dir(folder)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();

    // the bigger drawings sort last, and are found first
    folders.sort();
    folders.reverse();

    folders.into_iter().find_map(|folder| search(&folder, name, depth - 1))
}

/*
 * a(iiay): pictures at several sizes, each argb in network byte order;
 * the best one is written out as a png named after its pixels, so a new
 * picture gets a new file and the image cache never shows an old one
 */
fn pixmap(value: &Value, owner: &str, path: &str) -> Option<PathBuf> {
    let mut best: Option<(i64, i64, &[Value])> = None;

    for picture in value.list() {
        let [width, height, bytes] = picture.list() else {
            continue;
        };

        let (width, height) = (width.number() as i64, height.number() as i64);

        let bytes = bytes.list();

        if width <= 0 || height <= 0 || bytes.len() as i64 != width * height * 4 {
            continue;
        }

        // the smallest that is at least as big as wanted, or else the biggest there is
        let better = match best {
            None => true,
            Some((best_width, _, _)) if best_width < WANTED_SIZE => width > best_width,
            Some((best_width, _, _)) => width >= WANTED_SIZE && width < best_width,
        };

        if better {
            best = Some((width, height, bytes));
        }
    }

    let (width, height, bytes) = best?;

    let mut rgba = Vec::with_capacity(bytes.len());

    for pixel in bytes.chunks(4) {
        let channel = |index: usize| pixel[index].number() as u8;

        rgba.extend_from_slice(&[channel(1), channel(2), channel(3), channel(0)]);
    }

    let mut hasher = DefaultHasher::new();

    (owner, path, &rgba).hash(&mut hasher);

    let folder = format!("{}/.cache/amane/tray", home());

    let file = PathBuf::from(format!("{folder}/{:016x}.png", hasher.finish()));

    if !file.exists() {
        let _ = fs::create_dir_all(&folder);

        let partial = file.with_extension("partial");

        fs::write(&partial, png::encode(width as u32, height as u32, &rgba)).ok()?;
        fs::rename(&partial, &file).ok()?;
    }

    Some(file)
}
