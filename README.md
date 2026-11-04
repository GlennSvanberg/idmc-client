# IDMC Client

A desktop power-user client for Informatica IDMC metadata. This iteration does not connect to IDMC. It loads a Business 360 export (or a small hand-written model) and mock records, then opens a docking workspace: tables, record detail, and a hierarchy tree when the model has one.

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
cargo run -p idmc-client -- --help
```

`--values FILE` overlays code-list rows onto Reference 360 names. The Person launch does this automatically. Point `--export` at the Item package without `--values` and the Item lookups stay empty, because that overlay file names Person code lists.

The workspace is saved to `workspace.json` in the current directory (`--workspace` overrides the path). `Ctrl+S` writes it. Structural changes (close, dock, theme, reset) save as well.

## Keyboard

`Ctrl+K` opens the command palette. Every command the app can run is registered there, including commands that have no shortcut. `Up`/`Down` move the selection in the focused view, and move the highlight while the palette is open. `Ctrl+Alt+C` toggles the field chooser on a table. `Ctrl+Alt+L` chooses a source link. `Ctrl+Shift+L` toggles the light and dark theme.

Drag a tab onto another pane. The overlay shows whether the view will become a tab or split left, right, above, or below. Dividers resize. `Ctrl+Alt` and an arrow move the focused view.

## What you should see

The Person workspace opens as a table of people, the selected person's detail, and tabs for the first child collections (addresses, phones, emails). Selecting a person updates the detail and those tables. Address rows are the nested PostalAddress groups on that person, not a separate master file. Country codes render with the sample labels (Sweden, Norway, …).

The Item export has no mock records in this repo. It still loads as a model: entities, the description field group, the supplier reference, and the inverse. The hand-written sample under `mock/sample/` is a category tree with products and articles, which is what the tree view is for. Person has no hierarchy, so **New tree view** reports that.

## Layout of the repo

| Path | Role |
| --- | --- |
| `crates/model` | Entities, fields, lookups, relationships. JSON model and the IDMC export loader. |
| `crates/backend` | `Backend` plus `MockBackend`. Projects nested field groups into child rows. |
| `crates/layout` | Docking tree as data. No UI. |
| `crates/ui` | GPUI 0.2.2 application (`idmc-client`). |
| `mock/idmc-export/person` | Customer 360 Person export. Default. |
| `mock/idmc-export/item` | Product 360 Item and Supplier export. Second sample. |
| `mock/reference-values.json` | Sample code values. Not part of the Informatica export. |
| `mock/sample` | Hand-written model and records. |
| `docs/decisions.md` | UI toolkit choice and open questions. |
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
