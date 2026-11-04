//! Metadata model for the IDMC client.
//!
//! Views, field choosers, and link menus read this definition. They do not
//! hardcode entity or field names. Two loaders implement [`ModelSource`]:
//! a hand-written JSON file, and an Informatica Business 360 export package.

mod graph;
mod idmc;
mod types;
mod value;
mod write;

pub use graph::{
    build_entity_scene, build_model_scene, build_record_fields, expand_key, node_mark, radius_for, stub_entity,
    GraphEdge, GraphNode, GraphProperty, NodeId, NodeKind, Scene, SCENE_CAP,
};

pub use idmc::IdmcExportSource;
pub use types::*;
pub use value::{value_at, FieldValue, Record};
pub use write::{
    api_body, child_record_id, edit_text, lookup_choices, parse_field, patch_ops, push_child, read_field,
    remove_child, set_child_field, split_child_id, validate_edit, DEFAULT_SOURCE_SYSTEM,
};

/// Something that can produce a [`ModelDefinition`]. IDMC export packages and
/// hand-written JSON files both sit behind this so a later source (a live
/// metadata API, a migration dump) can be added without changing the UI.
pub trait ModelSource {
    fn load(&self) -> Result<ModelDefinition, ModelError>;
}

pub struct JsonModelSource {
    pub name: String,
    pub json: String,
}

impl JsonModelSource {
    pub fn open(path: &std::path::Path) -> Result<Self, ModelError> {
        let json = std::fs::read_to_string(path)?;
        Ok(Self {
            name: path.display().to_string(),
            json,
        })
    }
}

impl ModelSource for JsonModelSource {
    fn load(&self) -> Result<ModelDefinition, ModelError> {
        let mut model: ModelDefinition = serde_json::from_str(&self.json)?;
        if model.origin.is_empty() {
            model.origin = format!("json:{}", self.name);
        }
        model.normalize();
        model.validate()?;
        Ok(model)
    }
}

