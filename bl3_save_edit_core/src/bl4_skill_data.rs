use std::collections::HashMap;

use once_cell::sync::Lazy;
use serde::Deserialize;

static RAW_SKILL_TREE_YAML: &str = include_str!("../resources/bl4_skill_trees.yaml");

#[derive(Debug, Clone, Deserialize)]
struct SkillTreeFile {
    characters: Vec<Bl4CharacterSkillTrees>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Bl4CharacterSkillTrees {
    pub id: String,
    pub display_name: String,
    pub trees: Vec<Bl4SkillTreeDefinition>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Bl4SkillTreeDefinition {
    pub name: String,
    #[serde(default)]
    pub action_skills: Vec<String>,
    #[serde(default)]
    pub skills: Vec<Bl4SkillDefinition>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Bl4SkillDefinition {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub required_points: i32,
    pub max_points: i32,
    pub required_skill: Option<String>,
    #[serde(default)]
    pub stats: Vec<Bl4SkillStat>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Bl4SkillStat {
    pub stat: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct Bl4SkillTreeDatabase {
    characters: Vec<Bl4CharacterSkillTrees>,
    by_id: HashMap<String, usize>,
    by_display_name: HashMap<String, usize>,
}

impl Bl4SkillTreeDatabase {
    fn new(characters: Vec<Bl4CharacterSkillTrees>) -> Self {
        let mut by_id = HashMap::new();
        let mut by_display_name = HashMap::new();
        for (idx, character) in characters.iter().enumerate() {
            by_id.insert(character.id.to_ascii_lowercase(), idx);
            by_display_name.insert(character.display_name.to_ascii_lowercase(), idx);
        }
        Self {
            characters,
            by_id,
            by_display_name,
        }
    }

    pub fn all(&self) -> &[Bl4CharacterSkillTrees] {
        &self.characters
    }

    pub fn get_by_id(&self, id: &str) -> Option<&Bl4CharacterSkillTrees> {
        let key = id.to_ascii_lowercase();
        self.by_id.get(&key).and_then(|idx| self.characters.get(*idx))
    }

    pub fn get_by_display_name(&self, name: &str) -> Option<&Bl4CharacterSkillTrees> {
        let key = name.to_ascii_lowercase();
        self.by_display_name
            .get(&key)
            .and_then(|idx| self.characters.get(*idx))
    }
}

impl Bl4CharacterSkillTrees {
    pub fn find_skill(&self, name: &str) -> Option<&Bl4SkillDefinition> {
        let target = name.trim();
        for tree in &self.trees {
            for skill in &tree.skills {
                if skill.name.eq_ignore_ascii_case(target) {
                    return Some(skill);
                }
            }
        }
        None
    }

    pub fn all_skills(&self) -> impl Iterator<Item = &Bl4SkillDefinition> {
        self.trees.iter().flat_map(|tree| tree.skills.iter())
    }
}

pub static BL4_SKILL_TREES: Lazy<Bl4SkillTreeDatabase> = Lazy::new(|| {
    let root: SkillTreeFile = serde_yaml::from_str(RAW_SKILL_TREE_YAML)
        .expect("failed to parse BL4 skill tree metadata");
    Bl4SkillTreeDatabase::new(root.characters)
});
