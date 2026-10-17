use amane::{
    Center, Color, Full, Monitor, Parent, Pointer, Rectangle, Row, Service, Stack, Start, Text,
    Weight, Widget, Workspace, Workspaces,
};

use super::{motion, px, star};
use crate::fonts;
use crate::overlay::Overlay;
use crate::settings::Settings;
use crate::theme::Theme;

// the logo and the inactive dot, the active star is drawn in star.rs
const LOGO: &str = "\u{f1105}";
const INACTIVE: &str = "\u{f444}";

// every workspace is a 26px slot, 4px apart, inside a 32px pill
const SLOT: f32 = 26.0;
const SLOT_GAP: f32 = 4.0;
const STRIP_HEIGHT: f32 = 32.0;
const STRIP_PADDING: f32 = 4.0;

// with the row's 10px gap this keeps 20px from the screen edge
const EDGE: f32 = 10.0;

pub fn view(monitor: &Monitor, theme: &Theme, width: f32) -> Row {
    let workspaces = Workspaces::read();

    let mut own: Vec<&Workspace> = Vec::new();

    // each bar only shows the workspaces of its own monitor
    for workspace in workspaces.list() {
        if workspace.output() == Some(monitor.name.as_str()) {
            own.push(workspace);
        }
    }

    own.sort_by_key(|workspace| workspace.index());

    let logo = Rectangle::new()
        .width(px(30.0))
        .height(px(STRIP_HEIGHT))
        .cursor(Pointer)
        .on_click(|_| Overlay::toggle_power_menu())
        .align_child(Center, Center)
        .child(Text::new(LOGO).size(px(28.0)).font(fonts::NERD).color(theme.accent));

    let name = Text::new(active_name(&own))
        .size(px(14.0))
        .font(fonts::BODY)
        .weight(Weight::Medium)
        .color(theme.text);

    let settings = Settings::read();

    let mut items: Vec<Box<dyn Widget>> = vec![Box::new(Rectangle::new().width(px(EDGE)).height(px(1.0)))];

    if settings.flag("bar_logo") {
        items.push(Box::new(logo));
    }

    if settings.flag("bar_workspaces") {
        items.push(Box::new(strip(monitor, &own, theme, settings.text("workspace_style"))));
    }

    if settings.flag("bar_workspace_name") {
        items.push(Box::new(name));
    }

    Row::new(items)
        .width(width)
        .height(Parent)
        .gap(px(10.0))
        .justify(Start)
        .align(Center)
}

// the dots or numbers, with the sliding accent highlight drawn over them
fn strip(monitor: &Monitor, workspaces: &[&Workspace], theme: &Theme, style: &str) -> Rectangle {
    let mut slots: Vec<Box<dyn Widget>> = Vec::new();

    let mut active = 0;

    for (position, workspace) in workspaces.iter().enumerate() {
        if workspace.active() {
            active = position;
        }

        slots.push(Box::new(slot(workspace, theme, style)));
    }

    let count = workspaces.len() as f32;

    let gaps = (count - 1.0).max(0.0);

    let content = count * px(SLOT) + gaps * px(SLOT_GAP);

    let (offset, rotation) = motion::highlight(&monitor.name, active, px(SLOT + SLOT_GAP));

    let highlight = Rectangle::new()
        .width(px(SLOT))
        .height(px(SLOT))
        .radius(Full)
        .fill(theme.accent)
        .translate(offset, 0.0);

    let active_index = workspaces.get(active).map_or(1, |workspace| workspace.index() as usize);

    // the pill style spins a star in the highlight, numbers repeat the active one
    let highlight: Box<dyn Widget> = match style {
        "numbers" => Box::new(
            highlight
                .align_child(Center, Center)
                .child(number(active_index, theme.on_accent)),
        ),
        "dots" => Box::new(highlight),
        _ => Box::new(highlight.child(star::view(px(SLOT), rotation, theme.on_accent))),
    };

    // the highlight slides over the dots, which stay where they are
    let layers = Stack::new(vec![Box::new(Row::new(slots).gap(px(SLOT_GAP)).align(Center)), highlight]);

    Rectangle::new()
        .width(content + px(STRIP_PADDING) * 2.0)
        .height(px(STRIP_HEIGHT))
        .radius(Full)
        .fill(theme.selected_surface)
        .align_child(Center, Center)
        .child(layers)
}

// a small dot or its number, the highlight covers the active one
fn slot(workspace: &Workspace, theme: &Theme, style: &str) -> Rectangle {
    let id = workspace.id();

    let color = if workspace.urgent() {
        theme.danger
    } else if workspace.active() {
        Color::TRANSPARENT
    } else {
        theme.muted_text
    };

    let mark = if style == "numbers" {
        number(workspace.index() as usize, color)
    } else {
        Text::new(INACTIVE).size(px(11.0)).font(fonts::SYMBOLS).color(color)
    };

    Rectangle::new()
        .width(px(SLOT))
        .height(px(SLOT))
        .cursor(Pointer)
        .on_click(move |_| Workspaces::focus(id))
        .align_child(Center, Center)
        .child(mark)
}

fn number(index: usize, color: Color) -> Text {
    Text::new(index.to_string())
        .size(px(12.0))
        .font(fonts::BODY)
        .weight(Weight::Bold)
        .tight()
        .color(color)
}

// the workspace's own name, or "Workspace 2" when it has none
fn active_name(workspaces: &[&Workspace]) -> String {
    for workspace in workspaces {
        if !workspace.active() {
            continue;
        }

        let index = workspace.index().to_string();

        return match workspace.name() {
            Some(name) if !name.is_empty() && name != index => String::from(name),
            _ => format!("Workspace {index}"),
        };
    }

    String::from("Desktop")
}
