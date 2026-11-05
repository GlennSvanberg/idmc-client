//! Loader for an Informatica Business 360 export package (zip or unpacked folder).
//!
//! Reference-entity files in this package identify code lists (name, guid,
//! dependency) but do not carry code rows. Callers overlay values separately.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::types::{
    pluralize, Cardinality, EmbeddedIn, EntityType, FieldDef, FieldSide, FieldType, Lookup,
    ModelDefinition, ModelError, Relationship,
};
use crate::ModelSource;

pub struct IdmcExportSource {
    root: PathBuf,
    _tmp: Option<tempfile::TempDir>,
}

impl IdmcExportSource {
    pub fn open(path: &Path) -> Result<Self, ModelError> {
        if path.is_file() {
            let tmp = tempfile::tempdir()?;
            extract_zip(path, tmp.path())?;
            let root = find_root(tmp.path())?;
            return Ok(Self {
                root,
                _tmp: Some(tmp),
            });
        }
        Ok(Self {
            root: find_root(path)?,
            _tmp: None,
        })
    }
}

impl ModelSource for IdmcExportSource {
    fn load(&self) -> Result<ModelDefinition, ModelError> {
        load_dir(&self.root)
    }
}

fn find_root(path: &Path) -> Result<PathBuf, ModelError> {
    if path.join("exportMetadata.v2.json").is_file() {
        return Ok(path.to_path_buf());
    }
    if path.file_name().is_some_and(|n| n == "Explore") {
        if let Some(parent) = path.parent() {
            if parent.join("exportMetadata.v2.json").is_file() {
                return Ok(parent.to_path_buf());
            }
        }
    }
    Err(ModelError::Message(format!(
        "{} is not an IDMC export (no exportMetadata.v2.json)",
        path.display()
    )))
}

fn extract_zip(zip_path: &Path, dest: &Path) -> Result<(), ModelError> {
    let file = File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        if name.ends_with('/') || name.contains("..") {
            continue;
        }
        let out = dest.join(&name);
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut outfile = File::create(&out)?;
        std::io::copy(&mut entry, &mut outfile)?;
    }
    Ok(())
}

fn load_dir(root: &Path) -> Result<ModelDefinition, ModelError> {
    let mut files = Vec::new();
    collect(root, &mut files)?;
    files.sort();
    let meta_name = fs::read_to_string(root.join("exportMetadata.v2.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|v| v.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
        .unwrap_or_else(|| "IDMC export".into());

    let mut lookups = Vec::new();
    let mut guid_to_lookup: BTreeMap<String, String> = BTreeMap::new();
    for path in files.iter().filter(|p| is_suffix(p, ".MDM_REFERENCE_ENTITY.dat")) {
        let value: Value = serde_json::from_str(&fs::read_to_string(path)?)?;
        if let Some(lookup) = parse_reference(&value)? {
            if let Some(guid) = &lookup.guid {
                guid_to_lookup.insert(guid.clone(), lookup.id.clone());
            }
            lookups.push(lookup);
        }
    }
    lookups.sort_by(|a, b| a.id.cmp(&b.id));

    let mut entities = Vec::new();
    let mut relationships = Vec::new();
    for path in files
        .iter()
        .filter(|p| is_suffix(p, ".MDM_BUSINESS_ENTITY.dat"))
    {
        let value: Value = serde_json::from_str(&fs::read_to_string(path)?)?;
        parse_business_entity(&value, &mut lookups, &guid_to_lookup, &mut entities, &mut relationships)?;
    }
    if entities.iter().all(|e| e.embedded_in.is_some()) {
        return Err(ModelError::Message(format!(
            "export {} has no business entity",
            root.display()
        )));
    }
    let mut model = ModelDefinition {
        name: meta_name.clone(),
        origin: format!("idmc-export:{meta_name}"),
        entities,
        lookups,
        relationships,
    };
    model.normalize();
    model.validate()?;
    Ok(model)
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), ModelError> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

fn is_suffix(path: &Path, suffix: &str) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.ends_with(suffix))
}

