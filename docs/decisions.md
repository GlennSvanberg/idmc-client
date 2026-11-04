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

Child rows are not master records. They live nested on the person. `MockBackend::related` projects them with ids `{parentId}::{field}::{index}`.

## Workspace file

`workspace.json` in the working directory. Versioned JSON: theme, layout tree, view specs, focus, zoom, selections. A saved file that names entities the current model does not have is stripped on load.

## Open questions

* Where real code-list value rows come from. This export does not contain them. A Reference 360 API, or a fuller export, has to fill `reference-values.json` before the lookups are anything but samples.
* IDMC authentication, org, and the runtime read API. There is no live backend yet. `Backend` is the seam.
* This Person export has no hierarchy. The tree view works for models that declare one (the sample category tree). It is not invented for Person.
* Promoting every `allowMany` group to a child entity is the linking model. A group that should stay an inline editor, rather than its own table, is not distinguished yet.
* Inverse relationships are derived from reference fields. If the export later contains explicit relationships, those should win and the derived inverse should not be duplicated.
* Workspace path is the working directory. A user config directory may be a better default once more than one model is in daily use.
* Editing and write-back are out of scope. The client is read-only.
* Localized text other than `en` is ignored. This export only has English.
* Custom fields (Supplier `X_…` in the Item package) are kept as ordinary fields. No special case.
