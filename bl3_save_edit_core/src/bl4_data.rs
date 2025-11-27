use std::collections::HashMap;

use once_cell::sync::Lazy;

#[derive(Debug)]
pub struct ItemTypeEntry {
    pub manufacturer: String,
    pub item_type: String,
}

#[derive(Debug)]
pub struct PartEntry {
    pub part_type: String,
    pub model_name: String,
    pub description: String,
    pub effects: String,
}

#[derive(Debug, Hash, PartialEq, Eq)]
struct PartKey {
    manufacturer: String,
    item_type: String,
    id: u32,
}

static ITEM_TYPES: Lazy<HashMap<u32, ItemTypeEntry>> = Lazy::new(|| {
    let mut map = HashMap::new();
    let csv_data = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/resources/bl4/item_types.csv"
    ));
    let mut reader = csv::Reader::from_reader(csv_data.as_bytes());
    for result in reader.records() {
        let record = match result {
            Ok(r) => r,
            Err(_) => continue,
        };
        let id: u32 = match record.get(0).and_then(|s| s.parse().ok()) {
            Some(v) => v,
            None => continue,
        };
        let manufacturer = record
            .get(1)
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "unknown".to_string());
        let item_type = record
            .get(2)
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "unknown".to_string());
        map.insert(
            id,
            ItemTypeEntry {
                manufacturer,
                item_type,
            },
        );
    }
    map
});

static PARTS: Lazy<HashMap<PartKey, PartEntry>> = Lazy::new(|| {
    let mut map = HashMap::new();
    let csv_data = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/resources/bl4/Weapon_Parts_Viewer.csv"
    ));
    let mut reader = csv::Reader::from_reader(csv_data.as_bytes());
    for result in reader.records() {
        let record = match result {
            Ok(r) => r,
            Err(_) => continue,
        };
        let manufacturer = match record.get(0) {
            Some(s) if !s.trim().is_empty() => s.trim().to_lowercase(),
            _ => continue,
        };
        let item_type = match record.get(1) {
            Some(s) if !s.trim().is_empty() => s.trim().to_lowercase(),
            _ => continue,
        };
        let id_val = record.get(2).map(|s| s.trim()).unwrap_or("");
        if id_val.is_empty() {
            continue;
        }
        let id_float: f32 = match id_val.parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let id = id_float.round() as u32;
        let part_type = record
            .get(3)
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let model_name = record
            .get(4)
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let description = record
            .get(5)
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let effects = record
            .get(6)
            .map(|s| s.trim().to_string())
            .unwrap_or_default();

        let key = PartKey {
            manufacturer,
            item_type,
            id,
        };

        map.insert(
            key,
            PartEntry {
                part_type,
                model_name,
                description,
                effects,
            },
        );
    }
    map
});

pub fn lookup_item_type(id: u32) -> Option<&'static ItemTypeEntry> {
    ITEM_TYPES.get(&id)
}

pub fn all_item_types() -> Vec<(u32, &'static ItemTypeEntry)> {
    let mut entries: Vec<(u32, &'static ItemTypeEntry)> =
        ITEM_TYPES.iter().map(|(id, entry)| (*id, entry)).collect();
    entries.sort_by_key(|(id, entry)| {
        (
            entry.manufacturer.to_lowercase(),
            entry.item_type.to_lowercase(),
            *id,
        )
    });
    entries
}

pub fn lookup_part(manufacturer: &str, item_type: &str, id: u32) -> Option<&'static PartEntry> {
    let key = PartKey {
        manufacturer: manufacturer.to_lowercase(),
        item_type: item_type.to_lowercase(),
        id,
    };
    PARTS.get(&key)
}

pub fn part_entries_for(manufacturer: &str, item_type: &str) -> Vec<(u32, &'static PartEntry)> {
    let manufacturer = manufacturer.to_lowercase();
    let item_type = item_type.to_lowercase();

    let mut entries: Vec<(u32, &'static PartEntry)> = PARTS
        .iter()
        .filter_map(|(key, entry)| {
            if key.manufacturer == manufacturer && key.item_type == item_type {
                Some((key.id, entry))
            } else {
                None
            }
        })
        .collect();

    entries.sort_by_key(|(id, _)| *id);
    entries
}
