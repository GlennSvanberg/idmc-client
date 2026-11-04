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

Search is `POST {mdm}/search/public/api/v1/search` with `entityType`, `search: "*"`, `pageSize`, `recordOffset`, and `recordsToReturn`. `fields` is not sent in the same body. Record ids are `_meta.businessId`. Search field names are prefixed with the entity id with the dots removed (`c360person.firstName`). The read API, `GET {mdm}/business-entity/public/api/v1/entity/{businessEntity}/{businessId}`, uses unprefixed names. Lookup values are `{Code, Name}` and the name is shown even when `reference-values.json` does not list that code. `_meta.id` is not used as the record id. `_contentMeta` is not stored.

The first search page of each master entity is loaded before the window opens. Further pages run on a background thread. A 401 logs in once and retries. A 429 honors `Retry-After` (capped at 30s) or waits with a short backoff. `--check-login` prints whether login and one search worked and does not open a window. `--debug-dump` writes responses with session ids and passwords replaced by `***`.

## Workspace file

`workspace.json` in the working directory. Versioned JSON: theme, layout tree, view specs, focus, zoom, selections. A saved file that names entities the current model does not have is stripped on load.

## Open questions

* Where a full code-list catalog comes from. The export still has no value rows. Live search and read responses carry `{Code, Name}` for the values on the records that were returned. Codes that never appear on a record stay blank unless `reference-values.json` lists them.
* There is still no documented metadata API. The model is the export package. `GET /metadata/api/v2/objects/...` shows up only as a relationship helper and is not called.
* This Person export has no hierarchy. The tree view works for models that declare one (the sample category tree). It is not invented for Person.
* Promoting every `allowMany` group to a child entity is the linking model. A group that should stay an inline editor, rather than its own table, is not distinguished yet.
* Inverse relationships are derived from reference fields. If the export later contains explicit relationships, those should win and the derived inverse should not be duplicated.
* Workspace path is the working directory. A user config directory may be a better default once more than one model is in daily use.
* Editing and write-back are out of scope. The client is read-only.
* Localized text other than `en` is ignored. This export only has English.
* Custom fields (Supplier `X_…` in the Item package) are kept as ordinary fields. No special case.
