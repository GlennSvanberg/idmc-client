# IDMC Client

A desktop power-user client for Informatica IDMC metadata. It loads a Business 360 export (or a small hand-written model) and either mock records or a live Business 360 org. The workspace is tables, record detail, and a hierarchy tree when the model has one.

The default model is the Customer 360 **Person** export. Product 360 **Item** is a second sample, loaded with `--export`, so the same loader is exercised on a different package. Nothing in the UI hardcodes entity or field names. The field chooser, the views you can open, and the source-link menu all come from the model.

## Run

From the repository root, with a stable Rust toolchain:

```bash
cargo run -p idmc-client
```

That loads `mock/idmc-export/person`, sample code-list values from `mock/reference-values.json`, and records from `mock/data.json`.

```bash
cargo run -p idmc-client -- --export mock/idmc-export/item
cargo run -p idmc-client -- --model mock/sample/model.json --data mock/sample/data.json
cargo run -p idmc-client --release -- --help
```

Copy `.env.example` to `.env` for a live org. `.env` is gitignored. The process environment wins over the file, and nothing in the client prints the password or the session id.

```bash
cargo run -p idmc-client -- --backend live
cargo run -p idmc-client -- --check-login
cargo run -p idmc-client -- --backend live --debug-dump /tmp/idmc-debug.log
```

`--backend mock` is the default, including when `.env` contains credentials but `IDMC_BACKEND` is unset or `mock`. `--check-login` logs in with API v3 (`POST {login}/saas/public/core/v3/login`) and falls back to v2 (`POST {login}/ma/api/v2/user/login`), then runs one search. Exit status is 0 when that works.

Live data uses `IDS-SESSION-ID`. HTTP calls time out after 15 seconds to connect and 60 seconds to read. The MDM host is the login `baseApiUrl` (v3) or `serverUrl` (v2) with `/saas` removed and `-mdm` added to the pod label (`https://usw1.dmp-us.informaticacloud.com/saas` becomes `https://usw1-mdm.dmp-us.informaticacloud.com`). A master table shows the search hit count immediately and fetches 500-row windows as you scroll, sorted by the server, with at most four requests in flight. A previous window is read from the local cache (`IDMC_CACHE`, else the user cache directory) and refreshed in the background. `--check-login` prints each step as it happens. A master record that is missing nested field groups is read with `GET /business-entity/public/api/v1/entity/{businessEntity}/{businessId}`. A 401 logs in again once. A 429 waits for `Retry-After` or backs off, up to 30 seconds. The model is still the export package. There is no general metadata API. Request and response shapes are the pages under `docs/idmc-api/`. How the windowed list relates to an Egress snapshot is `docs/performance.md`.

`cargo run -p idmc-client --release` is the build to use against a real org. Debug builds compile dependencies at `opt-level = 2` so the UI is usable, and still slower than release.

The tab bar holds several workspaces. `+` or Ctrl+K “New workspace from template” adds Browse, Search & inspect, or Data quality, built from the entities in the loaded model. Rename, duplicate, and close are in the same palette. Typing a record name in Ctrl+K searches every master entity (the Search API when live, local records when mock) and opens the match in a detail view.

`--values FILE` overlays code-list rows onto Reference 360 names. The Person launch does this automatically. Point `--export` at the Item package without `--values` and the Item lookups stay empty, because that overlay file names Person code lists. Live lookup objects `{Code, Name}` supply a display name when the overlay does not have that code.

Workspaces are saved to `workspace.json` in the current directory (`--workspace` overrides the path). `Ctrl+S` writes them. Structural changes (close, dock, theme, reset, workspace tabs) save as well. A file written by an older build, with one layout and no tabs, opens as a single Browse workspace.

## Keyboard

`Ctrl+K` opens the command palette. Every command the app can run is registered there, including commands that have no shortcut. `Up`/`Down` move the highlight in the palette and the selection in the focused view. Left, right, Home, End, Backspace, Delete, and Ctrl+A edit the palette text. Space inserts a space. **Open view** lists a table, detail, or tree for every entity in the model, including child collections. `Ctrl+Alt+C` toggles the field chooser on a table. `Ctrl+Alt+L` chooses a source link. `Ctrl+Shift+L` toggles the light and dark theme.

Drag a tab onto another pane. The overlay shows whether the view will become a tab or split left, right, above, or below. Dropping the active tab of a group onto that group's right edge splits it out. Dividers resize. `Ctrl+Alt` and an arrow move the focused view.

## What you should see

The Person workspace opens as a table of people, the selected person's detail, and tabs for the first child collections (addresses, phones, emails). Selecting a person updates the detail and those tables. Address rows are the nested PostalAddress groups on that person, not a separate master file. Country codes render with the sample labels (Sweden, Norway, …).

**Show in graph** (the Graph button on a table or detail, or Ctrl+K) opens a dockable graph of the selected record: the person, a node per child collection, then the child rows. A reference field becomes a stub until that record is fetched. Double-click a node to expand it in place. With nothing selected, **Open Graph view** draws the schema instead. The view holds at most 400 nodes. Wheel zoom stays between 0.25 and 2.5, dragging the background pans, dragging a node pins it, `F` fits, and Escape clears the selection.

The Item export has no mock records in this repo. It still loads as a model: entities, the description field group, the supplier reference, and the inverse. The hand-written sample under `mock/sample/` is a category tree with products and articles, which is what the tree view is for. Person has no hierarchy, so **New tree view** reports that.

## Layout of the repo

| Path | Role |
| --- | --- |
| `crates/model` | Entities, fields, lookups, relationships. JSON model and the IDMC export loader. |
| `crates/backend` | `Backend`, `MockBackend`, and the live Business 360 client. |
| `crates/layout` | Docking tree as data. No UI. |
| `crates/ui` | GPUI 0.2.2 application (`idmc-client`). |
| `mock/idmc-export/person` | Customer 360 Person export. Default. |
| `mock/idmc-export/item` | Product 360 Item and Supplier export. Second sample. |
| `mock/reference-values.json` | Sample code values. Not part of the Informatica export. |
| `mock/sample` | Hand-written model and records. |
| `docs/decisions.md` | UI toolkit choice and open questions. |
| `docs/idmc-api/` | Informatica pages for login, session, headers, search, and business-entity reads. |
| `third_party/xattr` | Vendored xattr 0.2.3 with a Linux `ENODATA` fix so GPUI links against current libc. |

## Linux

GPUI links Wayland, X11, and Vulkan. On Ubuntu:

```bash
sudo apt-get install -y pkg-config g++ cmake \
  libwayland-dev libxkbcommon-dev libxkbcommon-x11-dev libvulkan-dev \
  libxcb-shape0-dev libxcb-xfixes0-dev libxcb-xkb-dev libx11-xcb-dev libssl-dev
```

Rust 1.99's bundled lld looks for `libstdc++.so` on the default library path. The dev symlink lives under the GCC directory. If the link fails with `unable to find library -lstdc++`:

```bash
sudo ln -sfn "$(g++ -print-file-name=libstdc++.so)" /usr/lib/x86_64-linux-gnu/libstdc++.so
```

## Windows

Visual Studio Build Tools with the MSVC toolchain and the Windows SDK are enough. From the repository root:

```bash
cargo run -p idmc-client
```

No extra C libraries. The Linux `libstdc++` note does not apply.
