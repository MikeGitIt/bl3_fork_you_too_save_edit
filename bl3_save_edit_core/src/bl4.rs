use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashSet},
    fmt::Write as FmtWrite,
    fs,
    io::{Read, Write},
    path::Path,
};

use crate::bl4_data;
use crate::bl4_serial::{self, PartSubType as SerialPartSubType, Token as SerialToken};
use adler::adler32_slice;
use aes::{
    cipher::{BlockDecryptMut, BlockEncryptMut, KeyInit},
    Aes256,
};
use aes::cipher::generic_array::GenericArray;
use anyhow::{anyhow, bail, Context, Result};
use flate2::{
    read::{DeflateDecoder, ZlibDecoder},
    write::ZlibEncoder,
    Compression,
};
use once_cell::sync::Lazy;
use serde_yaml::{Mapping, Number, Value};

const CHARSET: &str =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=!$%&*()[]{}~`^_<>?#;";
const BASE_KEY: [u8; 32] = [
    0x35, 0xEC, 0x33, 0x77, 0xF3, 0x5D, 0xB0, 0xEA, 0xBE, 0x6B, 0x83, 0x11, 0x54, 0x03, 0xEB, 0xFB,
    0x27, 0x25, 0x64, 0x2E, 0xD5, 0x49, 0x06, 0x29, 0x05, 0x78, 0xBD, 0x60, 0xBA, 0x4A, 0xA7, 0x87,
];

#[derive(Debug, Clone, Copy)]
pub enum DecryptOutcome {
    Plain,
    WithDecoded { decoded_count: usize },
}

#[derive(Debug, Clone, Copy)]
pub enum EncryptOutcome {
    Plain,
    Reencoded,
    NoDecodedSection,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Bl4InventoryItem {
    pub slot: String,
    pub serial: String,
    pub state_flags: Option<i32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Bl4Cosmetics {
    pub body: Option<String>,
    pub head: Option<String>,
    pub skin: Option<String>,
    pub primary_color: Option<String>,
    pub secondary_color: Option<String>,
    pub tertiary_color: Option<String>,
    pub echo_body: Option<String>,
    pub echo_attachment: Option<String>,
    pub echo_skin: Option<String>,
    pub vehicle_skin: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Bl4VehicleLoadout {
    pub personal_vehicle: Option<String>,
    pub hover_drive: Option<String>,
    pub vehicle_weapon_slot: Option<i32>,
    pub vehicle_cosmetic: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Bl4SduLevels {
    pub backpack: i32,
    pub pistol: i32,
    pub smg: i32,
    pub assault_rifle: i32,
    pub shotgun: i32,
    pub sniper: i32,
    pub heavy: i32,
    pub grenade: i32,
    pub bank: i32,
    pub lost_loot: i32,
}

impl Bl4SduLevels {
    fn record_node(
        &mut self,
        node_name: &str,
        points_spent: Option<i32>,
        activation_level: Option<i32>,
        is_activated: Option<bool>,
    ) {
        let unlocked = points_spent.unwrap_or_default() > 0
            || is_activated.unwrap_or(false)
            || activation_level.unwrap_or_default() > 0;
        if !unlocked {
            return;
        }

        let suffix_level = node_name
            .rsplit('_')
            .next()
            .and_then(|suffix| {
                suffix
                    .chars()
                    .filter(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse::<i32>()
                    .ok()
            })
            .unwrap_or(0);
        let level = activation_level.unwrap_or(suffix_level);

        let update = |slot: &mut i32| {
            if level > *slot {
                *slot = level;
            }
        };

        if node_name.starts_with("Ammo_Pistol") {
            update(&mut self.pistol);
        } else if node_name.starts_with("Ammo_SMG") {
            update(&mut self.smg);
        } else if node_name.starts_with("Ammo_AR") {
            update(&mut self.assault_rifle);
        } else if node_name.starts_with("Ammo_SG") {
            update(&mut self.shotgun);
        } else if node_name.starts_with("Ammo_SR") {
            update(&mut self.sniper);
        } else if node_name.starts_with("Ammo_GL") || node_name.starts_with("Ammo_Grenade") {
            update(&mut self.grenade);
        } else if node_name.starts_with("Ammo_RL")
            || node_name.starts_with("Ammo_HW")
            || node_name.starts_with("Ammo_Heavy")
        {
            update(&mut self.heavy);
        } else if node_name.starts_with("Backpack") {
            update(&mut self.backpack);
        } else if node_name.starts_with("Bank") {
            update(&mut self.bank);
        } else if node_name.starts_with("Lost_Loot") {
            update(&mut self.lost_loot);
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Bl4PointPools {
    pub character_progress: Option<i64>,
    pub specialization_tokens: Option<i64>,
    pub echo_tokens: Option<i64>,
    pub other: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Bl4SkillNode {
    pub name: String,
    pub points_spent: Option<i32>,
    pub activation_level: Option<i32>,
    pub is_activated: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Bl4SkillTree {
    pub name: String,
    pub group_def_name: Option<String>,
    pub nodes: Vec<Bl4SkillNode>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Bl4MissionStatus {
    pub set: String,
    pub mission: String,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Bl4SaveSummary {
    pub file_name: String,
    pub char_guid: Option<String>,
    pub class: Option<String>,
    pub char_name: Option<String>,
    pub player_difficulty: Option<String>,
    pub character_level: Option<i32>,
    pub character_experience: Option<i64>,
    pub specialization_level: Option<i32>,
    pub specialization_points: Option<i64>,
    pub point_pools: Bl4PointPools,
    pub sdu_levels: Bl4SduLevels,
    pub skill_trees: Vec<Bl4SkillTree>,
    pub tracked_missions: Vec<String>,
    pub tracked_missions_need_none: bool,
    pub missions: Vec<Bl4MissionStatus>,
    pub currencies: BTreeMap<String, i64>,
    pub ammo: BTreeMap<String, i32>,
    pub inventory: Vec<Bl4InventoryItem>,
    pub equip_slots_unlocked: Vec<i32>,
    pub unique_rewards: Vec<String>,
    pub cosmetics: Bl4Cosmetics,
    pub vehicle_loadout: Bl4VehicleLoadout,
    pub active_missions: Vec<Bl4MissionStatus>,
    pub unlockables: BTreeMap<String, Vec<String>>,
    pub progression_in_state: bool,
    pub missions_in_state: bool,
}

#[derive(Debug, Clone)]
pub struct Bl4LoadedSave {
    pub summary: Bl4SaveSummary,
    pub yaml: Value,
}

impl PartialEq for Bl4LoadedSave {
    fn eq(&self, other: &Self) -> bool {
        self.summary == other.summary
    }
}

impl Eq for Bl4LoadedSave {}

impl PartialOrd for Bl4LoadedSave {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.summary.partial_cmp(&other.summary)
    }
}

impl Ord for Bl4LoadedSave {
    fn cmp(&self, other: &Self) -> Ordering {
        self.summary.cmp(&other.summary)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Bl4EditState {
    pub char_guid: Option<String>,
    pub char_name: Option<String>,
    pub class_name: Option<String>,
    pub character_level: Option<i32>,
    pub character_experience: Option<i64>,
    pub ability_points: Option<i64>,
    pub player_difficulty: Option<String>,
    pub specialization_level: Option<i32>,
    pub specialization_points: Option<i64>,
    pub currencies: BTreeMap<String, i64>,
    pub ammo: BTreeMap<String, i32>,
    pub sdu_levels: Bl4SduLevels,
    pub sdu_levels_dirty: bool,
    pub point_pools: Bl4PointPools,
    pub equip_slots_unlocked: Vec<i32>,
    pub unique_rewards_dirty: bool,
    pub unique_rewards: Vec<String>,
    pub cosmetics: Bl4Cosmetics,
    pub vehicle_loadout: Bl4VehicleLoadout,
    pub tracked_missions: Vec<String>,
    pub tracked_missions_need_none: bool,
    pub missions: Vec<Bl4MissionStatus>,
    pub inventory: Vec<Bl4InventoryItem>,
    pub skill_trees: Vec<Bl4SkillTree>,
    pub unlockables: BTreeMap<String, Vec<String>>,
    pub progression_in_state: bool,
    pub missions_in_state: bool,
    pub tracked_missions_dirty: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ItemStats {
    pub primary_stat: Option<u16>,
    pub secondary_stat: Option<u16>,
    pub level: Option<u16>,
    pub rarity: Option<u8>,
    pub manufacturer: Option<u8>,
    pub item_class: Option<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct DecodedItem {
    pub serial: String,
    pub item_type: String,
    pub item_category: String,
    pub length: usize,
    pub stats: ItemStats,
    pub confidence: String,
    pub token_stream: Vec<SerialToken>,
    pub tokens: Option<String>,
    pub bit_string: Option<String>,
    pub parts: Vec<DecodedPart>,
}

#[derive(Debug, Clone)]
pub struct DecodedPart {
    pub token_index: usize,
    pub index: u32,
    pub subtype: SerialPartSubType,
    pub values: Vec<u32>,
    pub label: Option<String>,
    pub part_type: Option<String>,
    pub description: Option<String>,
    pub effects: Option<String>,
    pub value_labels: Vec<Option<String>>,
}

impl Default for DecodedPart {
    fn default() -> Self {
        Self {
            token_index: 0,
            index: 0,
            subtype: SerialPartSubType::None,
            values: Vec::new(),
            label: None,
            part_type: None,
            description: None,
            effects: None,
            value_labels: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Bl4ItemModel {
    pub original_serial: String,
    pub tokens: Vec<SerialToken>,
    pub token_text: String,
    pub bit_string: String,
    pub manufacturer_index: Option<u32>,
    pub manufacturer: Option<String>,
    pub item_type: Option<String>,
    pub level: Option<u32>,
    level_marker_token_index: Option<usize>,
    level_value_token_index: Option<usize>,
    pub parts: Vec<DecodedPart>,
}

impl Bl4ItemModel {
    pub fn decode(serial: &str) -> Result<Self> {
        let (tokens, bit_string) = bl4_serial::deserialize(serial)?;
        let mut model = Self {
            original_serial: serial.to_string(),
            tokens,
            token_text: String::new(),
            bit_string,
            manufacturer_index: None,
            manufacturer: None,
            item_type: None,
            level: None,
            level_marker_token_index: None,
            level_value_token_index: None,
            parts: Vec::new(),
        };
        model.refresh_metadata();
        Ok(model)
    }

    fn refresh_metadata(&mut self) {
        self.token_text = bl4_serial::tokens_to_string(&self.tokens);
        self.manufacturer_index = find_int_at_pos(&self.tokens, 0);

        let mut manufacturer = None;
        let mut item_type = None;
        let mut item_info: Option<&bl4_data::ItemTypeEntry> = None;

        if let Some(idx) = self.manufacturer_index {
            if let Some(info) = bl4_data::lookup_item_type(idx) {
                manufacturer = Some(info.manufacturer.clone());
                item_type = Some(info.item_type.clone());
                item_info = Some(info);
            }
        }

        self.manufacturer = manufacturer;
        self.item_type = item_type;

        if let Some((marker_idx, value_idx)) = find_level_token_indices(&self.tokens) {
            self.level_marker_token_index = Some(marker_idx);
            self.level_value_token_index = Some(value_idx);
            self.level = match self.tokens.get(value_idx) {
                Some(SerialToken::VarInt(value)) | Some(SerialToken::VarBit(value)) => Some(*value),
                _ => None,
            };
        } else {
            self.level_marker_token_index = None;
            self.level_value_token_index = None;
            self.level = None;
        }

        self.parts = decode_parts(&self.tokens, item_info);

        if let Ok(serial) = bl4_serial::serialize(&self.tokens) {
            self.original_serial = serial.clone();
            if let Ok((_, bits)) = bl4_serial::deserialize(&serial) {
                self.bit_string = bits;
            }
        }
    }

    pub fn token_text(&self) -> &str {
        &self.token_text
    }

    pub fn to_serial(&self) -> Result<String> {
        bl4_serial::serialize(&self.tokens)
    }

    pub fn set_level(&mut self, new_level: u32) -> Result<()> {
        let idx = self
            .level_value_token_index
            .ok_or_else(|| anyhow!("level token not found in serial"))?;
        match self
            .tokens
            .get_mut(idx)
            .ok_or_else(|| anyhow!("level token index out of bounds"))?
        {
            SerialToken::VarInt(value) | SerialToken::VarBit(value) => {
                *value = new_level;
            }
            _ => bail!("unexpected token kind for level"),
        }
        self.refresh_metadata();
        Ok(())
    }

    pub fn set_manufacturer_index(&mut self, new_index: u32) -> Result<()> {
        let idx = find_int_token_index(&self.tokens, 0)
            .ok_or_else(|| anyhow!("manufacturer token not found in serial"))?;
        match self
            .tokens
            .get_mut(idx)
            .ok_or_else(|| anyhow!("manufacturer token index out of bounds"))?
        {
            SerialToken::VarInt(value) | SerialToken::VarBit(value) => {
                *value = new_index;
            }
            _ => bail!("unexpected token kind for manufacturer index"),
        }
        self.refresh_metadata();
        Ok(())
    }

    pub fn set_part_index(&mut self, part_idx: usize, new_index: u32) -> Result<()> {
        let part_meta = self
            .parts
            .get(part_idx)
            .ok_or_else(|| anyhow!("part index {part_idx} out of bounds"))?
            .token_index;
        match self
            .tokens
            .get_mut(part_meta)
            .ok_or_else(|| anyhow!("part token index out of bounds"))?
        {
            SerialToken::Part(part) => {
                part.index = new_index;
            }
            _ => bail!("token at index {part_meta} is not a part"),
        }
        self.refresh_metadata();
        Ok(())
    }

    pub fn set_part_values(&mut self, part_idx: usize, new_values: Vec<u32>) -> Result<()> {
        let part_meta = self
            .parts
            .get(part_idx)
            .ok_or_else(|| anyhow!("part index {part_idx} out of bounds"))?
            .token_index;
        match self
            .tokens
            .get_mut(part_meta)
            .ok_or_else(|| anyhow!("part token index out of bounds"))?
        {
            SerialToken::Part(part) => match part.subtype {
                SerialPartSubType::None => {
                    bail!("part {part_idx} does not support value overrides");
                }
                SerialPartSubType::Int => {
                    if new_values.len() != 1 {
                        bail!("int subtypes require exactly one value");
                    }
                    part.value = new_values[0];
                }
                SerialPartSubType::List => {
                    part.values = new_values;
                }
            },
            _ => bail!("token at index {part_meta} is not a part"),
        }
        self.refresh_metadata();
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Bl4CatalogItem {
    pub type_id: u32,
    pub manufacturer: String,
    pub item_type: String,
    pub sample_parts: Vec<Bl4PartOption>,
    pub part_catalog: BTreeMap<String, Vec<Bl4PartOption>>,
}

impl Bl4CatalogItem {
    pub fn label(&self) -> String {
        let manufacturer = self.manufacturer.replace('_', " ");
        let item_type = display_item_type(&self.item_type);
        format!("{} · {} (#{})", manufacturer, item_type, self.type_id)
    }

    pub fn sample_summary(&self, limit: usize) -> Option<String> {
        if self.sample_parts.is_empty() {
            return None;
        }
        let summary: Vec<String> = self
            .sample_parts
            .iter()
            .take(limit)
            .map(|part| part.label.clone())
            .filter(|label| !label.is_empty())
            .collect();
        if summary.is_empty() {
            None
        } else {
            Some(format!("Sample parts: {}", summary.join(", ")))
        }
    }
}

fn display_item_type(raw: &str) -> String {
    let mut title = raw.replace('_', " ");
    if raw.eq_ignore_ascii_case("enhancement") {
        title = "Artifact".to_string();
    }
    title
}

fn is_artifact_type(item_type: &str) -> bool {
    let normalized = item_type.trim().to_ascii_lowercase();
    normalized.contains("artifact") || normalized == "enhancement"
}

static BL4_CATALOG: Lazy<Vec<Bl4CatalogItem>> = Lazy::new(|| {
    let mut items = Vec::new();
    for (type_id, entry) in bl4_data::all_item_types() {
        let options = collect_part_options(&entry.manufacturer, &entry.item_type);
        let mut sample_parts = Vec::new();
        for part_list in options.values() {
            if let Some(first) = part_list.first() {
                sample_parts.push(first.clone());
            }
        }
        items.push(Bl4CatalogItem {
            type_id,
            manufacturer: entry.manufacturer.clone(),
            item_type: entry.item_type.clone(),
            sample_parts,
            part_catalog: options,
        });
    }
    items
});

pub fn search_catalog(query: &str, limit: usize) -> Vec<Bl4CatalogItem> {
    if query.trim().is_empty() {
        return Vec::new();
    }
    let needle = query.to_ascii_lowercase();
    BL4_CATALOG
        .iter()
        .filter(|item| item.label().to_ascii_lowercase().contains(&needle))
        .take(limit)
        .cloned()
        .collect()
}

pub fn artifact_catalog(limit: usize) -> Vec<Bl4CatalogItem> {
    BL4_CATALOG
        .iter()
        .filter(|item| is_artifact_type(&item.item_type))
        .take(limit)
        .cloned()
        .collect()
}

pub fn search_artifact_catalog(query: &str, limit: usize) -> Vec<Bl4CatalogItem> {
    if query.trim().is_empty() {
        return artifact_catalog(limit);
    }
    let needle = query.to_ascii_lowercase();
    BL4_CATALOG
        .iter()
        .filter(|item| is_artifact_type(&item.item_type))
        .filter(|item| item.label().to_ascii_lowercase().contains(&needle))
        .take(limit)
        .cloned()
        .collect()
}

#[derive(Debug, Clone)]
pub struct Bl4PartOption {
    pub id: u32,
    pub part_type: String,
    pub label: String,
    pub description: Option<String>,
    pub effects: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Bl4InventoryItemModel {
    pub slot: String,
    pub state_flags: Option<i32>,
    pub item: Bl4ItemModel,
}

impl Bl4InventoryItemModel {
    pub fn decode(item: &Bl4InventoryItem) -> Result<Self> {
        let model = Bl4ItemModel::decode(&item.serial)?;
        Ok(Self {
            slot: item.slot.clone(),
            state_flags: item.state_flags,
            item: model,
        })
    }

    pub fn to_inventory_item(&self) -> Result<Bl4InventoryItem> {
        Ok(Bl4InventoryItem {
            slot: self.slot.clone(),
            serial: self.item.to_serial()?,
            state_flags: self.state_flags,
        })
    }

    pub fn serial(&self) -> &str {
        &self.item.original_serial
    }

    pub fn refresh_serial(&mut self) -> Result<()> {
        let serial = self.item.to_serial()?;
        self.item = Bl4ItemModel::decode(&serial)?;
        Ok(())
    }

    pub fn has_state_flag(&self, flag: i32) -> bool {
        self.state_flags.unwrap_or(0) & flag != 0
    }

    pub fn set_state_flag(&mut self, flag: i32, enabled: bool) {
        let mut current = self.state_flags.unwrap_or(0);
        if enabled {
            current |= flag;
        } else {
            current &= !flag;
        }
        self.state_flags = Some(current);
    }

    pub fn clear_state_flags(&mut self) {
        self.state_flags = Some(0);
    }
}

pub fn collect_part_options(
    manufacturer: &str,
    item_type: &str,
) -> BTreeMap<String, Vec<Bl4PartOption>> {
    if manufacturer.is_empty() || item_type.is_empty() {
        return BTreeMap::new();
    }

    let mut options: BTreeMap<String, Vec<Bl4PartOption>> = BTreeMap::new();

    for (id, entry) in bl4_data::part_entries_for(manufacturer, item_type) {
        let part_type = if entry.part_type.is_empty() {
            "misc".to_string()
        } else {
            entry.part_type.clone()
        };

        let label = if !entry.model_name.is_empty() {
            entry.model_name.clone()
        } else if !entry.part_type.is_empty() {
            entry.part_type.clone()
        } else {
            format!("part_{id}")
        };

        let option = Bl4PartOption {
            id,
            part_type: part_type.clone(),
            label,
            description: if entry.description.is_empty() {
                None
            } else {
                Some(entry.description.clone())
            },
            effects: if entry.effects.is_empty() {
                None
            } else {
                Some(entry.effects.clone())
            },
        };

        options.entry(part_type).or_default().push(option);
    }

    for values in options.values_mut() {
        values.sort_by_key(|option| option.id);
    }

    options
}

pub fn derive_key(steamid: &str) -> Result<[u8; 32]> {
    let digits: String = steamid.chars().filter(|ch| ch.is_ascii_digit()).collect();
    if digits.is_empty() {
        bail!("steamid must contain digits");
    }

    let sid = digits.parse::<u64>()?;
    let sid_le = sid.to_le_bytes();

    let mut key = BASE_KEY;
    for (idx, byte) in key.iter_mut().take(8).enumerate() {
        *byte ^= sid_le[idx];
    }

    Ok(key)
}

pub fn decrypt_sav_to_yaml(encrypted: &[u8], steamid: &str) -> Result<Vec<u8>> {
    if encrypted.len() % 16 != 0 {
        bail!("input .sav size {} not multiple of 16", encrypted.len());
    }

    let key = derive_key(steamid)?;
    let mut buffer = encrypted.to_vec();
    aes_ecb_decrypt(&mut buffer, &key);

    let body = match pkcs7_unpad(16, &buffer) {
        Ok(data) => data,
        Err(_) => buffer,
    };

    let mut yaml_bytes = Vec::new();
    let mut zlib_decoder = ZlibDecoder::new(&body[..]);
    if let Err(zlib_err) = zlib_decoder.read_to_end(&mut yaml_bytes) {
        yaml_bytes.clear();
        let mut deflate_decoder = DeflateDecoder::new(&body[..]);
        if let Err(deflate_err) = deflate_decoder.read_to_end(&mut yaml_bytes) {
            let mut preview = String::new();
            for byte in body.iter().take(16) {
                let _ = write!(preview, "{byte:02x} ");
            }
            let footer_info = if body.len() >= 8 {
                let footer = &body[body.len() - 8..];
                format!("footer raw: {:02x?} {:02x?}", &footer[..4], &footer[4..])
            } else {
                "footer: <missing>".to_string()
            };
            bail!(
                "corrupt deflate stream (zlib error: {zlib_err}; deflate error: {deflate_err}; first bytes: {preview}; {footer_info})"
            );
        }
    }

    if body.len() >= 8 {
        let footer = &body[body.len() - 8..];
        let expected_adler = u32::from_be_bytes([footer[0], footer[1], footer[2], footer[3]]);
        let expected_len = u32::from_le_bytes([footer[4], footer[5], footer[6], footer[7]]);
        let actual_adler = adler32_slice(&yaml_bytes);
        if actual_adler != expected_adler {
            bail!("checksum mismatch: expected {expected_adler:#010x}, got {actual_adler:#010x}");
        }
        if yaml_bytes.len() as u32 != expected_len {
            bail!(
                "length mismatch: expected {}, got {}",
                expected_len,
                yaml_bytes.len()
            );
        }
    }

    Ok(yaml_bytes)
}

pub fn encrypt_yaml_to_sav(yaml_bytes: &[u8], steamid: &str) -> Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(9));
    encoder.write_all(yaml_bytes)?;
    let mut compressed = encoder.finish()?;

    let checksum = adler32_slice(yaml_bytes);
    compressed.extend_from_slice(&checksum.to_be_bytes());
    compressed.extend_from_slice(&(yaml_bytes.len() as u32).to_le_bytes());

    let mut padded = pkcs7_pad(16, &compressed);
    let key = derive_key(steamid)?;
    aes_ecb_encrypt(&mut padded, &key);
    Ok(padded)
}

pub fn decrypt_file(
    input_path: &Path,
    output_path: &Path,
    steamid: &str,
    decode_serials: bool,
) -> Result<DecryptOutcome> {
    let encrypted =
        fs::read(input_path).with_context(|| format!("failed to read {}", input_path.display()))?;
    let yaml_bytes = decrypt_sav_to_yaml(&encrypted, steamid)?;

    if decode_serials {
        let yaml_str =
            String::from_utf8(yaml_bytes).context("decrypted YAML is not valid UTF-8 text")?;
        let yaml_value: Value =
            serde_yaml::from_str(&yaml_str).context("failed to parse YAML after decrypt")?;
        let decoded_serials = find_and_decode_serials_in_yaml(&yaml_value);

        if decoded_serials.is_empty() {
            fs::write(output_path, yaml_str.as_bytes())
                .with_context(|| format!("failed to write {}", output_path.display()))?;
            Ok(DecryptOutcome::Plain)
        } else {
            let updated_yaml = insert_decoded_items_in_yaml(&yaml_value, &decoded_serials);
            let yaml_output =
                serde_yaml::to_string(&updated_yaml).context("failed to encode YAML output")?;
            fs::write(output_path, yaml_output.as_bytes())
                .with_context(|| format!("failed to write {}", output_path.display()))?;
            Ok(DecryptOutcome::WithDecoded {
                decoded_count: decoded_serials.len(),
            })
        }
    } else {
        fs::write(output_path, &yaml_bytes)
            .with_context(|| format!("failed to write {}", output_path.display()))?;
        Ok(DecryptOutcome::Plain)
    }
}

pub fn encrypt_file(
    input_path: &Path,
    output_path: &Path,
    steamid: &str,
    encode_serials: bool,
) -> Result<EncryptOutcome> {
    let (yaml_bytes, outcome) = if encode_serials {
        let yaml_content = fs::read_to_string(input_path)
            .with_context(|| format!("failed to read {}", input_path.display()))?;
        let yaml_value: Value = serde_yaml::from_str(&yaml_content)
            .context("failed to parse YAML before encrypting")?;

        let has_decoded_section = yaml_value
            .as_mapping()
            .and_then(|map| map.get(&Value::String("_DECODED_ITEMS".into())))
            .is_some();

        if has_decoded_section {
            let updated_value = extract_and_encode_serials_from_yaml(&yaml_value)?;
            let yaml_output = serde_yaml::to_string(&updated_value)
                .context("failed to serialize YAML after encoding serials")?
                .into_bytes();
            (yaml_output, EncryptOutcome::Reencoded)
        } else {
            (yaml_content.into_bytes(), EncryptOutcome::NoDecodedSection)
        }
    } else {
        (
            fs::read(input_path)
                .with_context(|| format!("failed to read {}", input_path.display()))?,
            EncryptOutcome::Plain,
        )
    };

    let encrypted = encrypt_yaml_to_sav(&yaml_bytes, steamid)?;
    fs::write(output_path, &encrypted)
        .with_context(|| format!("failed to write {}", output_path.display()))?;

    Ok(outcome)
}

pub fn load_save_from_bytes(file_path: &Path, data: &[u8], user_id: &str) -> Result<Bl4LoadedSave> {
    let yaml_bytes = decrypt_sav_to_yaml(data, user_id)?;
    let root_value: Value = serde_yaml::from_slice(&yaml_bytes)?;

    let summary = summarize_from_value(file_path, &root_value);

    Ok(Bl4LoadedSave {
        summary,
        yaml: root_value,
    })
}

pub fn summarize_from_value(file_path: &Path, root_value: &Value) -> Bl4SaveSummary {
    let mut summary = Bl4SaveSummary {
        file_name: file_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| file_path.to_string_lossy().to_string()),
        ..Bl4SaveSummary::default()
    };

    let root_map = match root_value.as_mapping() {
        Some(map) => map,
        None => return summary,
    };

    if let Some(state_map) = get_mapping(root_map, "state") {
        summary.char_guid = get_string(state_map, "char_guid").map(|s| s.trim().to_string());
        summary.class = get_string(state_map, "class");
        summary.char_name = get_string(state_map, "char_name");
        summary.player_difficulty = get_string(state_map, "player_difficulty");

        if let Some(experience_seq) = get_sequence(state_map, "experience") {
            for entry in experience_seq {
                if let Some(entry_map) = entry.as_mapping() {
                    if let Some(entry_type) = get_string(entry_map, "type") {
                        match entry_type.as_str() {
                            "Character" => {
                                summary.character_level = get_i32(entry_map, "level");
                                summary.character_experience = get_i64(entry_map, "points");
                            }
                            "Specialization" => {
                                summary.specialization_level = get_i32(entry_map, "level");
                                summary.specialization_points = get_i64(entry_map, "points");
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        if let Some(currencies_map) = get_mapping(state_map, "currencies") {
            for (key, value) in currencies_map {
                if let Some(key_str) = value_key_to_string(key) {
                    if let Some(amount) = value.as_i64() {
                        summary.currencies.insert(key_str, amount);
                    }
                }
            }
        }

        if let Some(ammo_map) = get_mapping(state_map, "ammo") {
            for (key, value) in ammo_map {
                if let Some(key_str) = value_key_to_string(key) {
                    if let Some(amount) = value.as_i64() {
                        if let Ok(as_i32) = amount.try_into() {
                            summary.ammo.insert(key_str, as_i32);
                        }
                    }
                }
            }
        }

        if let Some(inventory_map) = get_mapping(state_map, "inventory") {
            if let Some(items_map) = get_mapping(inventory_map, "items") {
                if let Some(backpack_map) = get_mapping(items_map, "backpack") {
                    for (slot_key, slot_value) in backpack_map {
                        if let Some(slot_name) = value_key_to_string(slot_key) {
                            if let Some(item_map) = slot_value.as_mapping() {
                                if let Some(serial) = get_string(item_map, "serial") {
                                    let state_flags = get_i64(item_map, "state_flags")
                                        .and_then(|v| v.try_into().ok());
                                    summary.inventory.push(Bl4InventoryItem {
                                        slot: slot_name,
                                        serial,
                                        state_flags,
                                    });
                                }
                            }
                        }
                    }
                }
            }

            if let Some(equip_seq) = get_sequence(inventory_map, "equip_slots_unlocked") {
                let mut slots: Vec<i32> = equip_seq
                    .iter()
                    .filter_map(|value| value.as_i64())
                    .filter_map(|raw| raw.try_into().ok())
                    .collect();
                slots.sort_unstable();
                slots.dedup();
                summary.equip_slots_unlocked = slots;
            }
        }

        if let Some(actor_parts_map) = get_mapping(state_map, "gbxactorparts") {
            if let Some(character_parts) = get_string(actor_parts_map, "character") {
                let parsed = parse_actor_part_list(&character_parts);
                summary.cosmetics.body = find_part_value(&parsed, &["Cosmetics_", "_Body"]);
                summary.cosmetics.head = find_part_value(&parsed, &["Cosmetics_", "_Head"]);
                summary.cosmetics.skin = find_part_value(&parsed, &["Cosmetics_", "_Skin"]);
                summary.cosmetics.primary_color =
                    find_part_value(&parsed, &["Cosmetics_Colorization_Primary"]);
                summary.cosmetics.secondary_color =
                    find_part_value(&parsed, &["Cosmetics_Colorization_Secondary"]);
                summary.cosmetics.tertiary_color =
                    find_part_value(&parsed, &["Cosmetics_Colorization_Tertiary"]);
            }

            if let Some(echo_parts) = get_string(actor_parts_map, "echo4") {
                let parsed = parse_actor_part_list(&echo_parts);
                summary.cosmetics.echo_body =
                    find_part_value(&parsed, &["cosmetics_echo4_body", "Cosmetics_Echo4_Body"]);
                summary.cosmetics.echo_attachment = find_part_value(
                    &parsed,
                    &["Cosmetics_Echo4_Attachment", "cosmetics_echo4_attachment"],
                );
                summary.cosmetics.echo_skin =
                    find_part_value(&parsed, &["cosmetics_echo4_skin", "Cosmetics_Echo4_Skin"]);
            }

            if let Some(vehicle_parts) = get_string(actor_parts_map, "vehicle") {
                let parsed = parse_actor_part_list(&vehicle_parts);
                if let Some(value) = find_part_value(&parsed, &["Cosmetics_Vehicle", "Vehicle_Mat"])
                {
                    summary.cosmetics.vehicle_skin = Some(value.clone());
                    summary.vehicle_loadout.vehicle_cosmetic = Some(value);
                }
            }
        }

        summary.vehicle_loadout.personal_vehicle = get_string(state_map, "personal_vehicle");
        summary.vehicle_loadout.hover_drive = get_string(state_map, "hover_drive");
        summary.vehicle_loadout.vehicle_weapon_slot = get_i32(state_map, "vehicle_weapon_slot");

        if let Some(rewards_seq) = get_sequence(state_map, "unique_rewards") {
            let mut rewards: Vec<String> = rewards_seq
                .iter()
                .filter_map(Value::as_str)
                .map(|s| s.to_string())
                .collect();
            rewards.sort();
            rewards.dedup();
            summary.unique_rewards = rewards;
        }

        let mut progression_location_state = true;
        let progression_map = if let Some(map) = get_mapping(state_map, "progression") {
            Some(map)
        } else if let Some(map) = get_mapping(root_map, "progression") {
            progression_location_state = false;
            Some(map)
        } else {
            None
        };
        summary.progression_in_state = progression_location_state;
        if let Some(progression_map) = progression_map {
            if let Some(point_pools_map) = get_mapping(progression_map, "point_pools") {
                for (key, value) in point_pools_map {
                    if let Some(key_str) = value_key_to_string(key) {
                        if let Some(amount) = value.as_i64() {
                            match key_str.as_str() {
                                "characterprogresspoints" => {
                                    summary.point_pools.character_progress = Some(amount);
                                }
                                "specializationtokenpool" => {
                                    summary.point_pools.specialization_tokens = Some(amount);
                                }
                                "echotokenprogresspoints" => {
                                    summary.point_pools.echo_tokens = Some(amount);
                                }
                                _ => {
                                    summary.point_pools.other.insert(key_str, amount);
                                }
                            }
                        }
                    }
                }
            }

            if let Some(graphs_seq) = get_sequence(progression_map, "graphs") {
                for graph in graphs_seq {
                    if let Some(graph_map) = graph.as_mapping() {
                        let graph_name = get_string(graph_map, "name").unwrap_or_default();
                        let mut nodes = Vec::new();
                        if let Some(nodes_seq) = get_sequence(graph_map, "nodes") {
                            for node in nodes_seq {
                                if let Some(node_map) = node.as_mapping() {
                                    let node_name =
                                        get_string(node_map, "name").unwrap_or_default();
                                    let points_spent = get_i32(node_map, "points_spent");
                                    let activation_level = get_i32(node_map, "activation_level");
                                    let is_activated = node_map
                                        .get(&Value::String("is_activated".into()))
                                        .and_then(Value::as_bool);

                                    if !node_name.is_empty()
                                        || points_spent.is_some()
                                        || activation_level.is_some()
                                        || is_activated.is_some()
                                    {
                                        summary.sdu_levels.record_node(
                                            &node_name,
                                            points_spent,
                                            activation_level,
                                            is_activated,
                                        );

                                        nodes.push(Bl4SkillNode {
                                            name: node_name,
                                            points_spent,
                                            activation_level,
                                            is_activated,
                                        });
                                    }
                                }
                            }
                        }

                        if !nodes.is_empty() {
                            summary.skill_trees.push(Bl4SkillTree {
                                name: graph_name,
                                group_def_name: get_string(graph_map, "group_def_name"),
                                nodes,
                            });
                        }
                    }
                }
            }
        }

        let mut missions_location_state = true;
        let missions_map = if let Some(map) = get_mapping(state_map, "missions") {
            Some(map)
        } else if let Some(map) = get_mapping(root_map, "missions") {
            missions_location_state = false;
            Some(map)
        } else {
            None
        };
        summary.missions_in_state = missions_location_state;
        if let Some(missions_map) = missions_map {
            if let Some(tracked_seq) = get_sequence(missions_map, "tracked_missions") {
                for entry in tracked_seq {
                    match entry {
                        Value::Null => summary.tracked_missions_need_none = true,
                        Value::String(name) if name == "none" => {
                            summary.tracked_missions_need_none = true;
                        }
                        Value::String(name) => {
                            if !name.trim().is_empty() {
                                summary.tracked_missions.push(name.clone());
                            }
                        }
                        _ => {}
                    }
                }
            }

            if let Some(local_sets_map) = get_mapping(missions_map, "local_sets") {
                for (set_key, set_value) in local_sets_map {
                    let set_name = value_key_to_string(set_key).unwrap_or_default();
                    if let Some(set_map) = set_value.as_mapping() {
                        if let Some(missions) = get_mapping(set_map, "missions") {
                            for (mission_key, mission_value) in missions {
                                let mission_name =
                                    value_key_to_string(mission_key).unwrap_or_default();
                                let status = mission_value
                                    .as_mapping()
                                    .and_then(|mission_map| get_string(mission_map, "status"));
                                let mission_status = Bl4MissionStatus {
                                    set: set_name.clone(),
                                    mission: mission_name,
                                    status: status.clone(),
                                };
                                if status
                                    .as_deref()
                                    .map(mission_status_is_active)
                                    .unwrap_or(false)
                                {
                                    summary.active_missions.push(mission_status.clone());
                                }
                                summary.missions.push(mission_status);
                            }
                        }
                    }
                }
            }
        }
    }

    summary.inventory.sort_by(|a, b| a.slot.cmp(&b.slot));
    summary.tracked_missions.sort();
    summary.active_missions.sort();

    if let Some(unlockables_map) = get_mapping(root_map, "unlockables") {
        for (category_key, category_value) in unlockables_map {
            let Some(category_name) = value_key_to_string(category_key) else {
                continue;
            };
            let Some(category_map) = category_value.as_mapping() else {
                continue;
            };
            if let Some(entries_seq) = get_sequence(category_map, "entries") {
                let mut entries: Vec<String> = entries_seq
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|s| s.to_string())
                    .collect();
                entries.sort();
                entries.dedup();
                summary.unlockables.insert(category_name, entries);
            }
        }
    }

    summary
}

pub fn apply_edit_state(root_value: &mut Value, edits: &Bl4EditState) -> Result<()> {
    let Some(root_map) = mapping_mut(root_value) else {
        return Ok(());
    };
    let Some(state_value) = root_map.get_mut(&Value::String("state".into())) else {
        return Ok(());
    };
    let existing_unique_rewards: Vec<String>;
    let unlockables_copy = edits.unlockables.clone();

    {
        let Some(state_map) = mapping_mut(state_value) else {
            return Ok(());
        };

        if let Some(guid) = &edits.char_guid {
            state_map.insert(
                Value::String("char_guid".into()),
                Value::String(guid.clone()),
            );
        }
        if let Some(name) = &edits.char_name {
            state_map.insert(
                Value::String("char_name".into()),
                Value::String(name.clone()),
            );
        }

        if let Some(difficulty) = &edits.player_difficulty {
            state_map.insert(
                Value::String("player_difficulty".into()),
                Value::String(difficulty.clone()),
            );
        }

        if let Some(class_name) = &edits.class_name {
            state_map.insert(
                Value::String("class".into()),
                Value::String(class_name.clone()),
            );
        }

        if let Some(experience_value) = state_map.get_mut(&Value::String("experience".into())) {
            if let Some(experience_seq) = sequence_mut(experience_value) {
                for entry in experience_seq {
                    if let Some(entry_map) = mapping_mut(entry) {
                        let entry_type = entry_map
                            .get(&Value::String("type".into()))
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        match entry_type {
                            "Character" => {
                                if let Some(level) = edits.character_level {
                                    entry_map.insert(
                                        Value::String("level".into()),
                                        Value::Number(Number::from(level)),
                                    );
                                }
                                if let Some(points) = edits.character_experience {
                                    entry_map.insert(
                                        Value::String("points".into()),
                                        Value::Number(Number::from(points)),
                                    );
                                }
                            }
                            "Specialization" => {
                                if let Some(level) = edits.specialization_level {
                                    entry_map.insert(
                                        Value::String("level".into()),
                                        Value::Number(Number::from(level)),
                                    );
                                }
                                if let Some(points) =
                                    edits.specialization_points.or(edits.ability_points)
                                {
                                    entry_map.insert(
                                        Value::String("points".into()),
                                        Value::Number(Number::from(points)),
                                    );
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        if !edits.currencies.is_empty() {
            let currencies_value = state_map
                .entry(Value::String("currencies".into()))
                .or_insert_with(|| Value::Mapping(Mapping::new()));
            if let Some(currencies_map) = mapping_mut(currencies_value) {
                for (key, value) in &edits.currencies {
                    currencies_map.insert(
                        Value::String(key.clone()),
                        Value::Number(Number::from(*value)),
                    );
                }
            }
        }

        if !edits.ammo.is_empty() {
            let ammo_value = state_map
                .entry(Value::String("ammo".into()))
                .or_insert_with(|| Value::Mapping(Mapping::new()));
            if let Some(ammo_map) = mapping_mut(ammo_value) {
                for (key, value) in &edits.ammo {
                    ammo_map.insert(
                        Value::String(key.clone()),
                        Value::Number(Number::from(*value)),
                    );
                }
            }
        }

        existing_unique_rewards = get_sequence(state_map, "unique_rewards")
            .map(|seq| {
                seq.iter()
                    .filter_map(Value::as_str)
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default();

        if edits.progression_in_state {
            let progression_value = state_map
                .entry(Value::String("progression".into()))
                .or_insert_with(|| Value::Mapping(Mapping::new()));
            if let Some(progression_map) = mapping_mut(progression_value) {
                apply_progression_edits(progression_map, edits);
            }
        }

        let inventory_value = state_map
            .entry(Value::String("inventory".into()))
            .or_insert_with(|| Value::Mapping(Mapping::new()));
        if let Some(inventory_map) = mapping_mut(inventory_value) {
            let items_value = inventory_map
                .entry(Value::String("items".into()))
                .or_insert_with(|| Value::Mapping(Mapping::new()));
            if let Some(items_map) = mapping_mut(items_value) {
                let backpack_key = Value::String("backpack".into());
                if !items_map.contains_key(&backpack_key) {
                    items_map.insert(backpack_key.clone(), Value::Mapping(Mapping::new()));
                }
                if let Some(backpack_value) = items_map.get_mut(&backpack_key) {
                    if let Some(backpack_map) = mapping_mut(backpack_value) {
                        for item in &edits.inventory {
                            let slot_key = Value::String(item.slot.clone());
                            if !backpack_map.contains_key(&slot_key) {
                                backpack_map.insert(slot_key.clone(), Value::Mapping(Mapping::new()));
                            }
                            if let Some(slot_value) = backpack_map.get_mut(&slot_key) {
                                if let Some(slot_map) = mapping_mut(slot_value) {
                                    slot_map.insert(
                                        Value::String("serial".into()),
                                        Value::String(item.serial.clone()),
                                    );
                                    match item.state_flags {
                                        Some(flags) => {
                                            slot_map.insert(
                                                Value::String("state_flags".into()),
                                                Value::Number(Number::from(flags)),
                                            );
                                        }
                                        None => {
                                            slot_map
                                                .remove(&Value::String("state_flags".into()));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let mut slots = edits.equip_slots_unlocked.clone();
            slots.sort_unstable();
            slots.dedup();
            let slot_values: Vec<Value> = slots
                .into_iter()
                .map(|slot| Value::Number(Number::from(slot)))
                .collect();
            inventory_map.insert(
                Value::String("equip_slots_unlocked".into()),
                Value::Sequence(slot_values),
            );
        }

        set_optional_string(
            state_map,
            "personal_vehicle",
            &edits.vehicle_loadout.personal_vehicle,
        );
        set_optional_string(state_map, "hover_drive", &edits.vehicle_loadout.hover_drive);
        set_optional_i32(
            state_map,
            "vehicle_weapon_slot",
            &edits.vehicle_loadout.vehicle_weapon_slot,
        );

        if state_map.contains_key(&Value::String("vehicle_cosmetic".into())) {
            set_optional_string(
                state_map,
                "vehicle_cosmetic",
                &edits.vehicle_loadout.vehicle_cosmetic,
            );
        }

        apply_cosmetics(state_map, &edits.cosmetics, &edits.vehicle_loadout);

        if edits.missions_in_state && (edits.tracked_missions_dirty || !edits.missions.is_empty())
        {
            apply_missions(
                state_map,
                &edits.tracked_missions,
                &edits.missions,
                edits.tracked_missions_need_none,
            );
        }

        let mut final_unique_rewards = if edits.unique_rewards_dirty {
            edits.unique_rewards.clone()
        } else {
            existing_unique_rewards.clone()
        };
        let mut unique_changed = edits.unique_rewards_dirty;

        let unlockable_rewards = collect_unlockable_rewards(&edits.unlockables);
        if !unlockable_rewards.is_empty() {
            final_unique_rewards = final_unique_rewards
                .into_iter()
                .filter(|reward| !is_unlockable_reward_id(reward))
                .collect();
            final_unique_rewards.extend(unlockable_rewards);
            unique_changed = true;
        }

        if unique_changed {
            final_unique_rewards.sort_unstable();
            final_unique_rewards.dedup();

            if !final_unique_rewards.is_empty() {
                let reward_values: Vec<Value> =
                    final_unique_rewards.iter().cloned().map(Value::String).collect();
                state_map.insert(
                    Value::String("unique_rewards".into()),
                    Value::Sequence(reward_values),
                );
            } else {
                state_map.remove(&Value::String("unique_rewards".into()));
            }
        }
    }

    if !edits.progression_in_state {
        let progression_value = root_map
            .entry(Value::String("progression".into()))
            .or_insert_with(|| Value::Mapping(Mapping::new()));
        if let Some(progression_map) = mapping_mut(progression_value) {
            apply_progression_edits(progression_map, edits);
        }
    }

    if !edits.missions_in_state && (edits.tracked_missions_dirty || !edits.missions.is_empty()) {
        apply_missions(
            root_map,
            &edits.tracked_missions,
            &edits.missions,
            edits.tracked_missions_need_none,
        );
    }

    if !unlockables_copy.is_empty() {
        let unlockables_value = root_map
            .entry(Value::String("unlockables".into()))
            .or_insert_with(|| Value::Mapping(Mapping::new()));
        if let Some(unlockables_map) = mapping_mut(unlockables_value) {
            for (category, entries) in unlockables_copy {
                let entry_values: Vec<Value> = entries.into_iter().map(Value::String).collect();
                let category_key = Value::String(category.clone());
                let category_value = unlockables_map
                    .entry(category_key)
                    .or_insert_with(|| Value::Mapping(Mapping::new()));
                if let Some(category_map) = mapping_mut(category_value) {
                    category_map.insert(
                        Value::String("entries".into()),
                        Value::Sequence(entry_values),
                    );
                }
            }
        }
    }

    Ok(())
}

pub fn yaml_to_encrypted_sav(yaml: &Value, steamid: &str) -> Result<Vec<u8>> {
    let yaml_string =
        serde_yaml::to_string(yaml).context("failed to serialize YAML before encrypting")?;
    encrypt_yaml_to_sav(yaml_string.as_bytes(), steamid)
}

fn apply_progression_edits(progression_map: &mut Mapping, edits: &Bl4EditState) {
    let point_pools_value = progression_map
        .entry(Value::String("point_pools".into()))
        .or_insert_with(|| Value::Mapping(Mapping::new()));
    if let Some(point_pools_map) = mapping_mut(point_pools_value) {
        if let Some(value) = edits
            .point_pools
            .character_progress
            .or(edits.ability_points)
        {
            point_pools_map.insert(
                Value::String("characterprogresspoints".into()),
                Value::Number(Number::from(value)),
            );
        }
        if let Some(value) = edits
            .point_pools
            .specialization_tokens
            .or(edits.specialization_points)
        {
            point_pools_map.insert(
                Value::String("specializationtokenpool".into()),
                Value::Number(Number::from(value)),
            );
        }
        if let Some(value) = edits.point_pools.echo_tokens {
            point_pools_map.insert(
                Value::String("echotokenprogresspoints".into()),
                Value::Number(Number::from(value)),
            );
        }
        for (key, value) in &edits.point_pools.other {
            point_pools_map.insert(
                Value::String(key.clone()),
                Value::Number(Number::from(*value)),
            );
        }
    }

    let graphs_value = progression_map
        .entry(Value::String("graphs".into()))
        .or_insert_with(|| Value::Sequence(Vec::new()));
    if let Some(graphs_seq) = sequence_mut(graphs_value) {
        let skill_tree_overrides: BTreeMap<_, _> = edits
            .skill_trees
            .iter()
            .map(|tree| (tree.name.as_str(), tree))
            .collect();
        // Apply SDU graph if caller marked dirty OR any requested SDU level is non-zero.
        // This avoids missing updates when the dirty flag is not set due to baseline issues.
        let any_non_zero = edits.sdu_levels.backpack
            | edits.sdu_levels.pistol
            | edits.sdu_levels.smg
            | edits.sdu_levels.assault_rifle
            | edits.sdu_levels.shotgun
            | edits.sdu_levels.sniper
            | edits.sdu_levels.heavy
            | edits.sdu_levels.grenade
            | edits.sdu_levels.bank
            | edits.sdu_levels.lost_loot;
        if edits.sdu_levels_dirty || any_non_zero != 0 {
            let mut sdu_found = false;

            for idx in 0..graphs_seq.len() {
                let Some(graph_map) = mapping_mut(&mut graphs_seq[idx]) else {
                    continue;
                };
                let graph_name = graph_map
                    .get(&Value::String("name".into()))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if graph_name == "sdu_upgrades" {
                    sdu_found = true;
                    let nodes_value = graph_map
                        .entry(Value::String("nodes".into()))
                        .or_insert_with(|| Value::Sequence(Vec::new()));
                    if let Some(nodes_seq) = sequence_mut(nodes_value) {
                        apply_sdu_levels_to_nodes(nodes_seq, &edits.sdu_levels);
                    }
                }
            }

            if !sdu_found {
                let mut graph_map = Mapping::new();
                graph_map.insert(
                    Value::String("name".into()),
                    Value::String("sdu_upgrades".into()),
                );
                graph_map.insert(
                    Value::String("group_def_name".into()),
                    Value::String("Oak2_GlobalProgressGraph_Group".into()),
                );
                let mut nodes_seq = Vec::new();
                apply_sdu_levels_to_nodes(&mut nodes_seq, &edits.sdu_levels);
                graph_map.insert(Value::String("nodes".into()), Value::Sequence(nodes_seq));
                graphs_seq.push(Value::Mapping(graph_map));
            }
        }

        if !skill_tree_overrides.is_empty() {
            let mut applied_graphs: HashSet<String> = HashSet::new();
            for idx in 0..graphs_seq.len() {
                let Some(graph_map) = mapping_mut(&mut graphs_seq[idx]) else {
                    continue;
                };
                let graph_name = graph_map
                    .get(&Value::String("name".into()))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if graph_name == "sdu_upgrades" {
                    continue;
                }
                if let Some(tree) = skill_tree_overrides.get(graph_name) {
                    applied_graphs.insert(tree.name.clone());
                    let nodes_value = graph_map
                        .entry(Value::String("nodes".into()))
                        .or_insert_with(|| Value::Sequence(Vec::new()));
                    let Some(nodes_seq) = sequence_mut(nodes_value) else {
                        continue;
                    };
                    apply_skill_tree_override(nodes_seq, tree);
                }
            }

            for tree in edits.skill_trees.iter() {
                if applied_graphs.contains(&tree.name) {
                    continue;
                }

                let mut graph_map = Mapping::new();
                graph_map.insert(
                    Value::String("name".into()),
                    Value::String(tree.name.clone()),
                );
                if let Some(group) = tree
                    .group_def_name
                    .clone()
                    .filter(|value| !value.trim().is_empty())
                {
                    graph_map.insert(Value::String("group_def_name".into()), Value::String(group));
                } else if let Some(fallback) = infer_group_def_name(&tree.name) {
                    graph_map.insert(
                        Value::String("group_def_name".into()),
                        Value::String(fallback),
                    );
                }
                let mut nodes_seq = Vec::new();
                apply_skill_tree_override(&mut nodes_seq, tree);
                graph_map.insert(Value::String("nodes".into()), Value::Sequence(nodes_seq));
                graphs_seq.push(Value::Mapping(graph_map));
            }
        }
    }
}

fn apply_sdu_levels_to_nodes(nodes_seq: &mut Vec<Value>, levels: &Bl4SduLevels) {
    // Rebuild SDU nodes to exactly match unlocked tiers per family, using only
    // name + points_spent entries. Avoid activation flags which are not present
    // in working saves and appear to be rejected by the game.

    // Helper to push desired nodes for a family
    let mut desired: Vec<(String, i64)> = Vec::new();
    let mut push_family = |prefix: &str, max_tiers: i32, target: i32| {
        if target <= 0 {
            return;
        }
        let limit = i32::min(max_tiers, target);
        for idx in 1..=limit {
            let name = format!("{}{:02}", prefix, idx);
            desired.push((name, default_sdu_cost(idx)));
        }
    };

    // Ammo families in BL4 go to tier 7
    push_family("Ammo_Pistol_", 7, levels.pistol);
    push_family("Ammo_SMG_", 7, levels.smg);
    push_family("Ammo_AR_", 7, levels.assault_rifle);
    push_family("Ammo_SG_", 7, levels.shotgun);
    push_family("Ammo_SR_", 7, levels.sniper);

    // Backpack always 8-tier family.
    push_family("Backpack_", 8, levels.backpack);

    // Some BL4 saves include Bank/Lost_Loot nodes alongside SDUs; if caller didn't
    // specify them but other SDUs are requested, include them at max to mirror
    // working saves so the graph is accepted by the game.
    let any_sdu_requested =
        levels.backpack > 0
            || levels.pistol > 0
            || levels.smg > 0
            || levels.assault_rifle > 0
            || levels.shotgun > 0
            || levels.sniper > 0;
    let bank_target = if levels.bank == 0 && any_sdu_requested { 8 } else { levels.bank };
    let lost_loot_target = if levels.lost_loot == 0 && any_sdu_requested {
        8
    } else {
        levels.lost_loot
    };
    push_family("Bank_", 8, bank_target);
    push_family("Lost_Loot_", 8, lost_loot_target);

    // Replace nodes with the exact desired list (preserve order by family/tier)
    nodes_seq.clear();
    for (name, cost) in desired {
        let mut node_map = Mapping::new();
        node_map.insert(Value::String("name".into()), Value::String(name));
        node_map.insert(
            Value::String("points_spent".into()),
            Value::Number(Number::from(cost)),
        );
        nodes_seq.push(Value::Mapping(node_map));
    }

    // Development-time validation: ensure we only emit name + points_spent keys.
    #[cfg(debug_assertions)]
    {
        for node in nodes_seq.iter() {
            if let Some(map) = node.as_mapping() {
                for key in map.keys() {
                    let Some(k) = key.as_str() else { continue };
                    assert!(k == "name" || k == "points_spent", "unexpected SDU node key: {}", k);
                }
            }
        }
    }
}

fn apply_skill_tree_override(nodes_seq: &mut Vec<Value>, tree: &Bl4SkillTree) {
    for override_node in &tree.nodes {
        let node_name = override_node.name.as_str();
        let mut target_map = None;
        for node in nodes_seq.iter_mut() {
            if let Some(node_map) = mapping_mut(node) {
                let existing_name = node_map
                    .get(&Value::String("name".into()))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if existing_name == node_name {
                    target_map = Some(node_map);
                    break;
                }
            }
        }

        let node_map = if let Some(map) = target_map {
            map
        } else {
            let mut new_map = Mapping::new();
            new_map.insert(
                Value::String("name".into()),
                Value::String(override_node.name.clone()),
            );
            nodes_seq.push(Value::Mapping(new_map));
            mapping_mut(nodes_seq.last_mut().expect("node mapping")).expect("node mapping")
        };

        match override_node.points_spent {
            Some(value) => {
                node_map.insert(
                    Value::String("points_spent".into()),
                    Value::Number(Number::from(value)),
                );
            }
            None => {
                node_map.remove(&Value::String("points_spent".into()));
            }
        }

        match override_node.activation_level {
            Some(value) => {
                node_map.insert(
                    Value::String("activation_level".into()),
                    Value::Number(Number::from(value)),
                );
            }
            None => {
                node_map.remove(&Value::String("activation_level".into()));
            }
        }

        match override_node.is_activated {
            Some(value) => {
                node_map.insert(Value::String("is_activated".into()), Value::Bool(value));
            }
            None => {
                node_map.remove(&Value::String("is_activated".into()));
            }
        }
    }
}

fn infer_group_def_name(tree_name: &str) -> Option<String> {
    let candidate = tree_name
        .rsplit(|c| c == '/' || c == '\\' || c == '.')
        .next()
        .unwrap_or(tree_name)
        .trim();
    if candidate.is_empty() {
        return None;
    }
    if candidate.starts_with("ProgressGroup_") {
        return Some(candidate.to_string());
    }
    let sanitized: String = candidate
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if sanitized.is_empty() {
        return None;
    }
    Some(format!("ProgressGroup_{}", sanitized))
}

fn collect_unlockable_rewards(unlockables: &BTreeMap<String, Vec<String>>) -> Vec<String> {
    let mut rewards = Vec::new();
    for (category, entries) in unlockables {
        for entry in entries {
            if let Some(reward) = unlockable_entry_to_reward(category, entry) {
                rewards.push(reward);
            }
        }
    }
    rewards.sort_unstable();
    rewards.dedup();
    rewards
}

fn unlockable_entry_to_reward(category: &str, entry: &str) -> Option<String> {
    let suffix = entry.split('.').last().unwrap_or(entry);
    match category {
        "unlockable_hoverdrives" => Some(format!(
            "Reward_HoverDrive_{}",
            format_unlockable_suffix(suffix)
        )),
        _ => None,
    }
}

fn format_unlockable_suffix(raw: &str) -> String {
    raw.split('_')
        .map(|segment| {
            if segment.chars().all(|c| c.is_ascii_digit()) {
                segment.to_string()
            } else {
                let mut chars = segment.chars();
                match chars.next() {
                    Some(first) => {
                        first.to_ascii_uppercase().to_string()
                            + &chars.as_str().to_ascii_lowercase()
                    }
                    None => String::new(),
                }
            }
        })
        .collect::<Vec<_>>()
        .join("_")
}

fn is_unlockable_reward_id(reward: &str) -> bool {
    reward.starts_with("Reward_HoverDrive_")
}

fn desired_sdu_level(name: &str, levels: &Bl4SduLevels) -> Option<i32> {
    if name.starts_with("Ammo_Pistol") {
        Some(levels.pistol)
    } else if name.starts_with("Ammo_SMG") {
        Some(levels.smg)
    } else if name.starts_with("Ammo_AR") {
        Some(levels.assault_rifle)
    } else if name.starts_with("Ammo_SG") {
        Some(levels.shotgun)
    } else if name.starts_with("Ammo_SR") {
        Some(levels.sniper)
    } else if name.starts_with("Backpack") {
        Some(levels.backpack)
    } else if name.starts_with("Bank") {
        Some(levels.bank)
    } else if name.starts_with("Lost_Loot") {
        Some(levels.lost_loot)
    } else {
        None
    }
}

fn ensure_sdu_nodes(nodes: &mut Vec<Value>, levels: &Bl4SduLevels) {
    // Kept for compatibility, but restrict to BL4 families and correct tier caps.
    const PREFIXES: &[(&str, usize)] = &[
        ("Ammo_Pistol_", 7),
        ("Ammo_SMG_", 7),
        ("Ammo_AR_", 7),
        ("Ammo_SG_", 7),
        ("Ammo_SR_", 7),
        ("Backpack_", 8),
        ("Bank_", 8),
        ("Lost_Loot_", 8),
    ];

    let mut existing: HashSet<String> = nodes
        .iter()
        .filter_map(|node| {
            node.as_mapping().and_then(|map| {
                map.get(&Value::String("name".into()))
                    .and_then(Value::as_str)
                    .map(|s| s.to_string())
            })
        })
        .collect();

    for (prefix, count) in PREFIXES {
        let probe_name = format!("{prefix}{:02}", 1);
        let desired_level = desired_sdu_level(&probe_name, levels).unwrap_or_default();

        for idx in 1..=*count {
            let Some(idx_level) = i32::try_from(idx).ok() else {
                continue;
            };
            if idx_level > desired_level {
                continue;
            }
            let name = format!("{}{:02}", prefix, idx);
            if existing.contains(&name) {
                continue;
            }
            let mut node_map = Mapping::new();
            node_map.insert(Value::String("name".into()), Value::String(name.clone()));
            node_map.insert(
                Value::String("points_spent".into()),
                Value::Number(Number::from(0)),
            );
            nodes.push(Value::Mapping(node_map));
            existing.insert(name);
        }
    }
}

fn default_sdu_cost(level: i32) -> i64 {
    match level {
        1 => 5,
        2 => 10,
        3 => 20,
        4 => 30,
        5 => 50,
        6 => 80,
        7 => 120,
        8 => 235,
        _ => 0,
    }
}

#[allow(deprecated)]
fn aes_ecb_encrypt(data: &mut [u8], key: &[u8; 32]) {
    if data.is_empty() {
        return;
    }
    let mut cipher = Aes256::new_from_slice(key).expect("key length must be 32 bytes");
    for chunk in data.chunks_exact_mut(16) {
        let block = GenericArray::from_mut_slice(chunk);
        cipher.encrypt_block_mut(block);
    }
}

#[allow(deprecated)]
fn aes_ecb_decrypt(data: &mut [u8], key: &[u8; 32]) {
    if data.is_empty() {
        return;
    }
    let mut cipher = Aes256::new_from_slice(key).expect("key length must be 32 bytes");
    for chunk in data.chunks_exact_mut(16) {
        let block = GenericArray::from_mut_slice(chunk);
        cipher.decrypt_block_mut(block);
    }
}

fn pkcs7_pad(block_size: usize, data: &[u8]) -> Vec<u8> {
    assert!(block_size > 0);
    let pad_len = block_size - (data.len() % block_size);
    let mut result = data.to_vec();
    result.extend(std::iter::repeat(pad_len as u8).take(pad_len));
    result
}

fn pkcs7_unpad(block_size: usize, data: &[u8]) -> Result<Vec<u8>> {
    if data.is_empty() || data.len() % block_size != 0 {
        bail!("invalid padding");
    }
    let pad_len = *data.last().ok_or_else(|| anyhow!("missing padding"))? as usize;
    if pad_len == 0 || pad_len > block_size || pad_len > data.len() {
        bail!("invalid padding length");
    }
    if !data[data.len() - pad_len..]
        .iter()
        .all(|&byte| byte as usize == pad_len)
    {
        bail!("invalid padding bytes");
    }
    Ok(data[..data.len() - pad_len].to_vec())
}

pub fn bit_pack_decode(serial: &str) -> Vec<u8> {
    let payload = serial.strip_prefix("@Ug").unwrap_or(serial);
    let mut values = Vec::with_capacity(payload.len());
    let charset = CHARSET.as_bytes();
    for byte in payload.bytes() {
        if let Some(pos) = charset.iter().position(|&c| c == byte) {
            values.push(pos as u8);
        }
    }

    let mut result = Vec::new();
    let mut accumulator: u32 = 0;
    let mut bit_count = 0;

    for val in values {
        accumulator = (accumulator << 6) | (val as u32);
        bit_count += 6;

        while bit_count >= 8 {
            bit_count -= 8;
            let byte = ((accumulator >> bit_count) & 0xFF) as u8;
            result.push(byte);
        }
    }

    if bit_count > 0 {
        let byte = ((accumulator << (8 - bit_count)) & 0xFF) as u8;
        result.push(byte);
    }

    result
}

pub fn bit_pack_encode(data: &[u8], prefix: &str) -> String {
    let charset = CHARSET.as_bytes();
    let mut result = String::with_capacity(prefix.len() + (data.len() + 2) / 3 * 4);
    result.push_str(prefix);

    let mut accumulator: u32 = 0;
    let mut bit_count = 0;

    for &byte in data {
        accumulator = (accumulator << 8) | (byte as u32);
        bit_count += 8;

        while bit_count >= 6 {
            bit_count -= 6;
            let index = ((accumulator >> bit_count) & 0x3F) as usize;
            if let Some(&ch) = charset.get(index) {
                result.push(ch as char);
            }
        }
    }

    if bit_count > 0 {
        let index = ((accumulator << (6 - bit_count)) & 0x3F) as usize;
        if let Some(&ch) = charset.get(index) {
            result.push(ch as char);
        }
    }

    result
}

fn read_u16_le(data: &[u8], offset: usize) -> Option<u16> {
    if data.len() >= offset + 2 {
        Some(u16::from_le_bytes([data[offset], data[offset + 1]]))
    } else {
        None
    }
}

fn read_u8(data: &[u8], offset: usize) -> Option<u8> {
    data.get(offset).copied()
}

fn decode_weapon(data: &[u8], serial: &str) -> DecodedItem {
    let mut stats = ItemStats::default();

    stats.primary_stat = read_u16_le(data, 0);
    stats.secondary_stat = read_u16_le(data, 12);
    stats.manufacturer = read_u8(data, 4);
    stats.item_class = read_u8(data, 8);
    stats.rarity = read_u8(data, 1);

    if let Some(level) = read_u8(data, 13) {
        if matches!(level, 2 | 34) {
            stats.level = Some(level as u16);
        }
    }

    let confidence = if matches!(data.len(), 24 | 26) {
        "high"
    } else {
        "medium"
    };

    DecodedItem {
        serial: serial.to_string(),
        item_type: "r".into(),
        item_category: "weapon".into(),
        length: data.len(),
        stats,
        confidence: confidence.into(),
        token_stream: Vec::new(),
        tokens: None,
        bit_string: None,
        parts: Vec::new(),
    }
}

fn decode_equipment_e(data: &[u8], serial: &str) -> DecodedItem {
    let mut stats = ItemStats::default();

    stats.primary_stat = read_u16_le(data, 2);
    stats.secondary_stat = read_u16_le(data, 8);
    if data.len() > 38 {
        stats.level = read_u16_le(data, 10);
    }
    stats.manufacturer = read_u8(data, 1);
    stats.item_class = read_u8(data, 3);
    stats.rarity = read_u8(data, 9);

    let confidence = if matches!(stats.manufacturer, Some(49)) {
        "high"
    } else {
        "medium"
    };

    DecodedItem {
        serial: serial.to_string(),
        item_type: "e".into(),
        item_category: "equipment".into(),
        length: data.len(),
        stats,
        confidence: confidence.into(),
        token_stream: Vec::new(),
        tokens: None,
        bit_string: None,
        parts: Vec::new(),
    }
}

fn decode_equipment_d(data: &[u8], serial: &str) -> DecodedItem {
    let mut stats = ItemStats::default();

    stats.primary_stat = read_u16_le(data, 4);
    stats.secondary_stat = read_u16_le(data, 8);
    stats.level = read_u16_le(data, 10);
    stats.manufacturer = read_u8(data, 5);
    stats.item_class = read_u8(data, 6);
    stats.rarity = read_u8(data, 14);

    let confidence = if matches!(stats.manufacturer, Some(15)) {
        "high"
    } else {
        "medium"
    };

    DecodedItem {
        serial: serial.to_string(),
        item_type: "d".into(),
        item_category: "equipment_alt".into(),
        length: data.len(),
        stats,
        confidence: confidence.into(),
        token_stream: Vec::new(),
        tokens: None,
        bit_string: None,
        parts: Vec::new(),
    }
}

fn decode_other_type(data: &[u8], serial: &str, item_type: &str) -> DecodedItem {
    let mut stats = ItemStats::default();

    if let Some(primary) = read_u16_le(data, 0) {
        stats.primary_stat = Some(primary);
    }
    if let Some(secondary) = read_u16_le(data, 2) {
        stats.secondary_stat = Some(secondary);
    }
    stats.manufacturer = read_u8(data, 1);
    stats.rarity = read_u8(data, 2);

    let category = match item_type {
        "w" => "weapon_special",
        "u" => "utility",
        "f" => "consumable",
        "!" => "special",
        _ => "unknown",
    };

    DecodedItem {
        serial: serial.to_string(),
        item_type: item_type.into(),
        item_category: category.into(),
        length: data.len(),
        stats,
        confidence: "low".into(),
        token_stream: Vec::new(),
        tokens: None,
        bit_string: None,
        parts: Vec::new(),
    }
}

pub fn decode_item_serial(serial: &str) -> DecodedItem {
    match Bl4ItemModel::decode(serial) {
        Ok(model) => {
            let manufacturer_idx = model.manufacturer_index;
            let mut stats = ItemStats::default();
            if let Some(level) = model.level {
                let clamped = level.min(u32::from(u16::MAX));
                stats.level = Some(clamped as u16);
            }

            let item_type_label = manufacturer_idx
                .and_then(bl4_data::lookup_item_type)
                .map(|info| format!("{}:{}", info.manufacturer, info.item_type))
                .unwrap_or_else(|| {
                    manufacturer_idx
                        .map(|idx| format!("manufacturer_index:{idx}"))
                        .unwrap_or_else(|| "?".into())
                });

            let item_category = infer_category(&model.tokens);

            DecodedItem {
                serial: serial.to_string(),
                item_type: item_type_label,
                item_category,
                length: model.tokens.len(),
                stats,
                confidence: "structural".into(),
                token_stream: model.tokens.clone(),
                tokens: Some(model.token_text().to_string()),
                bit_string: Some(model.bit_string.clone()),
                parts: model.parts.clone(),
            }
        }
        Err(err) => {
            let mut fallback = legacy_decode_item_serial(serial);
            fallback.tokens = Some(format!("decode error: {err}"));
            fallback
        }
    }
}

fn legacy_decode_item_serial(serial: &str) -> DecodedItem {
    let data = bit_pack_decode(serial);
    let item_type = serial
        .strip_prefix("@Ug")
        .and_then(|s| s.chars().next())
        .map(|c| c.to_string())
        .unwrap_or_else(|| "?".into());

    let mut item = match item_type.as_str() {
        "r" => decode_weapon(&data, serial),
        "e" => decode_equipment_e(&data, serial),
        "d" => decode_equipment_d(&data, serial),
        other => decode_other_type(&data, serial, other),
    };
    item.tokens = None;
    item.bit_string = None;
    item.parts = Vec::new();
    item.token_stream.clear();
    item
}

fn find_int_at_pos(tokens: &[SerialToken], mut pos: usize) -> Option<u32> {
    for token in tokens {
        match token {
            SerialToken::VarInt(value) | SerialToken::VarBit(value) => {
                if pos == 0 {
                    return Some(*value);
                }
                pos -= 1;
            }
            _ => {}
        }
    }
    None
}

fn find_int_token_index(tokens: &[SerialToken], mut pos: usize) -> Option<usize> {
    for (idx, token) in tokens.iter().enumerate() {
        match token {
            SerialToken::VarInt(_) | SerialToken::VarBit(_) => {
                if pos == 0 {
                    return Some(idx);
                }
                pos -= 1;
            }
            _ => {}
        }
    }
    None
}

fn find_level(tokens: &[SerialToken]) -> Option<u32> {
    let mut pos = 2;
    loop {
        let marker = find_int_at_pos(tokens, pos)?;
        if marker == 1 {
            return find_int_at_pos(tokens, pos + 1);
        }
        pos += 2;
        if pos > tokens.len() {
            return None;
        }
    }
}

fn find_level_token_indices(tokens: &[SerialToken]) -> Option<(usize, usize)> {
    let mut pos = 2;
    loop {
        let marker_idx = find_int_token_index(tokens, pos)?;
        let marker_value = match tokens.get(marker_idx)? {
            SerialToken::VarInt(value) | SerialToken::VarBit(value) => *value,
            _ => return None,
        };
        if marker_value == 1 {
            let value_idx = find_int_token_index(tokens, pos + 1)?;
            return Some((marker_idx, value_idx));
        }
        pos += 2;
    }
}

fn infer_category(tokens: &[SerialToken]) -> String {
    let part_count = tokens
        .iter()
        .filter(|token| matches!(token, SerialToken::Part(_)))
        .count();
    if part_count == 0 {
        "unknown".into()
    } else {
        format!("parts:{part_count}")
    }
}

fn decode_parts(
    tokens: &[SerialToken],
    item_info: Option<&bl4_data::ItemTypeEntry>,
) -> Vec<DecodedPart> {
    let mut results = Vec::new();
    let (manufacturer, item_type) = item_info
        .map(|info| (info.manufacturer.as_str(), info.item_type.as_str()))
        .unwrap_or(("", ""));

    for (token_index, token) in tokens.iter().enumerate() {
        let SerialToken::Part(part) = token else {
            continue;
        };

        let mut decoded = DecodedPart {
            token_index,
            index: part.index,
            subtype: part.subtype,
            values: Vec::new(),
            label: None,
            part_type: None,
            description: None,
            effects: None,
            value_labels: Vec::new(),
        };

        if !manufacturer.is_empty() && !item_type.is_empty() {
            if let Some(entry) = bl4_data::lookup_part(manufacturer, item_type, part.index) {
                let label = if entry.model_name.is_empty() {
                    entry.part_type.clone()
                } else if entry.part_type.is_empty() {
                    entry.model_name.clone()
                } else {
                    format!("{} ({})", entry.model_name, entry.part_type)
                };
                if !label.is_empty() {
                    decoded.label = Some(label);
                }
                if !entry.part_type.is_empty() {
                    decoded.part_type = Some(entry.part_type.clone());
                }
                if !entry.description.is_empty() {
                    decoded.description = Some(entry.description.clone());
                }
                if !entry.effects.is_empty() {
                    decoded.effects = Some(entry.effects.clone());
                }
            }
        }

        match part.subtype {
            SerialPartSubType::None => {}
            SerialPartSubType::Int => {
                decoded.values.push(part.value);
                decoded
                    .value_labels
                    .push(resolve_part_label(manufacturer, item_type, part.value));
            }
            SerialPartSubType::List => {
                for value in &part.values {
                    decoded.values.push(*value);
                    decoded
                        .value_labels
                        .push(resolve_part_label(manufacturer, item_type, *value));
                }
            }
        }

        results.push(decoded);
    }

    results
}

fn resolve_part_label(manufacturer: &str, item_type: &str, id: u32) -> Option<String> {
    if manufacturer.is_empty() || item_type.is_empty() {
        return None;
    }

    bl4_data::lookup_part(manufacturer, item_type, id).and_then(|entry| {
        if !entry.model_name.is_empty() {
            Some(entry.model_name.clone())
        } else if !entry.part_type.is_empty() {
            Some(entry.part_type.clone())
        } else {
            None
        }
    })
}

pub fn encode_item_serial(decoded_item: &DecodedItem) -> String {
    if !decoded_item.token_stream.is_empty() {
        if let Ok(serial) = bl4_serial::serialize(&decoded_item.token_stream) {
            return serial;
        }
    }

    let mut data = bit_pack_decode(&decoded_item.serial);

    match decoded_item.item_type.as_str() {
        "r" => {
            if let Some(value) = decoded_item.stats.primary_stat {
                if data.len() >= 2 {
                    data[0..2].copy_from_slice(&value.to_le_bytes());
                }
            }
            if let Some(value) = decoded_item.stats.secondary_stat {
                if data.len() >= 14 {
                    data[12..14].copy_from_slice(&value.to_le_bytes());
                }
            }
            if let Some(value) = decoded_item.stats.rarity {
                if data.len() >= 2 {
                    data[1] = value;
                }
            }
            if let Some(value) = decoded_item.stats.manufacturer {
                if data.len() >= 5 {
                    data[4] = value;
                }
            }
            if let Some(value) = decoded_item.stats.item_class {
                if data.len() >= 9 {
                    data[8] = value;
                }
            }
        }
        "e" => {
            if let Some(value) = decoded_item.stats.primary_stat {
                if data.len() >= 4 {
                    data[2..4].copy_from_slice(&value.to_le_bytes());
                }
            }
            if let Some(value) = decoded_item.stats.secondary_stat {
                if data.len() >= 10 {
                    data[8..10].copy_from_slice(&value.to_le_bytes());
                }
            }
            if let Some(value) = decoded_item.stats.manufacturer {
                if data.len() >= 2 {
                    data[1] = value;
                }
            }
            if let Some(value) = decoded_item.stats.item_class {
                if data.len() >= 4 {
                    data[3] = value;
                }
            }
            if let Some(value) = decoded_item.stats.rarity {
                if data.len() >= 10 {
                    data[9] = value;
                }
            }
        }
        "d" => {
            if let Some(value) = decoded_item.stats.primary_stat {
                if data.len() >= 6 {
                    data[4..6].copy_from_slice(&value.to_le_bytes());
                }
            }
            if let Some(value) = decoded_item.stats.secondary_stat {
                if data.len() >= 10 {
                    data[8..10].copy_from_slice(&value.to_le_bytes());
                }
            }
            if let Some(value) = decoded_item.stats.manufacturer {
                if data.len() >= 6 {
                    data[5] = value;
                }
            }
            if let Some(value) = decoded_item.stats.item_class {
                if data.len() >= 7 {
                    data[6] = value;
                }
            }
        }
        _ => {}
    }

    // Derive the 1-char type code from the original serial, not from the
    // human-readable item_type label stored in decoded metadata.
    let type_code = decoded_item
        .serial
        .strip_prefix("@Ug")
        .and_then(|s| s.chars().next())
        .unwrap_or('r');
    let prefix = format!("@Ug{}", type_code);
    bit_pack_encode(&data, &prefix)
}

fn stats_to_mapping(stats: &ItemStats) -> Mapping {
    let mut mapping = Mapping::new();
    if let Some(value) = stats.primary_stat {
        mapping.insert(
            Value::String("primary_stat".into()),
            Value::Number(Number::from(u64::from(value))),
        );
    }
    if let Some(value) = stats.secondary_stat {
        mapping.insert(
            Value::String("secondary_stat".into()),
            Value::Number(Number::from(u64::from(value))),
        );
    }
    if let Some(value) = stats.level {
        mapping.insert(
            Value::String("level".into()),
            Value::Number(Number::from(u64::from(value))),
        );
    }
    if let Some(value) = stats.rarity {
        mapping.insert(
            Value::String("rarity".into()),
            Value::Number(Number::from(u64::from(value))),
        );
    }
    if let Some(value) = stats.manufacturer {
        mapping.insert(
            Value::String("manufacturer".into()),
            Value::Number(Number::from(u64::from(value))),
        );
    }
    if let Some(value) = stats.item_class {
        mapping.insert(
            Value::String("item_class".into()),
            Value::Number(Number::from(u64::from(value))),
        );
    }
    mapping
}

pub fn find_and_decode_serials_in_yaml(yaml: &Value) -> BTreeMap<String, DecodedItem> {
    let mut decoded = BTreeMap::new();
    search_yaml(yaml, "", &mut decoded);
    decoded
}

fn search_yaml(value: &Value, path: &str, decoded: &mut BTreeMap<String, DecodedItem>) {
    match value {
        Value::String(s) if s.starts_with("@Ug") => {
            let item = decode_item_serial(s);
            if item.confidence != "none" {
                decoded.insert(path.to_string(), item);
            }
        }
        Value::Sequence(seq) => {
            for (index, item) in seq.iter().enumerate() {
                let new_path = format!("{path}[{index}]");
                search_yaml(item, &new_path, decoded);
            }
        }
        Value::Mapping(map) => {
            for (key, value) in map {
                if let Value::String(key_str) = key {
                    let new_path = if path.is_empty() {
                        key_str.clone()
                    } else {
                        format!("{path}.{key_str}")
                    };
                    search_yaml(value, &new_path, decoded);
                }
            }
        }
        Value::Tagged(tagged) => {
            search_yaml(&tagged.value, path, decoded);
        }
        _ => {}
    }
}

pub fn insert_decoded_items_in_yaml(
    yaml: &Value,
    decoded_serials: &BTreeMap<String, DecodedItem>,
) -> Value {
    let mut result = yaml.clone();
    let mut decoded_map = Mapping::new();

    for (path, item) in decoded_serials {
        let mut item_map = Mapping::new();
        item_map.insert(
            Value::String("original_serial".into()),
            Value::String(item.serial.clone()),
        );
        item_map.insert(
            Value::String("item_type".into()),
            Value::String(item.item_type.clone()),
        );
        item_map.insert(
            Value::String("category".into()),
            Value::String(item.item_category.clone()),
        );
        item_map.insert(
            Value::String("confidence".into()),
            Value::String(item.confidence.clone()),
        );

        let stats_map = stats_to_mapping(&item.stats);
        item_map.insert(Value::String("stats".into()), Value::Mapping(stats_map));

        decoded_map.insert(Value::String(path.clone()), Value::Mapping(item_map));
    }

    if let Value::Mapping(map) = &mut result {
        map.insert(
            Value::String("_DECODED_ITEMS".into()),
            Value::Mapping(decoded_map),
        );
    }

    result
}

pub fn extract_and_encode_serials_from_yaml(yaml: &Value) -> Result<Value> {
    let mut result = yaml.clone();
    let decoded_items_value = match &yaml {
        Value::Mapping(map) => map.get(&Value::String("_DECODED_ITEMS".into())).cloned(),
        _ => None,
    };

    let Some(Value::Mapping(decoded_items)) = decoded_items_value else {
        return Ok(result);
    };

    for (path_value, item_value) in &decoded_items {
        let path = match path_value {
            Value::String(path) => path.clone(),
            _ => continue,
        };

        let Value::Mapping(item_map) = item_value else {
            continue;
        };

        let original_serial = item_map
            .get(&Value::String("original_serial".into()))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("missing original_serial for path {path}"))?;

        let item_type = item_map
            .get(&Value::String("item_type".into()))
            .and_then(|v| v.as_str())
            .unwrap_or("?");

        let category = item_map
            .get(&Value::String("category".into()))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let confidence = item_map
            .get(&Value::String("confidence".into()))
            .and_then(|v| v.as_str())
            .unwrap_or("low");

        // Collect desired overrides from stats (and optional token stream) only.
        let mut desired_stats = ItemStats::default();
        let mut any_override = false;
        if let Some(Value::Mapping(stats_map)) = item_map.get(&Value::String("stats".into())) {
            let get_u16 = |key: &str| stats_map
                .get(&Value::String(key.into()))
                .and_then(|v| v.as_u64())
                .map(|v| v as u16);
            let get_u8 = |key: &str| stats_map
                .get(&Value::String(key.into()))
                .and_then(|v| v.as_u64())
                .map(|v| v as u8);

            desired_stats.primary_stat = get_u16("primary_stat");
            desired_stats.secondary_stat = get_u16("secondary_stat");
            desired_stats.level = get_u16("level");
            desired_stats.rarity = get_u8("rarity");
            desired_stats.manufacturer = get_u8("manufacturer");
            desired_stats.item_class = get_u8("item_class");

            any_override |= desired_stats.primary_stat.is_some()
                || desired_stats.secondary_stat.is_some()
                || desired_stats.level.is_some()
                || desired_stats.rarity.is_some()
                || desired_stats.manufacturer.is_some()
                || desired_stats.item_class.is_some();
        }

        // Determine if overrides actually differ from the baseline derived from original_serial.
        let baseline = decode_item_serial(original_serial);
        let mut differs = false;
        let diff_field = |a: Option<u16>, b: Option<u16>| -> bool { a.is_some() && a != b };
        let diff_field8 = |a: Option<u8>, b: Option<u8>| -> bool { a.is_some() && a != b };
        differs |= diff_field(desired_stats.primary_stat, baseline.stats.primary_stat);
        differs |= diff_field(desired_stats.secondary_stat, baseline.stats.secondary_stat);
        differs |= diff_field(desired_stats.level, baseline.stats.level);
        differs |= diff_field8(desired_stats.rarity, baseline.stats.rarity);
        differs |= diff_field8(desired_stats.manufacturer, baseline.stats.manufacturer);
        differs |= diff_field8(desired_stats.item_class, baseline.stats.item_class);

        // Only re-encode if there are overrides and they differ. Otherwise leave the serial intact.
        if any_override && differs {
            let decoded_item = DecodedItem {
                serial: original_serial.to_string(),
                item_type: item_type.to_string(),
                item_category: category.to_string(),
                length: 0,
                stats: desired_stats,
                confidence: confidence.to_string(),
                token_stream: Vec::new(),
                tokens: None,
                bit_string: None,
                parts: Vec::new(),
            };

            let new_serial = encode_item_serial(&decoded_item);
            set_nested_value(&mut result, &path, Value::String(new_serial))
                .with_context(|| format!("unable to update path {path}"))?;
        }
    }

    if let Value::Mapping(map) = &mut result {
        map.remove(&Value::String("_DECODED_ITEMS".into()));
    }

    Ok(result)
}

fn set_nested_value(root: &mut Value, path: &str, new_value: Value) -> Result<()> {
    let segments = parse_path(path)?;
    let mut current = root;

    for segment in &segments[..segments.len() - 1] {
        match segment {
            PathSegment::Key(key) => {
                let mapping = mapping_mut(current)
                    .ok_or_else(|| anyhow!("expected mapping for key {key}"))?;
                current = mapping
                    .get_mut(&Value::String(key.clone()))
                    .ok_or_else(|| anyhow!("key {key} not found"))?;
            }
            PathSegment::Index(index) => {
                let seq = sequence_mut(current)
                    .ok_or_else(|| anyhow!("expected sequence for index {index}"))?;
                current = seq
                    .get_mut(*index)
                    .ok_or_else(|| anyhow!("index {index} out of bounds"))?;
            }
        }
    }

    match segments.last().ok_or_else(|| anyhow!("empty path"))? {
        PathSegment::Key(key) => {
            let mapping =
                mapping_mut(current).ok_or_else(|| anyhow!("expected mapping to set key {key}"))?;
            mapping.insert(Value::String(key.clone()), new_value);
        }
        PathSegment::Index(index) => {
            let seq = sequence_mut(current)
                .ok_or_else(|| anyhow!("expected sequence to set index {index}"))?;
            if let Some(slot) = seq.get_mut(*index) {
                *slot = new_value;
            } else {
                bail!("index {index} out of bounds");
            }
        }
    }

    Ok(())
}

#[derive(Debug)]
enum PathSegment {
    Key(String),
    Index(usize),
}

fn parse_path(path: &str) -> Result<Vec<PathSegment>> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut chars = path.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '.' => {
                if !current.is_empty() {
                    segments.push(PathSegment::Key(std::mem::take(&mut current)));
                }
            }
            '[' => {
                if !current.is_empty() {
                    segments.push(PathSegment::Key(std::mem::take(&mut current)));
                }
                let mut index_str = String::new();
                while let Some(&next_ch) = chars.peek() {
                    if next_ch == ']' {
                        chars.next();
                        break;
                    }
                    index_str.push(next_ch);
                    chars.next();
                }
                if index_str.is_empty() {
                    bail!("empty index in path {path}");
                }
                let index = index_str.parse::<usize>()?;
                segments.push(PathSegment::Index(index));
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        segments.push(PathSegment::Key(current));
    }

    if segments.is_empty() {
        bail!("path {path} produced no segments");
    }

    Ok(segments)
}

fn mapping_mut(value: &mut Value) -> Option<&mut Mapping> {
    match value {
        Value::Mapping(map) => Some(map),
        Value::Tagged(tagged) => mapping_mut(&mut tagged.value),
        _ => None,
    }
}

fn sequence_mut(value: &mut Value) -> Option<&mut Vec<Value>> {
    match value {
        Value::Sequence(seq) => Some(seq),
        Value::Tagged(tagged) => sequence_mut(&mut tagged.value),
        _ => None,
    }
}

fn parse_actor_part_list(value: &str) -> BTreeMap<String, String> {
    let mut parts = BTreeMap::new();
    for token in value.split(',') {
        let trimmed = token.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("gap") {
            continue;
        }
        if let Some(start) = trimmed.find('[') {
            let key = trimmed[..start].trim().to_string();
            let remainder = &trimmed[start + 1..];
            if let Some(end) = remainder.rfind(']') {
                let val = remainder[..end].trim().to_string();
                parts.insert(key, val);
            } else {
                parts.insert(key, remainder.trim().to_string());
            }
        } else {
            parts.insert(trimmed.to_string(), String::new());
        }
    }
    parts
}

fn format_actor_part_list(parts: &BTreeMap<String, String>) -> String {
    if parts.is_empty() {
        return String::new();
    }
    parts
        .iter()
        .map(|(key, value)| {
            if value.trim().is_empty() {
                key.clone()
            } else {
                format!("{key}[{value}]")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn find_part_value(parts: &BTreeMap<String, String>, patterns: &[&str]) -> Option<String> {
    if patterns.is_empty() {
        return None;
    }
    let lowered: Vec<String> = patterns.iter().map(|p| p.to_ascii_lowercase()).collect();
    parts.iter().find_map(|(key, value)| {
        let key_lower = key.to_ascii_lowercase();
        let value_lower = value.to_ascii_lowercase();
        if lowered
            .iter()
            .all(|pattern| key_lower.contains(pattern) || value_lower.contains(pattern))
        {
            Some(value.clone())
        } else {
            None
        }
    })
}

fn mission_status_is_active(status: &str) -> bool {
    let lowered = status.to_ascii_lowercase();
    if lowered.is_empty()
        || lowered == "none"
        || lowered.contains("complete")
        || lowered.contains("finished")
        || lowered.contains("deactivated")
        || lowered.contains("inactive")
    {
        return false;
    }

    lowered.contains("active")
        || lowered.contains("inprogress")
        || lowered.contains("started")
        || lowered.contains("running")
        || lowered.contains("pending")
}

fn value_key_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(num) => num.as_i64().map(|n| n.to_string()),
        _ => None,
    }
}

fn get_mapping<'a>(map: &'a Mapping, key: &str) -> Option<&'a Mapping> {
    map.get(&Value::String(key.to_string()))
        .and_then(Value::as_mapping)
}

fn get_sequence<'a>(map: &'a Mapping, key: &str) -> Option<&'a Vec<Value>> {
    map.get(&Value::String(key.to_string()))
        .and_then(Value::as_sequence)
}

fn get_string(map: &Mapping, key: &str) -> Option<String> {
    map.get(&Value::String(key.to_string()))
        .and_then(|v| match v {
            Value::String(s) => Some(s.clone()),
            Value::Number(num) => num.as_i64().map(|n| n.to_string()),
            _ => None,
        })
}

fn get_i64(map: &Mapping, key: &str) -> Option<i64> {
    map.get(&Value::String(key.to_string()))
        .and_then(|value| match value {
            Value::Number(number) => number.as_i64(),
            Value::String(text) => text.trim().parse::<i64>().ok(),
            _ => None,
        })
}

fn get_i32(map: &Mapping, key: &str) -> Option<i32> {
    get_i64(map, key).and_then(|n| n.try_into().ok())
}

fn set_optional_string(map: &mut Mapping, key: &str, value: &Option<String>) {
    let key_value = Value::String(key.to_string());
    if let Some(val) = value {
        map.insert(key_value, Value::String(val.clone()));
    } else {
        map.remove(&key_value);
    }
}

fn set_optional_i32(map: &mut Mapping, key: &str, value: &Option<i32>) {
    let key_value = Value::String(key.to_string());
    if let Some(val) = value {
        map.insert(key_value, Value::Number(Number::from(*val)));
    } else {
        map.remove(&key_value);
    }
}

fn update_part_value(
    parts: &mut BTreeMap<String, String>,
    patterns: &[&str],
    new_value: Option<&str>,
) {
    if patterns.is_empty() {
        return;
    }
    let lowered: Vec<String> = patterns.iter().map(|p| p.to_ascii_lowercase()).collect();
    let mut matched_key: Option<String> = None;
    for (key, value) in parts.iter() {
        let key_lower = key.to_ascii_lowercase();
        let value_lower = value.to_ascii_lowercase();
        if lowered
            .iter()
            .all(|pattern| key_lower.contains(pattern) || value_lower.contains(pattern))
        {
            matched_key = Some(key.clone());
            break;
        }
    }

    match (matched_key, new_value) {
        (Some(key), Some(val)) => {
            if let Some(entry) = parts.get_mut(&key) {
                *entry = val.to_string();
            }
        }
        (Some(key), None) => {
            parts.remove(&key);
        }
        (None, Some(val)) => {
            let synthetic = patterns
                .first()
                .map(|s| s.replace(' ', "_"))
                .unwrap_or_else(|| "cosmetics_entry".to_string());
            parts.insert(synthetic, val.to_string());
        }
        (None, None) => {}
    }
}

fn update_actor_parts_section<F>(actor_parts_map: &mut Mapping, key: &str, mut update: F)
where
    F: FnMut(&mut BTreeMap<String, String>),
{
    let key_value = Value::String(key.to_string());
    let existing = actor_parts_map
        .get(&key_value)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let mut parts = parse_actor_part_list(&existing);
    let baseline = parts.clone();
    update(&mut parts);
    if parts != baseline {
        let formatted = format_actor_part_list(&parts);
        actor_parts_map.insert(key_value, Value::String(formatted));
    }
}

fn apply_cosmetics(
    state_map: &mut Mapping,
    cosmetics: &Bl4Cosmetics,
    vehicle_loadout: &Bl4VehicleLoadout,
) {
    let actor_parts_value = state_map
        .entry(Value::String("gbxactorparts".into()))
        .or_insert_with(|| Value::Mapping(Mapping::new()));

    let Some(actor_parts_map) = mapping_mut(actor_parts_value) else {
        return;
    };

    update_actor_parts_section(actor_parts_map, "character", |parts| {
        update_part_value(parts, &["cosmetics_", "_body"], cosmetics.body.as_deref());
        update_part_value(parts, &["cosmetics_", "_head"], cosmetics.head.as_deref());
        update_part_value(parts, &["cosmetics_", "_skin"], cosmetics.skin.as_deref());
        update_part_value(
            parts,
            &["cosmetics_colorization_primary"],
            cosmetics.primary_color.as_deref(),
        );
        update_part_value(
            parts,
            &["cosmetics_colorization_secondary"],
            cosmetics.secondary_color.as_deref(),
        );
        update_part_value(
            parts,
            &["cosmetics_colorization_tertiary"],
            cosmetics.tertiary_color.as_deref(),
        );
    });

    update_actor_parts_section(actor_parts_map, "echo4", |parts| {
        update_part_value(
            parts,
            &["cosmetics_echo4_body"],
            cosmetics.echo_body.as_deref(),
        );
        update_part_value(
            parts,
            &["cosmetics_echo4_attachment"],
            cosmetics.echo_attachment.as_deref(),
        );
        update_part_value(
            parts,
            &["cosmetics_echo4_skin"],
            cosmetics.echo_skin.as_deref(),
        );
    });

    update_actor_parts_section(actor_parts_map, "vehicle", |parts| {
        update_part_value(
            parts,
            &["cosmetics_vehicle", "vehicle_mat"],
            cosmetics
                .vehicle_skin
                .as_deref()
                .or(vehicle_loadout.vehicle_cosmetic.as_deref()),
        );
    });
}

fn apply_missions(
    state_map: &mut Mapping,
    tracked_missions: &[String],
    missions: &[Bl4MissionStatus],
    append_none: bool,
) {
    let missions_value = state_map
        .entry(Value::String("missions".into()))
        .or_insert_with(|| Value::Mapping(Mapping::new()));

    let Some(missions_map) = mapping_mut(missions_value) else {
        return;
    };

    let mut tracked_values = tracked_missions
        .iter()
        .filter(|name| !name.is_empty())
        .cloned()
        .map(Value::String)
        .collect::<Vec<_>>();
    if append_none {
        tracked_values.push(Value::String("none".into()));
    }
    missions_map.insert(
        Value::String("tracked_missions".into()),
        Value::Sequence(tracked_values),
    );

    let mut status_lookup: BTreeMap<(String, String), Option<String>> = BTreeMap::new();
    for mission in missions {
        status_lookup.insert(
            (mission.set.clone(), mission.mission.clone()),
            mission.status.clone(),
        );
    }

    let local_sets_value = missions_map
        .entry(Value::String("local_sets".into()))
        .or_insert_with(|| Value::Mapping(Mapping::new()));
    let Some(local_sets_map) = mapping_mut(local_sets_value) else {
        return;
    };

    for (set_key, set_value) in local_sets_map.iter_mut() {
        let Some(set_name) = value_key_to_string(set_key) else {
            continue;
        };
        let Some(set_map) = mapping_mut(set_value) else {
            continue;
        };
        let Some(inner_missions_value) = set_map.get_mut(&Value::String("missions".into())) else {
            continue;
        };
        let Some(inner_missions_map) = mapping_mut(inner_missions_value) else {
            continue;
        };
        for (mission_key, mission_value) in inner_missions_map.iter_mut() {
            let Some(mission_name) = value_key_to_string(mission_key) else {
                continue;
            };
            let Some(status_opt) = status_lookup.get(&(set_name.clone(), mission_name.clone()))
            else {
                continue;
            };
            let Some(mission_entry_map) = mapping_mut(mission_value) else {
                continue;
            };
            match status_opt {
                Some(status) => {
                    mission_entry_map.insert(
                        Value::String("status".into()),
                        Value::String(status.clone()),
                    );
                }
                None => {
                    mission_entry_map.remove(&Value::String("status".into()));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        aes_ecb_decrypt, aes_ecb_encrypt, apply_edit_state, collect_part_options, default_sdu_cost,
        derive_key, summarize_from_value, Bl4EditState, Bl4InventoryItem, Bl4InventoryItemModel,
        Bl4ItemModel,
    };
    use serde_yaml::Value;
    use std::path::Path;

    #[test]
    fn key_derivation_matches_reference() {
        let key = derive_key("76561199131094380").expect("key derivation");
        assert_eq!(
            key.iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(""),
            "5981fa32f25da0ebbe6b83115403ebfb2725642ed54906290578bd60ba4aa787"
        );
    }

    #[test]
    fn aes_ecb_vectors() {
        let mut block = hex_literal::hex!(
            "6bc1bee22e409f96e93d7e117393172a
             ae2d8a571e03ac9c9eb76fac45af8e51"
        )
        .to_vec();
        let key = hex_literal::hex!(
            "603deb1015ca71be2b73aef0857d7781
             1f352c073b6108d72d9810a30914dff4"
        );
        let expected_cipher = hex_literal::hex!(
            "f3eed1bdb5d2a03c064b5a7e3db181f8
             591ccb10d410ed26dc5ba74a31362870"
        );

        aes_ecb_encrypt(&mut block, &key);
        assert_eq!(block, expected_cipher);

        aes_ecb_decrypt(&mut block, &key);
        assert_eq!(
            block,
            hex_literal::hex!(
                "6bc1bee22e409f96e93d7e117393172a
                 ae2d8a571e03ac9c9eb76fac45af8e51"
            )
            .to_vec()
        );
    }

    const SAMPLE_SERIAL: &str = "@Ugy3L+2}TYg%$yC%i7M2gZldO)@}cgb!l34$a-qf{00";

    #[test]
    fn bl4_item_model_round_trip_preserves_serial() {
        let model = Bl4ItemModel::decode(SAMPLE_SERIAL).expect("decode sample serial");
        let encoded = model.to_serial().expect("encode serial");
        assert_eq!(encoded, SAMPLE_SERIAL);
    }

    #[test]
    fn bl4_item_model_level_update_persists() {
        let mut model = Bl4ItemModel::decode(SAMPLE_SERIAL).expect("decode sample serial");
        let original_level = model.level.unwrap_or(1);
        let new_level = original_level + 1;

        model
            .set_level(new_level)
            .expect("set level should succeed");

        let encoded = model.to_serial().expect("encode serial");
        let updated = Bl4ItemModel::decode(&encoded).expect("re-decode serial");
        assert_eq!(updated.level, Some(new_level));
    }

    #[test]
    fn collect_part_options_returns_catalog() {
        let model = Bl4ItemModel::decode(SAMPLE_SERIAL).expect("decode sample serial");
        let manufacturer = model.manufacturer.as_deref().unwrap_or_default();
        let item_type = model.item_type.as_deref().unwrap_or_default();

        let catalog = collect_part_options(manufacturer, item_type);

        assert!(
            !catalog.is_empty(),
            "expected non-empty catalog for {}:{}",
            manufacturer,
            item_type
        );

        let total_entries: usize = catalog.values().map(|entries| entries.len()).sum();
        assert!(total_entries > 0, "catalog should contain part entries");
    }

    #[test]
    fn sdu_level_updates_persist() {
        let yaml = r#"
state:
  progression:
    graphs:
      - name: sdu_upgrades
        group_def_name: Oak2_GlobalProgressGraph_Group
        nodes:
          - name: Ammo_Pistol_01
            points_spent: 0
          - name: Ammo_Pistol_02
            points_spent: 0
          - name: Ammo_SMG_01
            points_spent: 0
          - name: Backpack_01
            points_spent: 0
          - name: Bank_01
            points_spent: 0
          - name: Lost_Loot_01
            points_spent: 0
  ammo: {}
  unique_rewards: []
  currencies: {}
"#;

        let mut value: Value = serde_yaml::from_str(yaml).expect("parse yaml");

        let mut edits = Bl4EditState::default();
        edits.sdu_levels.backpack = 8;
        edits.sdu_levels.pistol = 7;
        edits.sdu_levels.smg = 6;
        edits.sdu_levels.bank = 5;
        edits.sdu_levels.lost_loot = 4;

        apply_edit_state(&mut value, &edits).expect("apply edits");

        let summary = summarize_from_value(Path::new("test.sav"), &value);
        assert_eq!(summary.sdu_levels.backpack, 8);
        assert_eq!(summary.sdu_levels.pistol, 7);
        assert_eq!(summary.sdu_levels.smg, 6);
        assert_eq!(summary.sdu_levels.bank, 5);
        assert_eq!(summary.sdu_levels.lost_loot, 4);

        let graphs = value
            .as_mapping()
            .and_then(|map| map.get(&Value::String("state".into())))
            .and_then(Value::as_mapping)
            .and_then(|map| map.get(&Value::String("progression".into())))
            .and_then(Value::as_mapping)
            .and_then(|map| map.get(&Value::String("graphs".into())))
            .and_then(Value::as_sequence)
            .expect("graphs");

        let sdu_graph = graphs
            .iter()
            .find(|graph| {
                graph
                    .as_mapping()
                    .and_then(|map| map.get(&Value::String("name".into())))
                    .and_then(Value::as_str)
                    == Some("sdu_upgrades")
            })
            .expect("sdu graph");

        let nodes = sdu_graph
            .as_mapping()
            .and_then(|map| map.get(&Value::String("nodes".into())))
            .and_then(Value::as_sequence)
            .expect("nodes");

        let find_points = |name: &str| -> i64 {
            nodes
                .iter()
                .find(|node| {
                    node.as_mapping()
                        .and_then(|map| map.get(&Value::String("name".into())))
                        .and_then(Value::as_str)
                        == Some(name)
                })
                .and_then(|node| {
                    node.as_mapping()
                        .and_then(|map| map.get(&Value::String("points_spent".into())))
                        .and_then(Value::as_i64)
                })
                .unwrap_or_default()
        };

        assert_eq!(find_points("Backpack_08"), default_sdu_cost(8));
        assert_eq!(find_points("Ammo_Pistol_07"), default_sdu_cost(7));
        assert_eq!(find_points("Ammo_SMG_06"), default_sdu_cost(6));
        assert_eq!(find_points("Bank_05"), default_sdu_cost(5));
        assert_eq!(find_points("Lost_Loot_04"), default_sdu_cost(4));
    }

    #[test]
    fn sdu_summary_uses_activation_level_when_costs_are_zero() {
        let yaml = r#"
state:
  progression:
    graphs:
      - name: sdu_upgrades
        nodes:
          - name: Ammo_Pistol_03
            activation_level: 3
          - name: Ammo_SMG_02
            activation_level: 2
          - name: Backpack_02
            activation_level: 2
          - name: Lost_Loot_01
            activation_level: 1
  ammo: {}
  unique_rewards: []
  currencies: {}
"#;

        let value: Value = serde_yaml::from_str(yaml).expect("parse yaml");
        let summary = summarize_from_value(Path::new("activation_level.sav"), &value);
        assert_eq!(summary.sdu_levels.pistol, 3);
        assert_eq!(summary.sdu_levels.smg, 2);
        assert_eq!(summary.sdu_levels.backpack, 2);
        assert_eq!(summary.sdu_levels.lost_loot, 1);
    }

    #[test]
    fn sdu_points_accept_string_numbers() {
        let yaml = r#"
state:
  progression:
    graphs:
      - name: sdu_upgrades
        nodes:
          - name: Ammo_Pistol_01
            points_spent: "5"
          - name: Ammo_SMG_02
            points_spent: "10"
          - name: Backpack_03
            points_spent: "20"
          - name: Bank_04
            points_spent: "30"
          - name: Lost_Loot_05
            points_spent: "50"
  ammo: {}
  unique_rewards: []
  currencies: {}
"#;

        let value: Value = serde_yaml::from_str(yaml).expect("parse yaml");
        let summary = summarize_from_value(Path::new("string_points.sav"), &value);
        assert_eq!(summary.sdu_levels.pistol, 1);
        assert_eq!(summary.sdu_levels.smg, 2);
        assert_eq!(summary.sdu_levels.backpack, 3);
        assert_eq!(summary.sdu_levels.bank, 4);
        assert_eq!(summary.sdu_levels.lost_loot, 5);
    }

    #[test]
    fn sdu_summary_reads_root_level_progression() {
        let yaml = r#"
progression:
  graphs:
    - name: sdu_upgrades
      nodes:
        - name: Backpack_02
          points_spent: 10
        - name: Ammo_Pistol_03
          points_spent: 20
state:
  inventory: {}
  currencies: {}
  ammo: {}
"#;

        let value: Value = serde_yaml::from_str(yaml).expect("parse yaml");
        let summary = summarize_from_value(Path::new("root_progression.sav"), &value);
        assert_eq!(summary.sdu_levels.backpack, 2);
        assert_eq!(summary.sdu_levels.pistol, 3);
    }

    #[test]
    fn apply_edit_state_syncs_root_progression() {
        let yaml = r#"
progression:
  graphs:
    - name: sdu_upgrades
      nodes:
        - name: Ammo_Pistol_01
          points_spent: 0
        - name: Ammo_Pistol_02
          points_spent: 0
state:
  inventory: {}
  currencies: {}
  ammo: {}
"#;

        let mut value: Value = serde_yaml::from_str(yaml).expect("parse yaml");
        let mut edits = Bl4EditState::default();
        edits.sdu_levels.pistol = 2;

        apply_edit_state(&mut value, &edits).expect("apply edits");

        let root = value.as_mapping().expect("root map");
        let progression = root
            .get(&Value::String("progression".into()))
            .and_then(Value::as_mapping)
            .expect("root progression");
        let graphs = progression
            .get(&Value::String("graphs".into()))
            .and_then(Value::as_sequence)
            .expect("graphs seq");
        let sdu_graph = graphs
            .iter()
            .find(|graph| {
                graph
                    .as_mapping()
                    .and_then(|map| map.get(&Value::String("name".into())))
                    .and_then(Value::as_str)
                    == Some("sdu_upgrades")
            })
            .expect("sdu graph");
        let nodes = sdu_graph
            .as_mapping()
            .and_then(|map| map.get(&Value::String("nodes".into())))
            .and_then(Value::as_sequence)
            .expect("nodes");
        let pistol_two = nodes
            .iter()
            .find(|node| {
                node.as_mapping()
                    .and_then(|map| map.get(&Value::String("name".into())))
                    .and_then(Value::as_str)
                    == Some("Ammo_Pistol_02")
            })
            .and_then(|node| {
                node.as_mapping().and_then(|map| {
                    map.get(&Value::String("points_spent".into()))
                        .and_then(Value::as_i64)
                })
            })
            .expect("pistol level 2");
        assert_eq!(pistol_two, default_sdu_cost(2));

        let summary = summarize_from_value(Path::new("updated.sav"), &value);
        assert_eq!(summary.sdu_levels.pistol, 2);
    }

    #[test]
    fn sdu_nodes_shape_and_costs_match_working_format() {
        // Start with a minimal YAML without an SDU graph; edits should create it.
        let yaml = r#"
progression:
  graphs: []
state:
  inventory: {}
  currencies: {}
  ammo: {}
"#;

        let mut value: Value = serde_yaml::from_str(yaml).expect("parse yaml");
        let mut edits = Bl4EditState::default();
        // BL4: ammo tiers up to 7, backpack up to 8
        edits.sdu_levels.pistol = 3; // expect Ammo_Pistol_01..03
        edits.sdu_levels.backpack = 8; // expect Backpack_01..08

        apply_edit_state(&mut value, &edits).expect("apply edits");

        let root = value.as_mapping().expect("root map");
        let progression = root
            .get(&Value::String("progression".into()))
            .and_then(Value::as_mapping)
            .expect("root progression");
        let graphs = progression
            .get(&Value::String("graphs".into()))
            .and_then(Value::as_sequence)
            .expect("graphs seq");
        let sdu_graph = graphs
            .iter()
            .find(|graph| {
                graph
                    .as_mapping()
                    .and_then(|map| map.get(&Value::String("name".into())))
                    .and_then(Value::as_str)
                    == Some("sdu_upgrades")
            })
            .expect("sdu graph");
        let nodes = sdu_graph
            .as_mapping()
            .and_then(|map| map.get(&Value::String("nodes".into())))
            .and_then(Value::as_sequence)
            .expect("nodes");

        let mut seen: Vec<(String, i64)> = nodes
            .iter()
            .filter_map(|node| {
                let map = node.as_mapping()?;
                let name = map
                    .get(&Value::String("name".into()))
                    .and_then(Value::as_str)?
                    .to_string();
                let cost = map
                    .get(&Value::String("points_spent".into()))
                    .and_then(Value::as_i64)?;
                // Ensure no activation keys present
                assert!(
                    !map.contains_key(&Value::String("is_activated".into()))
                        && !map.contains_key(&Value::String("activation_level".into())),
                    "SDU node should not contain activation fields"
                );
                Some((name, cost))
            })
            .collect();

        // Helper to find cost by name
        let cost_of = |n: &str| -> Option<i64> {
            seen.iter().find(|(k, _)| k == n).map(|(_, v)| *v)
        };

        // Expected pistol nodes and costs
        assert_eq!(cost_of("Ammo_Pistol_01"), Some(default_sdu_cost(1)));
        assert_eq!(cost_of("Ammo_Pistol_02"), Some(default_sdu_cost(2)));
        assert_eq!(cost_of("Ammo_Pistol_03"), Some(default_sdu_cost(3)));
        assert!(cost_of("Ammo_Pistol_04").is_none());

        // Expected backpack full 8 tiers
        for idx in 1..=8 {
            let name = format!("Backpack_{:02}", idx);
            assert_eq!(cost_of(&name), Some(default_sdu_cost(idx)));
        }

        // No unsupported BL4 families are emitted
        for prefix in ["Ammo_GL_", "Ammo_Grenade_", "Ammo_RL_", "Ammo_HW_", "Ammo_Heavy_"] {
            assert!(
                !seen.iter().any(|(name, _)| name.starts_with(prefix)),
                "unexpected family emitted: {}",
                prefix
            );
        }
    }

    #[test]
    fn inventory_item_model_round_trip() {
        let base = Bl4InventoryItem {
            slot: "slot_0".into(),
            serial: SAMPLE_SERIAL.into(),
            state_flags: Some(0),
        };

        let mut model = Bl4InventoryItemModel::decode(&base).expect("decode inventory model");
        assert_eq!(model.slot, "slot_0");

        model.set_state_flag(0x2, true);
        assert!(model.has_state_flag(0x2));

        model.item.set_level(60).expect("adjust level");

        let updated = model.to_inventory_item().expect("serialize inventory item");
        assert_eq!(updated.slot, "slot_0");
        assert!(updated.state_flags.unwrap_or(0) & 0x2 != 0);

        let model_again = Bl4InventoryItemModel::decode(&updated).expect("decode again");
        assert_eq!(model_again.item.level, Some(60));
    }
}
