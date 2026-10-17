mod item;
pub mod menu;
mod png;

use std::thread;

use amane::{Argument, Bus, Service, Value};

pub use item::Item;

// where programs announce their tray icons, under the name every desktop uses
const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";

// a program that only says its path is found here by default
const DEFAULT_ITEM_PATH: &str = "/StatusNotifierItem";

// the item signals that mean something it shows changed
const ITEM_SIGNALS: [&str; 5] = ["NewIcon", "NewAttentionIcon", "NewStatus", "NewTitle", "NewMenu"];

// what qt asks for before it trusts the watcher, so its programs show up at all
const INTROSPECTION: &str = r#"<!DOCTYPE node PUBLIC "-//freedesktop//DTD D-BUS Object Introspection 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd">
<node>
  <interface name="org.kde.StatusNotifierWatcher">
    <method name="RegisterStatusNotifierItem"><arg name="service" type="s" direction="in"/></method>
    <method name="RegisterStatusNotifierHost"><arg name="service" type="s" direction="in"/></method>
    <property name="RegisteredStatusNotifierItems" type="as" access="read"/>
    <property name="IsStatusNotifierHostRegistered" type="b" access="read"/>
    <property name="ProtocolVersion" type="i" access="read"/>
    <signal name="StatusNotifierItemRegistered"><arg type="s"/></signal>
    <signal name="StatusNotifierItemUnregistered"><arg type="s"/></signal>
    <signal name="StatusNotifierHostRegistered"/>
    <signal name="StatusNotifierHostUnregistered"/>
  </interface>
  <interface name="org.freedesktop.DBus.Properties">
    <method name="Get"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="out"/></method>
    <method name="GetAll"><arg type="s" direction="in"/><arg type="a{sv}" direction="out"/></method>
  </interface>
  <interface name="org.freedesktop.DBus.Introspectable">
    <method name="Introspect"><arg type="s" direction="out"/></method>
  </interface>
</node>
"#;

/*
 * the system tray: the shell is the status notifier watcher and host, so
 * programs like telegram, discord or steam hand it their icons and menus
 * over d-bus; items are kept in the order they arrived
 */
#[derive(Default, PartialEq)]
pub struct Tray {
    pub items: Vec<Item>,
}

impl Service for Tray {
    fn new() -> Self {
        Self::default()
    }

    fn listen() {
        let bus = Bus::session();

        // every watch is set up before the name is taken, so the first registrations aren't missed
        let watcher = bus.methods(WATCHER_PATH, WATCHER);
        let properties = bus.methods(WATCHER_PATH, "org.freedesktop.DBus.Properties");
        let introspection = bus.methods(WATCHER_PATH, "org.freedesktop.DBus.Introspectable");
        let owners = bus.signals("org.freedesktop.DBus", "NameOwnerChanged");

        let mut item_signals = Vec::new();

        for interface in item::INTERFACES {
            for name in ITEM_SIGNALS {
                item_signals.push(bus.signals(interface, name));
            }
        }

        if !bus.own(WATCHER) {
            eprintln!("amane: another program is the tray watcher already, the tray stays empty");

            return;
        }

        thread::spawn(move || {
            for method in properties {
                let value = match method.arguments().get(1).map(Value::text) {
                    Some("RegisteredStatusNotifierItems") => registered(),
                    Some("IsStatusNotifierHostRegistered") => Argument::Bool(true),
                    Some("ProtocolVersion") => Argument::Int(0),

                    // GetAll, which only says the interface
                    _ if method.name() == "GetAll" => {
                        let mut all = std::collections::BTreeMap::new();

                        all.insert(String::from("RegisteredStatusNotifierItems"), registered());
                        all.insert(String::from("IsStatusNotifierHostRegistered"), Argument::Bool(true));
                        all.insert(String::from("ProtocolVersion"), Argument::Int(0));

                        method.reply(&[Argument::Map(all)]);

                        continue;
                    }

                    _ => Argument::Text(String::new()),
                };

                method.reply(&[Argument::Variant(Box::new(value))]);
            }
        });

        thread::spawn(move || {
            for method in introspection {
                method.reply(&[Argument::from(INTROSPECTION)]);
            }
        });

        // a program that quits takes its icons with it
        thread::spawn(move || {
            for signal in owners {
                let [name, _, new_owner] = signal.arguments() else {
                    continue;
                };

                if new_owner.text().is_empty() && name.text().starts_with(':') {
                    let gone = name.text().to_string();

                    let mut tray = Tray::write();

                    tray.items.retain(|item| item.owner != gone);
                }
            }
        });

        for signals in item_signals {
            thread::spawn(move || {
                for signal in signals {
                    refresh(signal.sender(), signal.path());
                }
            });
        }

        // a watcher that appears is announced, so programs started before the shell register again
        bus.emit(WATCHER_PATH, WATCHER, "StatusNotifierHostRegistered", &[]);

        for method in watcher {
            match method.name() {
                "RegisterStatusNotifierItem" => {
                    let service = method.arguments().first().map(Value::text).unwrap_or_default().to_string();

                    let sender = method.sender().to_string();

                    // answered before anything is asked of the program, which may be waiting for it
                    method.reply(&[]);

                    thread::spawn(move || register(&service, &sender));
                }

                _ => method.reply(&[]),
            }
        }
    }
}

