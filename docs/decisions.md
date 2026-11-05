# Decisions

## UI toolkit: plain GPUI 0.2.2

The client stays on `gpui` 0.2.2 from crates.io, the same crate the P360 client uses. Docking is the layout tree in `crates/layout` (copied and adapted from that client) drawn with ordinary GPUI divs.

**gpui-kit 0.9.0** was rejected. It depends on `gpui-pre` 0.3.11, a different package from crates.io `gpui` 0.2.2. Taking it would fork the UI stack away from the P360 client.

**gpui-component 0.5.1** does depend on `gpui` 0.2 and has a dock, a table, and inputs. It was rejected for two reasons. Its dock owns the layout as GPUI entities, which fights the pure-data layout crate we need to unit test and to save as JSON. Its default features also pull a tree-sitter editor stack this client does not use.

Revisit both when this client leaves GPUI 0.2.2.

## Model source

`ModelSource` has two implementations.

* `JsonModelSource` reads the hand-written file at `mock/sample/model.json`.
* `IdmcExportSource` reads a Business 360 export directory or zip. It looks for `exportMetadata.v2.json`, then `*.MDM_BUSINESS_ENTITY.dat` and `*.MDM_REFERENCE_ENTITY.dat`.

The default directory is `mock/idmc-export/person` (Customer 360 Person). `mock/idmc-export/item` (Product 360 Item and Supplier) stays in the tree so a second package proves the loader is not Person-specific. `cargo test -p idmc-model` loads both.

Field types come from the `eClass` suffix (`TextField`, `LookupField`, `FieldGroup`, `BusinessEntityReferenceField`, …). Labels are the English `LocalizedText`. Record labels come from `options.uiConfiguration.descriptionTemplate`. Person uses `firstName` and `lastName`. Item uses the nested path `description.productTitle`.

Repeatable field groups (`allowMany`, and not the system `_meta` group) become child entities with ids `{parentGuid}.{groupName}`. The relationship is embedded and one-to-many. That is what makes “Person table → detail → Addresses of the selected person” a model-driven link instead of a special case. System `_meta` stays on the field tree, is hidden from the default detail, and is not an entity.

A `BusinessEntityReferenceField` becomes a many-to-one relationship plus a derived inverse. Item’s supplier field is the example. Person’s export has no business-entity references.

Reference 360 files in this package carry the code-list identity only. There are no value rows. `mock/reference-values.json` supplies a handful of sample codes (gender, country, address type, phone type, …) and is documented as a sample, not as Informatica data. Names that are not in the model are an error, so the Person overlay is not applied to the Item export unless you pass it explicitly.

Child rows are not master records. They live nested on the person. `related` projects them with ids `{parentId}::{field}::{index}`.

## Live reads

`IDMC_BACKEND=live` (or `--backend live`) logs in and searches. The default stays mock so CI and a checkout without credentials still open the Person sample. Credentials live in `.env`, which is gitignored. `.env.example` lists the keys. The client never prints the password or `IDS-SESSION-ID`.

Login is v3 `POST {login}/saas/public/core/v3/login` (`products[].baseApiUrl`, `userInfo.sessionId`, which may be a JWT). A 404, 405, or 501 falls back to v2 `POST {login}/ma/api/v2/user/login` (`serverUrl`, `icSessionId`). A 401 or 403 does not fall through, so a bad password is not tried twice. The header on later calls is `IDS-SESSION-ID` for both versions, plus `MODEL_VERSION` set once to the epoch in milliseconds. The MDM host drops `/saas` from `baseApiUrl` or `serverUrl` and adds `-mdm` to the first DNS label when it is not already there. An IP host is left alone so a local stub can serve both.

Search is `POST {mdm}/search/public/api/v1/search` with `entityType`, `search: "*"`, `pageSize`, `recordOffset`, and `recordsToReturn`. `fields` is not sent in the same body, because the Search API rejects `search` and `fields` together. Record ids are `_meta.businessId`. Search field names are prefixed with the entity id with the dots removed (`c360person.firstName`). A dotted key such as `c360person.PostalAddress.addressLine1` is nested into that field group. A date sent as unix milliseconds is stored as `YYYY-MM-DD`. The read API, `GET {mdm}/business-entity/public/api/v1/entity/{businessEntity}/{businessId}`, uses unprefixed names and is the full record: `fullName`, `birthDate`, phone, and email are often absent from search. Visible master rows are read with that GET, a few at a time, and the table row is replaced. Lookup values are `{Code, Name}`. A search hit may send only `Name`. The name is shown even when `reference-values.json` does not list that code. `_meta.id` is not used as the record id. `_contentMeta` is not stored.

A master table is a window, not a download. The first search returns the hit count and 500 rows. Scrolling fetches the window under the viewport, four requests at a time, sorted by the server. The client does not sort those rows again. `search` and `fields` are not sent together. A disk cache per host and entity paints the last snapshot immediately. An Egress job into a local index is the alternative if a window is not enough; it is described in `docs/performance.md` and not implemented. A 401 logs in once and retries. A 429 honors `Retry-After` (capped at 30s) or waits with a short backoff. Connect times out at 15 seconds and a read at 60. `--check-login` prints each step and does not open a window. `--debug-dump` writes each request and response, with session ids and passwords replaced by `***`.

