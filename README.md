# Halo

A fast, light web browser for Windows, written in Rust.

Pages are drawn by **Blink** and run on the **V8** JavaScript engine (through the Microsoft Edge WebView2 runtime that ships with Windows 10 and 11). Everything around the page — tabs, address bar, themes, settings, downloads and extensions — is Halo's own code.

## Features

- **Dynamic interface** — a glass sidebar with vertical tabs (or classic top tabs), compact mode, pinned tabs, drag to reorder, tab search, and a command bar (Ctrl+K).
- **Themes and backgrounds** — 12 built-in themes including an animated one, your own picture as the background, accent colours and dimming. The new tab page uses the same look.
- **Google search by default** — or DuckDuckGo, Bing, Brave Search, Ecosia, Startpage, Qwant, Kagi, or your own engine.
- **Speed** — reopened tabs load only when you open them, unused tabs go to sleep to free memory (tabs playing sound stay awake), and GPU rasterization and zero-copy are switched on.
- **Extensions** — add them from the Chrome Web Store with “Add to Halo” in the address bar, or from a link, a .crx/.zip file, or an unpacked folder. One-click ad blocking with uBlock Origin Lite.
- **Downloads** — a downloads list (Ctrl+J) with open, show in folder, retry, and a choice of save location.
- **Settings** (Ctrl+,) — appearance, start-up and home page, search and address bar, tabs, privacy and security (HTTPS-only, pop-up blocking, clear on quit), site permissions (camera, microphone, location, notifications…), downloads, extensions, system (graphics acceleration, proxy, user agent) and about.
- **Animations** — smooth tab, panel and theme transitions, with a switch to turn them off.

## Requirements

- Windows 10 or 11 (64-bit)
- The Microsoft Edge WebView2 Runtime (already installed on Windows 11 and most Windows 10 PCs; otherwise get the “Evergreen” runtime from Microsoft)
- To build it yourself: [Rust](https://rustup.rs) 1.85 or newer with the MSVC toolchain

## Build and run

```
cargo build --release
target\release\halo-browser.exe
```

You can also pass addresses to open: `halo-browser.exe example.com`.

Every push to GitHub builds Halo automatically (see the **Actions** tab); the finished `halo-browser.exe` is attached to each run as a download.

## Keyboard shortcuts

| Keys | Action |
|---|---|
| Ctrl+T / Ctrl+W | New tab / close tab |
| Ctrl+Shift+T | Reopen closed tab |
| Ctrl+L, Alt+D, F6 | Address bar |
| Ctrl+K | Command bar |
| Ctrl+Tab, Ctrl+Shift+Tab | Next / previous tab |
| Ctrl+1…8, Ctrl+9 | Go to tab, last tab |
| Ctrl+D | Bookmark the page |
| Ctrl+J | Downloads |
| Ctrl+, | Settings |
| Ctrl+Shift+B | Compact sidebar |
| F5 / Ctrl+R | Reload |
| Ctrl+plus / minus / 0 | Zoom |

## Where your data lives

Settings, bookmarks, history, the session and downloads list are kept in `%LOCALAPPDATA%\HaloBrowser\state.json`; the page engine's data (cookies, cache, extensions) is in `%LOCALAPPDATA%\HaloBrowser\WebData`.

## Project layout

| Path | What it is |
|---|---|
| `src/main.rs` | Start-up, window, event loop, `--screenshot` option |
| `src/common.rs` | Shared types, address rules, search engines, built-in pages |
| `src/app.rs` | Tabs, page views, sleeping tabs, zoom, session |
| `src/features.rs` | Messages from the interface, downloads, extensions |
| `src/ext.rs` | Chrome Web Store download, unpacking, the engine's extension list |
| `src/store.rs` | Settings and saved data |
| `src/capture.rs` | Window screenshot (used by `--screenshot <file.png>`) |
| `src/ui.html` | The browser frame (sidebar, toolbar, panels) |
| `src/settings.html` | The Settings page (`halo://settings`) |
| `src/newtab.html`, `src/themes.js` | New tab page and themes |
| `MECHANICS.md`, `mechanics.json` | The rules and numbers the browser follows |

## Developer options

- `--screenshot <file.png>` — start, draw, save a picture of the window and exit (uses a separate temporary profile).
- `--theme <id>` and `--layout vertical|horizontal` — start with a given look.
- `--install-ext <store id>` — add a Chrome Web Store extension at start (for testing).
