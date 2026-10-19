use std::thread;

use amane::{
    Argument, Bus, Center, Color, Column, Full, Key, Keyboard, Layer, LayerWindow, Monitor,
    Padding, Parent, Pointer, Rectangle, Row, Service, Stack, Start, Text, Value, Weight, Widget,
    Zone, children,
};

use super::Item;
use crate::bar;
use crate::fonts;
use crate::settings::Settings;
use crate::theme::{self, Theme};

const MENU: &str = "com.canonical.dbusmenu";

const WIDTH: f32 = 270.0;
const RADIUS: f32 = 16.0;
const PADDING: f32 = 6.0;

const ROW_HEIGHT: f32 = 32.0;
const SEPARATOR_HEIGHT: f32 = 9.0;

// how far the menu stays from the screen's edge and the bar
const MARGIN: f32 = 10.0;

const CHECK_ICON: &str = "\u{f012c}";
const RADIO_ON_ICON: &str = "\u{f043e}";
const RADIO_OFF_ICON: &str = "\u{f043d}";
const SUBMENU_ICON: &str = "\u{f0142}";
const BACK_ICON: &str = "\u{f0141}";

#[derive(Clone, PartialEq)]
enum Toggle {
    None,
    Check(bool),
    Radio(bool),
}

// one line of a menu, as dbusmenu describes it
#[derive(Clone, PartialEq)]
struct Entry {
    id: i32,
    label: String,
    enabled: bool,
    separator: bool,
    toggle: Toggle,
    submenu: bool,
}

#[derive(Clone, PartialEq)]
struct Open {
    owner: String,
    path: String,

    // the monitor whose bar was clicked, the menu only shows there
    monitor: String,

    // where across the screen the icon was clicked, the menu centers under it
    anchor: f32,

    // the submenus walked into, each with its title, the top menu not counted
    trail: Vec<(i32, String)>,

    // none while the program is still answering
    entries: Option<Vec<Entry>>,
}

// the menu shown, if any, and the line the pointer is on
#[derive(Default, PartialEq)]
pub struct TrayMenu {
    open: Option<Open>,
    hovered: Option<i32>,
}

impl Service for TrayMenu {
    fn new() -> Self {
        Self::default()
    }

    // it only changes through input
    fn listen() {}
}

pub fn open(item: &Item, monitor: &str) {
    let Some(path) = item.menu.clone() else {
        return;
    };

    let menu = Open {
        owner: item.owner.clone(),
        path,
        monitor: String::from(monitor),
        anchor: bar::pointer_x(),
        trail: Vec::new(),
        entries: None,
    };

    TrayMenu::write().open = Some(menu.clone());

    load(menu, 0);
}

fn close() {
    let mut menu = TrayMenu::write();

    menu.open = None;
    menu.hovered = None;
}

/*
 * asks the program for one level of its menu; AboutToShow first, since
 * some programs only fill a menu in once it is about to be seen
 */
fn load(menu: Open, parent: i32) {
    thread::spawn(move || {
        let bus = Bus::session();

        bus.call(&menu.owner, &menu.path, MENU, "AboutToShow", &[Argument::Int(parent)]);

        let layout = bus.call(
            &menu.owner,
            &menu.path,
            MENU,
            "GetLayout",
            &[Argument::Int(parent), Argument::Int(1), Argument::TextList(Vec::new())],
        );

        // the revision, then (id, properties, children)
        let entries = match layout.list() {
            [_, root] => children(root),
            _ => Vec::new(),
        };

        let mut shown = TrayMenu::write();

        // the menu may have been closed or changed while the program answered
        if let Some(open) = shown.open.as_mut() {
            if open.owner == menu.owner && open.trail == menu.trail {
                open.entries = Some(entries);
            }
        }
    });
}

fn children(node: &Value) -> Vec<Entry> {
    let [_, _, children] = node.list() else {
        return Vec::new();
    };

    let mut entries = Vec::new();

    for child in children.list() {
        let [id, properties, _] = child.list() else {
            continue;
        };

        // a missing property has its default, so only a false that is there hides or disables
        let flag = |name: &str| !matches!(properties.get(name), Value::Bool(false));

        if !flag("visible") {
            continue;
        }

        let text = |name: &str| properties.get(name).text().to_string();

        let state = properties.get("toggle-state").number() == 1.0;

        let toggle = match text("toggle-type").as_str() {
            "checkmark" => Toggle::Check(state),
            "radio" => Toggle::Radio(state),
            _ => Toggle::None,
        };

        entries.push(Entry {
            id: id.number() as i32,
            label: mnemonic(&text("label")),
            enabled: flag("enabled"),
            separator: text("type") == "separator",
            toggle,
            submenu: text("children-display") == "submenu",
        });
    }

    entries
}