fn parse_reference(doc: &Value) -> Result<Option<Lookup>, ModelError> {
    let Some(objects) = doc.get("exportedMetadataObjects").and_then(|v| v.as_object()) else {
        return Ok(None);
    };
    for (guid, object) in objects {
        let Some(rda) = object.get("exportedRda") else { continue };
        let name = rda
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(guid)
            .to_string();
        let list_label = rda
            .get("defaultCodeList")
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let depends_on = rda
            .get("dependencyObj")
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let alternates = rda
            .get("alternativeCodeLists")
            .and_then(|v| v.as_array())
            .map(|rows| {
                rows.iter()
                    .filter_map(|row| row.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let code_guid = rda
            .get("codeValueBeGuid")
            .and_then(|v| v.as_str())
            .unwrap_or(guid)
            .to_string();
        return Ok(Some(Lookup {
            id: name.clone(),
            label: name,
            guid: Some(code_guid),
            list_label,
            depends_on,
            alternates,
            values: Vec::new(),
            closed: false,
        }));
    }
    Ok(None)
}

fn parse_business_entity(
    doc: &Value,
    lookups: &mut Vec<Lookup>,
    guid_to_lookup: &BTreeMap<String, String>,
    entities: &mut Vec<EntityType>,
    relationships: &mut Vec<Relationship>,
) -> Result<(), ModelError> {
    let Some(objects) = doc.get("exportedMetadataObjects").and_then(|v| v.as_object()) else {
        return Ok(());
    };
    let Some(root_guid) = doc.get("rootGuid").and_then(|v| v.as_str()) else {
        return Ok(());
    };
    let Some(object) = objects.get(root_guid) else {
        return Ok(());
    };
    if object.get("metadataType").and_then(|v| v.as_str()) != Some("businessEntity") {
        return Ok(());
    }
    let Some(payload) = object.get("payload") else { return Ok(()) };
    let code_map = object
        .get("codeListIdRdsNameMap")
        .cloned()
        .unwrap_or(Value::Null);
    let name = payload
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(root_guid)
        .to_string();
    let label = localized(payload.get("label")).unwrap_or_else(|| name.clone());
    let raw_fields = payload
        .get("field")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut fields = Vec::new();
    for raw in &raw_fields {
        fields.push(parse_field(raw, "", &code_map, guid_to_lookup, lookups));
    }
    let label_fields = description_template(payload);
    let entity = EntityType {
        id: root_guid.to_string(),
        name: name.clone(),
        label: label.clone(),
        label_plural: String::new(),
        label_fields,
        column_order: Vec::new(),
        fields,
        hierarchy: None,
        embedded_in: None,
    };
    // Promote repeatable, non-system field groups to child entities so a
    // table can list "Addresses of the selected Person" without a hardcoded view.
    let mut children = Vec::new();
    for field in entity.fields.iter().filter(|f| f.ty == FieldType::Group && f.many && !f.system) {
        let child_id = format!("{root_guid}.{}", field.name);
        let child_fields = field
            .children
            .iter()
            .map(|child| rebase(child, &field.id))
            .collect();
        children.push(EntityType {
            id: child_id.clone(),
            name: field.name.clone(),
            label: field.label.clone(),
            label_plural: String::new(),
            label_fields: Vec::new(),
            column_order: Vec::new(),
            fields: child_fields,
            hierarchy: None,
            embedded_in: Some(EmbeddedIn {
                parent: root_guid.to_string(),
                field: field.name.clone(),
            }),
        });
        relationships.push(Relationship {
            id: format!("{child_id}#items"),
            label: field.label.clone(),
            from: root_guid.to_string(),
            to: child_id,
            cardinality: Cardinality::OneToMany,
            field: field.name.clone(),
            field_on: FieldSide::From,
            embedded: true,
        });
    }
    for field in entity.leaves() {
        if field.ty != FieldType::Reference {
            continue;
        }
        let Some(target) = field.target_entity.clone() else { continue };
        let rel_id = format!("{root_guid}.{}", field.id);
        if field.many {
            relationships.push(Relationship {
                id: rel_id,
                label: field.label.clone(),
                from: root_guid.to_string(),
                to: target,
                cardinality: Cardinality::OneToMany,
                field: field.id.clone(),
                field_on: FieldSide::From,
                embedded: false,
            });
        } else {
            relationships.push(Relationship {
                id: rel_id.clone(),
                label: field.label.clone(),
                from: root_guid.to_string(),
                to: target.clone(),
                cardinality: Cardinality::ManyToOne,
                field: field.id.clone(),
                field_on: FieldSide::From,
                embedded: false,
            });
            relationships.push(Relationship {
                id: format!("{rel_id}.inverse"),
                label: pluralize(&label),
                from: target,
                to: root_guid.to_string(),
                cardinality: Cardinality::OneToMany,
                field: field.id.clone(),
                field_on: FieldSide::To,
                embedded: false,
            });
        }
    }
    entities.push(entity);
    entities.extend(children);
    Ok(())
}

fn rebase(field: &FieldDef, prefix: &str) -> FieldDef {
    let strip = format!("{prefix}.");
    let mut cloned = field.clone();
    cloned.id = field
        .id
        .strip_prefix(&strip)
        .unwrap_or(field.id.as_str())
        .to_string();
    cloned.children = field.children.iter().map(|c| rebase(c, prefix)).collect();
    cloned.default_column = false;
    cloned
}

fn parse_field(
    raw: &Value,
    prefix: &str,
    code_map: &Value,
    guid_to_lookup: &BTreeMap<String, String>,
    lookups: &mut Vec<Lookup>,
) -> FieldDef {
    let eclass = raw.get("eClass").and_then(|v| v.as_str()).unwrap_or("");
    let kind = eclass.rsplit("#//").next().unwrap_or("");
    let name = raw
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("field")
        .to_string();
    let id = if prefix.is_empty() {
        name.clone()
    } else {
        format!("{prefix}.{name}")
    };
    let label = localized(raw.get("label")).unwrap_or_else(|| name.clone());
    let system = raw.get("systemField").and_then(|v| v.as_bool()).unwrap_or(false);
    let many = raw.get("allowMany").and_then(|v| v.as_bool()).unwrap_or(false);
    let mandatory = raw.get("mandatory").and_then(|v| v.as_bool()).unwrap_or(false);
    let length = json_u32(raw.get("length"));
    let precision = json_u32(raw.get("precision"));
    let scale = json_u32(raw.get("scale"));
    let multiline = raw.get("isMultiline").and_then(|v| v.as_bool()).unwrap_or(false);
    let mut ty = match kind {
        "TextField" if multiline => FieldType::Text,
        "TextField" => FieldType::String,
        "IntegerField" => FieldType::Integer,
        "DecimalField" | "DoubleField" => FieldType::Decimal,
        "BooleanField" => FieldType::Boolean,
        "DateField" => FieldType::Date,
        "DateTimeField" => FieldType::DateTime,
        "URLField" => FieldType::Url,
        "LookupField" => FieldType::Lookup,
        "BusinessEntityReferenceField" => FieldType::Reference,
        "FieldGroup" => FieldType::Group,
        _ => FieldType::Unknown,
    };
    let mut lookup = None;
    if ty == FieldType::Lookup {
        if let Some(guid) = raw
            .get("codeField")
            .and_then(ref_guid)
        {
            let name = code_map
                .get(&guid)
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| guid_to_lookup.get(&guid).cloned());
            if let Some(name) = name {
                lookup = Some(name);
            } else {
                lookup = Some(guid.clone());
                if !lookups.iter().any(|l| l.id == guid) {
                    lookups.push(Lookup {
                        id: guid.clone(),
                        label: guid.clone(),
                        guid: Some(guid),
                        list_label: None,
                        depends_on: None,
                        alternates: Vec::new(),
                        values: Vec::new(),
                        closed: false,
                    });
                }
            }
        }
    }
    let target_entity = if ty == FieldType::Reference {
        raw.get("businessEntity").and_then(ref_target)
    } else {
        None
    };
    let mut children = Vec::new();
    if let Some(rows) = raw.get("field").and_then(|v| v.as_array()) {
        for row in rows {
            children.push(parse_field(row, &id, code_map, guid_to_lookup, lookups));
        }
    }
    if ty == FieldType::Unknown && !children.is_empty() {
        ty = FieldType::Group;
    }
    FieldDef {
        id,
        name,
        label,
        ty,
        mandatory,
        system,
        many,
        length,
        precision,
        scale,
        lookup,
        target_entity,
        children,
        default_column: false,
    }
}

fn description_template(payload: &Value) -> Vec<String> {
    let Some(rows) = payload
        .pointer("/options/uiConfiguration/descriptionTemplate")
        .and_then(|v| v.as_array())
    else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            let text = row.get("$ref").and_then(|v| v.as_str())?;
            let names = field_names(text);
            if names.is_empty() {
                None
            } else {
                Some(names.join("."))
            }
        })
        .collect()
}

fn field_names(reference: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = reference;
    while let Some(start) = rest.find("name='") {
        let after = &rest[start + 6..];
        if let Some(end) = after.find('\'') {
            names.push(after[..end].to_string());
            rest = &after[end + 1..];
        } else {
            break;
        }
    }
    names
}

fn localized(value: Option<&Value>) -> Option<String> {
    let value = value?;
    value
        .get("en")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn json_u32(value: Option<&Value>) -> Option<u32> {
    value.and_then(|v| v.as_u64()).and_then(|n| u32::try_from(n).ok())
}

fn ref_guid(value: &Value) -> Option<String> {
    ref_target(value).and_then(|text| {
        guid_in(&text).or_else(|| {
            if text.contains("://") || text.contains("@") {
                None
            } else {
                Some(text)
            }
        })
    })
}

fn ref_target(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Object(map) => map.get("$ref").and_then(|v| v.as_str()).map(|s| s.to_string()),
        _ => None,
    }
}

fn guid_in(text: &str) -> Option<String> {
    let key = "guid='";
    let start = text.find(key)? + key.len();
    let end = text[start..].find('\'')? + start;
    Some(text[start..end].to_string())
}