/*
 * "org.kde.StatusNotifierItem-123-1", ":1.42" or just "/org/ayatana/..."
 * for a path on the caller itself; the item is kept by its owner's unique
 * name, which is what its signals come from
 */
fn register(service: &str, sender: &str) {
    let (name, path) = if service.starts_with('/') {
        (sender.to_string(), service.to_string())
    } else {
        (service.to_string(), String::from(DEFAULT_ITEM_PATH))
    };

    let owner = if name.starts_with(':') {
        name.clone()
    } else {
        let owner = Bus::session().call(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "GetNameOwner",
            &[Argument::from(name.as_str())],
        );

        owner.text().to_string()
    };

    if owner.is_empty() {
        return;
    }

    refresh(&owner, &path);

    Bus::session().emit(
        WATCHER_PATH,
        WATCHER,
        "StatusNotifierItemRegistered",
        &[Argument::from(format!("{owner}{path}"))],
    );
}

// reads the item again, adding it if it is new; nothing happens for a signal from something else
fn refresh(owner: &str, path: &str) {
    let known = Tray::read().items.iter().any(|item| item.owner == owner && item.path == path);

    let Some(fresh) = item::read(owner, path) else {
        if known {
            Tray::write().items.retain(|item| !(item.owner == owner && item.path == path));
        }

        return;
    };

    let mut tray = Tray::write();

    match tray.items.iter_mut().find(|item| item.owner == owner && item.path == path) {
        Some(item) if *item != fresh => *item = fresh,
        Some(_) => {}
        None => tray.items.push(fresh),
    }
}

fn registered() -> Argument {
    let items = Tray::read()
        .items
        .iter()
        .map(|item| format!("{}{}", item.owner, item.path))
        .collect();

    Argument::TextList(items)
}

// what each kind of click does: activate, the menu, or the middle button's second action
pub fn click(item: &Item, button: amane::Button, monitor: &str) {
    let item = item.clone();

    match button {
        amane::Button::Left if !item.only_menu => {
            thread::spawn(move || {
                call(&item, "Activate", &[Argument::Int(0), Argument::Int(0)]);
            });
        }

        amane::Button::Middle => {
            thread::spawn(move || {
                call(&item, "SecondaryActivate", &[Argument::Int(0), Argument::Int(0)]);
            });
        }

        _ => {
            if item.menu.is_some() {
                menu::open(&item, monitor);
            } else {
                thread::spawn(move || {
                    call(&item, "ContextMenu", &[Argument::Int(0), Argument::Int(0)]);
                });
            }
        }
    }
}

pub fn scroll(item: &Item, scroll: amane::Scroll) {
    let item = item.clone();

    let (delta, orientation) = if scroll.y != 0.0 {
        (scroll.y, "vertical")
    } else {
        (scroll.x, "horizontal")
    };

    thread::spawn(move || {
        call(&item, "Scroll", &[Argument::Int(delta.round() as i32), Argument::from(orientation)]);
    });
}

fn call(item: &Item, method: &str, arguments: &[Argument]) -> Value {
    Bus::session().call(&item.owner, &item.path, item.interface, method, arguments)
}