// "_Quit" is drawn as "Quit", and "__" stands for a real underscore
fn mnemonic(label: &str) -> String {
    label.replace("__", "\u{0}").replace('_', "").replace('\u{0}', "_")
}

fn choose(entry: &Entry) {
    let Some(mut menu) = TrayMenu::read().open.clone() else {
        return;
    };

    if !entry.enabled || entry.separator {
        return;
    }

    if entry.submenu {
        menu.trail.push((entry.id, entry.label.clone()));
        menu.entries = None;

        TrayMenu::write().open = Some(menu.clone());

        load(menu, entry.id);

        return;
    }

    let id = entry.id;

    thread::spawn(move || {
        Bus::session().call(
            &menu.owner,
            &menu.path,
            MENU,
            "Event",
            &[
                Argument::Int(id),
                Argument::from("clicked"),
                Argument::Variant(Box::new(Argument::Int(0))),
                Argument::Unsigned(0),
            ],
        );
    });

    close();
}

// back to the menu around the open submenu
fn back() {
    let Some(mut menu) = TrayMenu::read().open.clone() else {
        return;
    };

    menu.trail.pop();
    menu.entries = None;

    let parent = menu.trail.last().map_or(0, |(id, _)| *id);

    TrayMenu::write().open = Some(menu.clone());

    load(menu, parent);
}

fn hover(id: i32, inside: bool) {
    let mut menu = TrayMenu::write();

    if inside {
        menu.hovered = Some(id);
    } else if menu.hovered == Some(id) {
        menu.hovered = None;
    }
}

fn key_pressed(key: Key) {
    if key == Key::Escape {
        close();
    }
}

/*
 * the open menu under the clicked icon, over a clear layer that
 * closes it on a click anywhere else
 */
pub fn view(monitor: &Monitor) -> LayerWindow {
    let shown = TrayMenu::read();

    let open = shown.open.as_ref().filter(|open| open.monitor == monitor.name).cloned();

    let hovered = shown.hovered;

    drop(shown);

    let window = LayerWindow::new()
        .width(Full)
        .height(Full)
        .layer(Layer::Overlay)
        .space(Zone::Ignore)
        .namespace("tray-menu");

    let Some(open) = open else {
        return window.visible(false);
    };

    let theme = theme::current();

    let card = card(&theme, &open, hovered);

    let height = card_height(&open);

    let bottom = Settings::read().text("bar_position") == "bottom";

    // under the clicked icon, kept off the screen's edges
    let right = (monitor.width as f32 - WIDTH - MARGIN).max(MARGIN);

    let x = (open.anchor - WIDTH / 2.0).clamp(MARGIN, right);

    let y = if bottom {
        monitor.height as f32 - bar::height() - height - MARGIN
    } else {
        bar::height() + MARGIN
    };

    let catcher = Rectangle::new()
        .width(Parent)
        .height(Parent)
        .fill(Color::TRANSPARENT)
        .on_click(|_| close());

    window
        .keyboard(Keyboard::Exclusive)
        .on_key(key_pressed)
        .child(Stack::new(children![catcher, card.translate(x, y)]).width(Parent).height(Parent))
}

fn card_height(open: &Open) -> f32 {
    let back = if open.trail.is_empty() { 0.0 } else { ROW_HEIGHT + SEPARATOR_HEIGHT };

    let entries = match &open.entries {
        Some(entries) if !entries.is_empty() => entries
            .iter()
            .map(|entry| if entry.separator { SEPARATOR_HEIGHT } else { ROW_HEIGHT })
            .sum(),
        _ => ROW_HEIGHT,
    };

    back + entries + PADDING * 2.0
}

fn card(theme: &Theme, open: &Open, hovered: Option<i32>) -> Rectangle {
    let inner = WIDTH - PADDING * 2.0;

    let mut rows: Vec<Box<dyn Widget>> = Vec::new();

    if let Some((_, title)) = open.trail.last() {
        rows.push(Box::new(back_row(theme, title, inner, hovered == Some(-1))));
        rows.push(Box::new(separator(theme, inner)));
    }

    match &open.entries {
        None => rows.push(Box::new(message(theme, "Loading…", inner))),
        Some(entries) if entries.is_empty() => rows.push(Box::new(message(theme, "Empty menu", inner))),

        Some(entries) => {
            for entry in entries {
                if entry.separator {
                    rows.push(Box::new(separator(theme, inner)));
                } else {
                    rows.push(Box::new(row(theme, entry, inner, hovered == Some(entry.id))));
                }
            }
        }
    }

    Rectangle::new()
        .width(WIDTH)
        .height(card_height(open))
        .radius(RADIUS)
        .fill(theme.surface)
        .border(1.0, theme.border)
        .padding(PADDING)
        .on_click(|_| {})
        .child(Column::new(rows))
}