A save of an existing record checks only the fields that changed. `reference-values.json` is a sample, not the tenant catalog, so a code that is not in that file is not rejected on the client. The write is the Business Entity API: `POST .../entity/{entity}?sourceSystem&sourcePKey` to create, and `PATCH .../entity/{entity}/{businessId}?sourceSystem` with `[{op, path, value}]` to update. Lookups go out as `{Code, Name}`. After a 2xx the client reads the record again and says what was written. A server error is shown as the server sent it.

A failed login does not exit. Transport errors and timeouts retry once in the background after two seconds, then the window stays open with the cause (network unreachable, authentication rejected, or login timed out), Retry, and Continue with mock data. A 401 or 403 is shown immediately and is not retried automatically. `--check-login` still prints and exits. The detail line is scrubbed: passwords, usernames, userinfo in URLs, and JWT-shaped tokens are not logged or shown.

The window uses one client-drawn title bar (transparent system title bar, client decorations). It holds the logo, the name once, workspace tabs, the theme toggle, and Ctrl+K. On Windows the min, max, and close controls are ours; the caption region drags and a double-click maximizes. Connection text, the model summary, and save or login errors sit in the bottom status bar only.

A lookup field edits from an inline menu anchored to the cell or the detail row. It opens on click, Enter, or typing, filters as you type, and Enter writes the highlighted reference value. The current value stays checked. A field that is not required has a Clear row. The options come from the reference values on the model.

Table columns remember width and order on the view. The header edge resizes (minimum 64px, double-click fits the label and the visible cells) and the header label reorders. Arrow keys move a mint cell cursor. The focused row stays selected so linked views follow it. Shift+wheel, a horizontal trackpad, and the horizontal scrollbar move the header and the body together. Right-clicking a row opens a menu of that entity's child and linked views, plus Open detail and Show in graph. Menus snap inside the window and scroll when they are taller than it. The command palette is a centered modal on a dimmed backdrop: a large search field with a mint ring, sections (Recent, Navigation, Records, and the rest), two-line rows, shortcut chips, and a footer of key hints. Right drills in, Left or Backspace goes back, and Escape closes. Record search stays in the same list. The accent stays Fiwe Mint.

Ctrl+K searches records as well as commands. Live search waits about 150ms, then runs a full-text search on each master entity and drops the result if the query changed. Mock search is local. Choosing a hit selects that record in a table of its entity and focuses a detail linked to it.

`workspace.json` version 2 stores a tab per workspace. Each table view also stores `columnWidths` by field id. Templates (Browse, Search & inspect, Data quality) are built from the loaded model. A version 1 file opens as one Browse tab.

## Workspace file

`workspace.json` in the working directory. Version 2 stores theme plus one tab per workspace (layout, views, focus, zoom, selections). A version 1 file is read as a single Browse tab. A saved file that names entities the current model does not have is stripped on load. The accent is Fiwe Mint `#47FF9A` on the dark theme. Light theme text uses a darker mint so it stays readable on white. The header draws `assets/logo/mark.svg` in color. The window icon on Windows is `assets/logo/idmc-client.ico` (16, 32, 48, and 256, packed from the PNGs) linked as resource 1, which is the icon GPUI loads for the window class.

## Open questions

* Where a full code-list catalog comes from. The export still has no value rows, and the sample file is not treated as closed. Live search and read responses carry `{Code, Name}` for the values on the records that were returned. Codes that never appear on a record stay blank unless `reference-values.json` lists them. The server remains the authority on save.
* There is still no documented metadata API. The model is the export package. Expanding a type in the graph view calls `GET /metadata/api/v2/objects/{entity}` as a relationship helper and ignores a failure.

## Graph view

The graph is a GPUI canvas, not a port of the Braian TypeScript. Force parameters match that Apache-2.0 graph: link distance `72 + r1 + r2` at strength 0.25, many-body charge −560 with a max distance of 820, collision at `r + 4` for two iterations, `alphaDecay` 0.032, `velocityDecay` 0.5. A merge keeps existing coordinates and reheats to 0.3. The scene is built from the loaded model (embedded child groups, then child rows, then reference stubs). Schema mode is one node per root type. Live expand reads `GET /business-entity/public/api/v1/entity/{be}/{id}` when the record is not cached, and dedupes a key that is already in flight. The scene stops at 400 nodes. Animation ticks at most twice per frame on the graph view alone, and captions are shaped once and painted on the canvas.

## Open questions

* This Person export has no hierarchy. The tree view works for models that declare one (the sample category tree). It is not invented for Person.
* Promoting every `allowMany` group to a child entity is the linking model. A group that should stay an inline editor, rather than its own table, is not distinguished yet.
* Inverse relationships are derived from reference fields. If the export later contains explicit relationships, those should win and the derived inverse should not be duplicated.
* Workspace path is the working directory. A user config directory may be a better default once more than one model is in daily use.
* Record edits are saved through the Business 360 entity API. Create is `POST /business-entity/public/api/v1/entity/{entity}?sourceSystem&sourcePKey`. Field changes are `PATCH` on `.../entity/{entity}/{businessId}?sourceSystem`. Delete is `DELETE` on that record with `sourceSystem`, and `sourcePKey` when the row has one. Child rows are part of the parent payload. The mock backend applies the same operations in memory. A live call updates the screen immediately and rolls the row back if the request fails.
* Localized text other than `en` is ignored. This export only has English.
* Custom fields (Supplier `X_…` in the Item package) are kept as ordinary fields. No special case.
