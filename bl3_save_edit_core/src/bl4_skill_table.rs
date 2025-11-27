use std::{collections::HashMap, fs, path::PathBuf};

use once_cell::sync::Lazy;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct RawSkillEntry {
    name: String,
    #[serde(rename = "type")]
    skill_type: String,
    #[serde(rename = "maxRank")]
    max_rank: i32,
}

#[derive(Debug, Deserialize)]
struct RawSkillTree {
    name: String,
    #[serde(rename = "actionSkill")]
    action_skill: Option<String>,
    #[serde(default)]
    skills: Vec<RawSkillEntry>,
}

#[derive(Debug, Deserialize)]
struct RawVaultHunter {
    name: String,
    class: String,
    #[serde(rename = "skillTrees")]
    skill_trees: Vec<RawSkillTree>,
}

#[derive(Debug, Deserialize)]
struct RawMetadataFile {
    #[serde(rename = "VaultHunters")]
    vault_hunters: Vec<RawVaultHunter>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillType {
    Passive,
    KillSkill,
    ActionSkill,
    Augment,
    Capstone,
    Other,
}

impl SkillType {
    fn from_str(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "passive" => SkillType::Passive,
            "kill skill" | "kill_skill" => SkillType::KillSkill,
            "action skill" | "action_skill" => SkillType::ActionSkill,
            "augment" => SkillType::Augment,
            "capstone" => SkillType::Capstone,
            _ => SkillType::Other,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SkillEntry {
    pub name: String,
    pub skill_type: SkillType,
    pub max_rank: i32,
}

#[derive(Debug, Clone)]
pub struct SkillTreeEntry {
    pub name: String,
    pub action_skill: Option<String>,
    pub skills: Vec<SkillEntry>,
    lookup: HashMap<String, usize>,
}

impl SkillTreeEntry {
    pub fn find_skill(&self, name: &str) -> Option<&SkillEntry> {
        self.lookup
            .get(&name.to_ascii_lowercase())
            .and_then(|idx| self.skills.get(*idx))
    }
}

#[derive(Debug, Clone)]
pub struct VaultHunterEntry {
    pub name: String,
    pub class: String,
    pub trees: Vec<SkillTreeEntry>,
    tree_lookup: HashMap<String, usize>,
}

impl VaultHunterEntry {
    pub fn tree_by_name(&self, name: &str) -> Option<&SkillTreeEntry> {
        self.tree_lookup
            .get(&name.to_ascii_lowercase())
            .and_then(|idx| self.trees.get(*idx))
    }

    pub fn tree_names(&self) -> Vec<String> {
        self.trees.iter().map(|t| t.name.clone()).collect()
    }
}

#[derive(Debug)]
pub struct SkillMetadataDatabase {
    hunters: HashMap<String, VaultHunterEntry>,
}

impl SkillMetadataDatabase {
    pub fn hunter_by_identifier(&self, identifier: &str) -> Option<&VaultHunterEntry> {
        let key = identifier.to_ascii_lowercase();
        if let Some(entry) = self.hunters.get(&key) {
            return Some(entry);
        }
        // try to strip prefixes like "char_"
        let trimmed = key.trim();
        if trimmed.starts_with("char_") {
            let alias = trimmed.trim_start_matches("char_");
            if let Some(entry) = self.hunters.get(alias) {
                return Some(entry);
            }
        }
        if let Some(entry) = self.hunters.get(trimmed) {
            return Some(entry);
        }
        // try to match by contains (paladin -> amon, siren -> vex, etc.)
        for (name, entry) in &self.hunters {
            if trimmed.contains(name) || name.contains(trimmed) {
                return Some(entry);
            }
            let class_key = entry.class.to_ascii_lowercase();
            if class_key.contains(trimmed) || trimmed.contains(&class_key) {
                return Some(entry);
            }
        }
        None
    }
}

pub static BL4_SKILL_METADATA: Lazy<SkillMetadataDatabase> = Lazy::new(|| {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("json")
        .join("all_vaulthunters_skills_mapping.json");
    let contents = fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!(
            "failed to read skill metadata file {}: {}",
            path.display(),
            err
        )
    });
    let parsed: RawMetadataFile = serde_json::from_str(&contents)
        .unwrap_or_else(|err| panic!("failed to parse {}: {}", path.display(), err));

    let mut hunters = HashMap::new();
    for hunter in parsed.vault_hunters {
        let mut trees = Vec::new();
        for tree in hunter.skill_trees {
            let mut skills = Vec::new();
            let mut lookup = HashMap::new();
            for (idx, skill) in tree.skills.into_iter().enumerate() {
                lookup.insert(skill.name.to_ascii_lowercase(), idx);
                skills.push(SkillEntry {
                    name: skill.name,
                    skill_type: SkillType::from_str(&skill.skill_type),
                    max_rank: skill.max_rank.max(0),
                });
            }
            trees.push(SkillTreeEntry {
                name: tree.name,
                action_skill: tree.action_skill,
                skills,
                lookup,
            });
        }
        let tree_lookup = trees
            .iter()
            .enumerate()
            .map(|(idx, tree)| (tree.name.to_ascii_lowercase(), idx))
            .collect();
        let entry = VaultHunterEntry {
            name: hunter.name.clone(),
            class: hunter.class.clone(),
            trees,
            tree_lookup,
        };
        let key_name = hunter.name.to_ascii_lowercase();
        let key_class = hunter.class.to_ascii_lowercase();
        hunters.insert(key_name, entry.clone());
        hunters.insert(key_class, entry.clone());
        for alias in extra_aliases(&entry.name) {
            hunters.insert(alias.to_ascii_lowercase(), entry.clone());
        }
    }

    SkillMetadataDatabase { hunters }
});

fn extra_aliases(name: &str) -> &'static [&'static str] {
    match name.to_ascii_lowercase().as_str() {
        "amon" => &["paladin"],
        "harlowe" => &["gravitar"],
        "rafa" => &["exosoldier"],
        "vex" => &["darksiren"],
        _ => &[],
    }
}
