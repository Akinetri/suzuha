# What this fork changes

A fork of [MystiaFin/suzuha](https://github.com/MystiaFin/suzuha). Everything below is on top of upstream `main` (`62118f3`).

> [!NOTE]
> Needs the library from [Akinetri/amane](https://github.com/Akinetri/amane) (branch `local`): it adds `Method::sender()`, `Apps::icon_path()`, the audio device list and text paste. Install its CLI with `cargo install --path cli`.

## Added

- **System tray** on the bar: a StatusNotifierWatcher of its own, app icons (including raw PNG pixmaps), dbusmenu menus that open under the clicked icon (`src/tray/`)
- **Bar pills**: keyboard layout, coffee (keeps the screen awake through `systemd-inhibit`), do-not-disturb
- **Calendar**: iCal links (e.g. the Google Calendar secret address) with recurring events, moved / cancelled occurrences and time zones; set under **Settings → Calendar**, shown as dots in the utility calendar and as an "Upcoming" desktop card (`src/calendar/`)
- **Desktop cards**: a music player, and synced lyrics from lrclib joined under it (cached in `~/.cache/amane/lyrics`, toggle `widget_lyrics` under **Settings → Floating**); the pair only sits in a screen corner and widens for the song's lines
- **Sound page** in the utility panel to pick the output device and microphone (`amane ipc call utility sound`)
- **Theme integrations**: Qt / KDE, Alacritty, fish, Firefox (Pywalfox), Telegram (freshGram, AyuGram), Legcord
- **IPC**: `stopwatch`, `coffee`, `dnd`

## Changed

- **Pomodoro → stopwatch**: one session counting up instead of fixed intervals; sessions go to `~/.local/state/amane/stopwatch-history`, with today / this week / all-time stats
- **Bar sizes** scale with the bar height
- **Vesktop**: a full theme file (Midnight in the shell's colors) at `~/.config/vesktop/themes/amane.theme.css` instead of a block in quick CSS; the old block is removed from quick CSS
- **README** lists the new features, dependencies (`systemd-inhibit`, `zip`, `pywalfox`, `kwriteconfig6`) and IPC calls

## Removed

- Pomodoro timer, along with its sounds and notifications