pub fn load_records(json: &str) -> Result<Vec<Record>, ModelError> {
    let doc: serde_json::Value = serde_json::from_str(json)?;
    let rows = doc
        .get("records")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ModelError::Message("data file needs a records array".into()))?;
    rows.iter()
        .map(|row| Record::from_json(row).map_err(ModelError::Message))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn mock_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mock")
    }

    fn load_export(rel: &str) -> ModelDefinition {
        IdmcExportSource::open(&mock_dir().join(rel))
            .unwrap_or_else(|err| panic!("open {rel}: {err}"))
            .load()
            .unwrap_or_else(|err| panic!("load {rel}: {err}"))
    }

    #[test]
    fn person_export_promotes_child_collections_and_reads_the_label_template() {
        let model = load_export("idmc-export/person");
        let person = model.entity("c360.person").expect("person");
        assert_eq!(person.label, "Person");
        assert_eq!(person.label_fields, vec!["firstName", "lastName"]);
        assert!(person.field("_meta").is_some_and(|f| f.system));
        assert!(model.entity("c360.person._meta").is_none());

        let address = model
            .entity("c360.person.PostalAddress")
            .expect("address collection");
        assert_eq!(address.label, "Address");
        assert_eq!(address.label_plural, "Addresses");
        assert_eq!(
            address.embedded_in.as_ref().map(|e| e.field.as_str()),
            Some("PostalAddress")
        );
        assert!(address.field("addressLine1").is_some_and(|f| f.mandatory));
        assert!(address.field("country").is_some_and(|f| f.lookup.as_deref() == Some("Country")));
        assert!(address.default_columns().iter().any(|c| c == "city"));
        assert!(address.default_columns().iter().any(|c| c == "postalCode"));

        assert!(model.entity("c360.person.Phone").is_some());
        assert!(model.entity("c360.person.ElectronicAddress").is_some());
        assert_eq!(model.embedded_children("c360.person").count(), 19);

        let rel = model
            .relationship("c360.person.PostalAddress#items")
            .expect("address relationship");
        assert!(rel.embedded);
        assert_eq!(rel.cardinality, Cardinality::OneToMany);
        assert_eq!(rel.from, "c360.person");

        let gender = person.field("gender").unwrap();
        assert_eq!(gender.lookup.as_deref(), Some("Gender"));
        let gender_list = model.lookup("Gender").unwrap();
        assert!(gender_list.values.is_empty(), "export has no code rows");
        assert_eq!(model.lookup("State").unwrap().depends_on.as_deref(), Some("Country"));

        let columns = person.default_columns();
        assert_eq!(&columns[..2], ["firstName", "lastName"]);
        assert!(columns.iter().any(|c| c == "gender"));

        let options = model.link_options("c360.person", "c360.person.PostalAddress", ViewKind::Table);
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].relationship, "c360.person.PostalAddress#items");
        assert!(model
            .link_options("c360.person", "c360.person.PostalAddress", ViewKind::Detail)
            .is_empty());
        assert!(model
            .link_options("c360.person.Phone", "c360.person", ViewKind::Detail)
            .is_empty());
        assert!(model
            .link_options("c360.person", "c360.person", ViewKind::Detail)
            .iter()
            .any(|o| o.relationship == "identity"));
        assert!(model
            .link_options("c360.person", "c360.person", ViewKind::Tree)
            .is_empty());
    }

    #[test]
    fn item_export_uses_the_same_loader() {
        let model = load_export("idmc-export/item");
        let item = model.entity("p360.item").expect("item");
        assert_eq!(item.label_fields, vec!["description.productTitle"]);
        assert!(model.entity("s360.supplier").is_some());
        assert!(model.entity("p360.item.description").is_some());
        assert!(model.entity("p360.item._meta").is_none());
        let supplier = model.relationship("p360.item.supplier").expect("supplier ref");
        assert_eq!(supplier.cardinality, Cardinality::ManyToOne);
        assert_eq!(supplier.to, "s360.supplier");
        assert!(!supplier.embedded);
        let inverse = model.relationship("p360.item.supplier.inverse").unwrap();
        assert_eq!(inverse.cardinality, Cardinality::OneToMany);
        assert_eq!(inverse.from, "s360.supplier");
        assert!(item.field("status").unwrap().lookup.as_deref() == Some("ItemStatus"));
    }

    #[test]
    fn zip_package_loads() {
        let dir = mock_dir().join("idmc-export/person");
        let tmp = tempfile::NamedTempFile::new().unwrap();
        {
            let file = std::fs::File::create(tmp.path()).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            fn add(zip: &mut zip::ZipWriter<std::fs::File>, root: &std::path::Path, dir: &std::path::Path, options: zip::write::SimpleFileOptions) {
                for entry in std::fs::read_dir(dir).unwrap() {
                    let entry = entry.unwrap();
                    let path = entry.path();
                    let rel = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                    if path.is_dir() {
                        add(zip, root, &path, options);
                    } else {
                        zip.start_file(rel, options).unwrap();
                        let bytes = std::fs::read(&path).unwrap();
                        use std::io::Write;
                        zip.write_all(&bytes).unwrap();
                    }
                }
            }
            add(&mut zip, &dir, &dir, options);
            zip.finish().unwrap();
        }
        let model = IdmcExportSource::open(tmp.path()).unwrap().load().unwrap();
        assert!(model.entity("c360.person").is_some());
    }

    #[test]
    fn lookup_overlay_resolves_labels() {
        let mut model = load_export("idmc-export/person");
        let json = std::fs::read_to_string(mock_dir().join("reference-values.json")).unwrap();
        model.apply_lookup_values(&json).unwrap();
        assert_eq!(model.lookup_label(Some("Gender"), "F"), "Female");
        assert_eq!(model.lookup_label(Some("Country"), "SE"), "Sweden");
        assert_eq!(
            model.lookup("State").unwrap().values.iter().find(|v| v.id == "SE-AB").unwrap().parent.as_deref(),
            Some("SE")
        );
    }

    #[test]
    fn handwritten_model_keeps_hierarchy_and_rejects_impossible_links() {
        let model = JsonModelSource::open(&mock_dir().join("sample/model.json"))
            .unwrap()
            .load()
            .unwrap();
        assert!(model.entity("Category").unwrap().hierarchy.is_some());
        let products = model.link_options("Category", "Product", ViewKind::Table);
        assert!(products.iter().any(|o| o.relationship == "category_products"));
        assert!(model.link_options("Category", "Product", ViewKind::Detail).is_empty());
        assert!(model
            .link_options("Product", "Product", ViewKind::Detail)
            .iter()
            .any(|o| o.relationship == "identity"));
        assert!(model.link_options("Category", "Article", ViewKind::Table).is_empty());
    }
}
