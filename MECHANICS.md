# Halo browser — mechanics sheet

Rebuild of Thorium (Chromium 138.0.7204.303 fork) as a Rust browser shell.
Pages run on Blink + V8 through the WebView2 runtime; the frame is written in Rust (tao + wry) with an HTML/CSS interface.

## What the original does (evidence)

| Fact | Where it comes from |
|---|---|
| `thorium.exe` is only a launcher: it sets the working folder, loads `chrome.dll` with `LoadLibraryExW` from the `138.0.7204.303` folder and calls its `ChromeMain` export. All browser behaviour lives in `chrome.dll`. | function `0x140028e80` (main_dll_loader_win.cc), evidence ev_1894ce70… |
| Version `138.0.7204.303` | string in function `0x140028e80` |
| `chrome.dll` is 272 MB, above the analysis tools' 256 MiB limit; it was searched as raw bytes instead (not decompiled). | tool refusal (over_limit) |
| Google search template `{google:baseURL}search?q={searchTerms}&…` | byte scan of chrome.dll |
| Other engines carried: DuckDuckGo `duckduckgo.com/?q={searchTerms}` and more | byte scan of chrome.dll |
| New tab page is `chrome://newtab` | byte scan of chrome.dll |
| Thorium-specific options: "Thorium Rectangular Tabs UI", "Thorium Top Bar Icons", "Thorium Custom DNS Config", "Thorium Easter Eggs" | byte scan of chrome.dll |
| Engine switches compiled in: `enable-gpu-rasterization`, `enable-zero-copy`, `enable-parallel-downloading` | byte scan of chrome.dll |
| Zoom steps: Chromium's preset zoom factors 25 %…500 % | Chromium standard (estimated: the table could not be located in chrome.dll) |

## Rules the rebuild follows

- **Address bar**: text with a scheme opens as is; a single word that looks like a host (`name.tld`, `localhost`, an IPv4 address, optional port/path) opens as `https://` (`http://` for localhost/IP); anything else is searched.
- **Default search engine: Google** — `https://www.google.com/search?q=<terms>` (spaces → `+`). Choices: Google, DuckDuckGo, Bing, Brave Search, Ecosia.
- **Tabs**: new tabs open right after the current one. Closing the current tab selects the tab that takes its place (the one to its right, else the last). Closing the last tab closes the window (as Chromium). Up to 25 closed tabs can be reopened (Ctrl+Shift+T). Ctrl+1…8 jump to that tab, Ctrl+9 to the last one. Double-click or right-click a tab pins it; pinned tabs gather at the start. Drag to reorder.
- **Links that open a new window** become a foreground tab; sized pop-ups (sign-in windows) keep their own window.
- **Zoom**: steps 25, 33, 50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200, 250, 300, 400, 500 %.
- **Speed**:
  - Restored tabs come back asleep and only load when opened (fast start).
  - Sleeping tabs: a tab not viewed for 30 minutes (setting: never/5/15/30/60) is unloaded; the visible tab and tabs playing sound are never put to sleep.
  - The same Blink switches as the original: GPU rasterization, zero-copy, parallel downloading.
- **Interface** (dynamic, sidebar-first): vertical tab sidebar (248 px, compact 64 px with Ctrl+Shift+B) or classic top tabs; glass toolbar over the theme background; command bar (Ctrl+K) for tabs, commands and themes; address suggestions from open tabs, bookmarks and history (history keeps up to 2000 sites).
- **Themes and backgrounds**: 12 built-in themes (Aurora, Dynamic (animated), Storm, Sunset, Ocean, Forest, Midnight, Graphite, Rose, Lavender, Sand, Paper), your own picture as background (up to 25 MB) with light/dark text choice, accent colour, background dimming 0–70 %. The new tab page uses the same background.
- **Animations** (Settings → Motion, on by default): new tabs open a gap and glide in without bouncing (300 ms; tab rows are updated in place so the motion is never cut short), new tab pages show the theme colour instead of a white flash and fade in as one piece, closed tabs shrink away (170 ms), the chosen tab's accent bar grows, panels, the command bar and suggestions ease open with staggered rows, theme backgrounds cross-fade (700 ms), buttons react on hover/press, the logo spins faster while pages load, the loading bar shimmers, and the new tab page fades in with floating accent glows and a blinking clock colon. Turning it off stops all motion.
- **New tab page**: clock and greeting, search box (current engine), up to 8 shortcuts (bookmarks first, then most visited sites, one per site), Customize button.

## Settings page (halo://settings, Ctrl+,)

Sections in the style of privacy-first Chromium browsers such as Helium (the exact Helium option list was not available to check; every option below is implemented, values estimated):

- **Appearance**: theme, side/top tabs, compact sidebar, toolbar density (comfortable/compact), corners (rounded/square), bookmarks bar, home button, animations, default page zoom (67–200 %), new tab clock and shortcuts.
- **On start-up & home**: continue where I left off (default) / new tab / home page; home page address (empty = new tab page).
- **Search & address bar**: Google (default), DuckDuckGo, Bing, Brave Search, Ecosia, Startpage, Qwant, Kagi, or a custom engine (address with `%s`); suggestions on/off; full addresses on/off (default off: `https://` and `www.` hidden until you click the address bar).
- **Tabs**: new tabs next to the current one (default) or at the end; close window with last tab (default on, as Chromium); sleeping tabs (never/5/15/30/60/120 min); sleep others now.
- **Privacy & security**: one-click ad and tracker blocking (uBlock Origin Lite from the Chrome Web Store), HTTPS-only (public `http://` addresses are reopened over `https://`; localhost, private IPs and single-word hosts are left alone), block pop-up windows, JavaScript, autoplay, form autofill (new tabs), clear history on quit, clear cookies and site data on quit (done at the next start, before any site loads), clear now buttons.
- **Site permissions**: camera, microphone, location, notifications, clipboard reading, multiple downloads — Ask (engine prompt) / Allow / Block, applied live.
- **Downloads**: list (newest first, up to 200) with bytes received, open, show in folder, retry, remove, clear; location (default the Windows Downloads folder), ask where to save each file, highlight the downloads button. Files that already exist get " (1)", " (2)"… A blank tab that only started a download closes itself.
- **Extensions**: add from a Chrome Web Store link or id (downloaded as a CRX for Chromium 138.0.7204.303, unpacked, loaded into the engine profile), "Add to Halo" button in the address bar on Web Store pages, install from a .crx/.zip, load unpacked folder, on/off, remove, open pop-up or options page in a tab, popular list. The engine's built-in Microsoft helpers are hidden.
- **System** (after restart): graphics acceleration, allow extensions, proxy (`http://host:port` or `socks5://host:port`); custom user agent for new tabs; Restart now.
- **About**: version, engine (Chromium version read from the engine), data folder, reset settings.