fn row(theme: &Theme, entry: &Entry, width: f32, hovered: bool) -> Rectangle {
    let color = if entry.enabled { theme.text } else { theme.muted_text };

    let mark = match entry.toggle {
        Toggle::Check(true) => CHECK_ICON,
        Toggle::Radio(true) => RADIO_ON_ICON,
        Toggle::Radio(false) => RADIO_OFF_ICON,
        _ => "",
    };

    let mark = Rectangle::new()
        .width(18.0)
        .height(ROW_HEIGHT)
        .align_child(Center, Center)
        .child(Text::new(mark).size(14.0).font(fonts::NERD).tight().color(theme.accent));

    let label_width = width - 18.0 - 18.0 - 20.0;

    let label = Rectangle::new()
        .width(label_width)
        .height(ROW_HEIGHT)
        .align_child(Start, Center)
        .child(Text::new(entry.label.as_str()).size(13.0).font(fonts::BODY).weight(Weight::Medium).color(color).elide());

    let arrow = if entry.submenu { SUBMENU_ICON } else { "" };

    let arrow = Rectangle::new()
        .width(18.0)
        .height(ROW_HEIGHT)
        .align_child(Center, Center)
        .child(Text::new(arrow).size(14.0).font(fonts::NERD).tight().color(theme.secondary_text));

    let fill = if hovered && entry.enabled {
        theme::mix(theme.surface, theme.text, 0.08)
    } else {
        Color::TRANSPARENT
    };

    let id = entry.id;
    let chosen = entry.clone();

    let mut line = Rectangle::new()
        .width(width)
        .height(ROW_HEIGHT)
        .radius(10.0)
        .fill(fill)
        .padding(Padding {
            top: 0.0,
            right: 6.0,
            bottom: 0.0,
            left: 6.0,
        })
        .on_hover(move |inside| hover(id, inside))
        .child(Row::new(children![mark, label, arrow]).gap(4.0).align(Center));

    if entry.enabled {
        line = line.cursor(Pointer).on_click(move |_| choose(&chosen));
    }

    line
}

fn back_row(theme: &Theme, title: &str, width: f32, hovered: bool) -> Rectangle {
    let fill = if hovered {
        theme::mix(theme.surface, theme.text, 0.08)
    } else {
        Color::TRANSPARENT
    };

    let content = Row::new(children![
        Text::new(BACK_ICON).size(14.0).font(fonts::NERD).tight().color(theme.secondary_text),
        Text::new(title).size(13.0).font(fonts::BODY).weight(Weight::SemiBold).color(theme.secondary_text).elide(),
    ])
    .gap(8.0)
    .align(Center);

    Rectangle::new()
        .width(width)
        .height(ROW_HEIGHT)
        .radius(10.0)
        .fill(fill)
        .padding(Padding {
            top: 0.0,
            right: 8.0,
            bottom: 0.0,
            left: 8.0,
        })
        .cursor(Pointer)
        .on_hover(|inside| hover(-1, inside))
        .on_click(|_| back())
        .align_child(Start, Center)
        .child(content)
}

fn separator(theme: &Theme, width: f32) -> Rectangle {
    Rectangle::new()
        .width(width)
        .height(SEPARATOR_HEIGHT)
        .align_child(Center, Center)
        .child(Rectangle::new().width(width - 12.0).height(1.0).fill(theme.border))
}

fn message(theme: &Theme, text: &str, width: f32) -> Rectangle {
    Rectangle::new()
        .width(width)
        .height(ROW_HEIGHT)
        .padding(Padding {
            top: 0.0,
            right: 8.0,
            bottom: 0.0,
            left: 8.0,
        })
        .align_child(Start, Center)
        .child(Text::new(text).size(13.0).font(fonts::BODY).color(theme.muted_text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_mnemonics() {
        assert_eq!(mnemonic("_Quit"), "Quit");
        assert_eq!(mnemonic("snake__case"), "snake_case");
    }
}
