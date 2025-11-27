use std::collections::{HashMap, HashSet};

use heck::TitleCase;
use iced::Color;
use iced::{
    button, pick_list, scrollable, text_input, tooltip, Alignment, Button, Checkbox, Column,
    Container, Element, Length, PickList, Row, Scrollable, Text, TextInput, Tooltip,
};

use bl3_save_edit_core::bl3_save::character_data::MAX_CHARACTER_LEVEL;
use bl3_save_edit_core::bl3_save::player_class::PlayerClass;
use bl3_save_edit_core::bl3_save::util::REQUIRED_XP_LIST;
use bl3_save_edit_core::{
    bl4::Bl4SaveSummary,
    bl4_skill_table::{SkillEntry, SkillTreeEntry, SkillType, VaultHunterEntry, BL4_SKILL_METADATA},
    game_data::GameDataKv,
};

use crate::bl3_ui::{Bl3Message, InteractionMessage};
use crate::bl3_ui_style::{Bl3UiStyle, Bl3UiTooltipStyle};
use crate::resources::fonts::{JETBRAINS_MONO, JETBRAINS_MONO_BOLD};
use crate::views::manage_save::character::ammo::AmmoSetter;
use crate::views::manage_save::character::gear::GearUnlocker;
use crate::views::manage_save::character::sdu::SduUnlocker;
use crate::views::manage_save::character::skins::SkinSelectors;
use crate::views::manage_save::{Bl4ViewMode, ManageSaveInteractionMessage};
use crate::views::InteractionExt;
use crate::widgets::labelled_element::LabelledElement;
use crate::widgets::number_input::NumberInput;
use crate::widgets::text_input_limited::TextInputLimited;

mod ammo;
mod gear;
mod sdu;
mod skins;

const BL4_CLASSES: &[&str] = &["Echo4", "DarkSiren", "ExoSoldier", "Gravitar", "Paladin"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharacterDetailTab {
    Overview,
    ActionSkills,
}

impl Default for CharacterDetailTab {
    fn default() -> Self {
        CharacterDetailTab::Overview
    }
}

#[derive(Debug, Clone, Default)]
pub struct Bl4SkillNodeState {
    pub name: String,
    pub points_spent: Option<i32>,
    pub activation_level: Option<i32>,
    pub is_activated: Option<bool>,
    pub points_input_state: text_input::State,
    pub activation_level_input_state: text_input::State,
}

#[derive(Debug, Clone, Default)]
pub struct Bl4SkillTreeState {
    pub name: String,
    pub group_def_name: Option<String>,
    pub nodes: Vec<Bl4SkillNodeState>,
}

#[derive(Debug, Default)]
pub struct CharacterState {
    pub detail_tab: CharacterDetailTab,
    pub detail_tab_overview_button_state: button::State,
    pub detail_tab_action_button_state: button::State,
    pub name_input: String,
    pub name_input_state: text_input::State,
    pub player_class_selector: pick_list::State<PlayerClass>,
    pub player_class_selected_class: PlayerClass,
    pub bl4_class_selector: pick_list::State<String>,
    pub bl4_class_selected: Option<String>,
    pub bl4_skill_tree_selector: pick_list::State<String>,
    pub bl4_skill_tree_selected: Option<String>,
    pub bl4_action_tree_selector: pick_list::State<String>,
    pub bl4_action_tree_selected: Option<String>,
    pub level_input: i32,
    pub xp_level_input_state: text_input::State,
    pub experience_points_input: i32,
    pub experience_points_input_state: text_input::State,
    pub ability_points_input: i32,
    pub ability_points_input_state: text_input::State,
    pub skin_selectors: SkinSelectors,
    pub gear_unlocker: GearUnlocker,
    pub ammo_setter: AmmoSetter,
    pub sdu_unlocker: SduUnlocker,
    pub bl4_body_input: String,
    pub bl4_body_input_state: text_input::State,
    pub bl4_head_input: String,
    pub bl4_head_input_state: text_input::State,
    pub bl4_skin_input: String,
    pub bl4_skin_input_state: text_input::State,
    pub bl4_primary_color_input: String,
    pub bl4_primary_color_input_state: text_input::State,
    pub bl4_secondary_color_input: String,
    pub bl4_secondary_color_input_state: text_input::State,
    pub bl4_tertiary_color_input: String,
    pub bl4_tertiary_color_input_state: text_input::State,
    pub bl4_echo_body_input: String,
    pub bl4_echo_body_input_state: text_input::State,
    pub bl4_echo_attachment_input: String,
    pub bl4_echo_attachment_input_state: text_input::State,
    pub bl4_echo_skin_input: String,
    pub bl4_echo_skin_input_state: text_input::State,
    pub bl4_vehicle_skin_input: String,
    pub bl4_vehicle_skin_input_state: text_input::State,
    pub bl4_unique_rewards_input: String,
    pub bl4_unique_rewards_input_state: text_input::State,
    pub bl4_equip_slots_input: String,
    pub bl4_equip_slots_input_state: text_input::State,
    pub bl4_progress_character_input: i32,
    pub bl4_progress_character_input_state: text_input::State,
    pub bl4_progress_specialization_input: i32,
    pub bl4_progress_specialization_input_state: text_input::State,
    pub bl4_progress_echo_input: i32,
    pub bl4_progress_echo_input_state: text_input::State,
    pub bl4_skill_trees: Vec<Bl4SkillTreeState>,
    pub bl4_skill_points_mass_input: i32,
    pub bl4_skill_points_mass_input_state: text_input::State,
    pub bl4_skill_apply_button_state: button::State,
    pub bl4_skill_max_button_state: button::State,
    pub bl4_skill_reset_button_state: button::State,
    pub bl4_clear_missions_button_state: button::State,
    pub bl4_clear_missions_pressed: bool,
    pub scroll: scrollable::State,
}

struct Bl4ActionSkillsContext<'a> {
    skill_trees: &'a mut Vec<Bl4SkillTreeState>,
    tree_selector_state: &'a mut pick_list::State<String>,
    selected_tree: &'a mut Option<String>,
    available_points: i32,
    mass_input_state: &'a mut text_input::State,
    mass_input_value: i32,
    apply_button_state: &'a mut button::State,
    max_button_state: &'a mut button::State,
    reset_button_state: &'a mut button::State,
    clear_missions_button_state: &'a mut button::State,
}

#[derive(Debug, Clone)]
pub enum SaveCharacterInteractionMessage {
    Name(String),
    Level(i32),
    ExperiencePoints(i32),
    AbilityPoints(i32),
    DetailTabChanged(CharacterDetailTab),
    PlayerClassSelected(PlayerClass),
    SkinMessage(CharacterSkinSelectedMessage),
    GearMessage(CharacterGearUnlockedMessage),
    SduMessage(CharacterSduMessage),
    AmmoMessage(CharacterAmmoMessage),
    MaxSduSlotsPressed,
    MaxAmmoAmountsPressed,
    Bl4ClassChanged(String),
    Bl4SkillTreeChanged(String),
    Bl4ActionSkillTreeChanged(String),
    Bl4CosmeticBody(String),
    Bl4CosmeticHead(String),
    Bl4CosmeticSkin(String),
    Bl4PrimaryColor(String),
    Bl4SecondaryColor(String),
    Bl4TertiaryColor(String),
    Bl4EchoBody(String),
    Bl4EchoAttachment(String),
    Bl4EchoSkin(String),
    Bl4VehicleSkin(String),
    Bl4EquipSlots(String),
    Bl4UniqueRewards(String),
    Bl4ProgressCharacter(i32),
    Bl4ProgressSpecialization(i32),
    Bl4ProgressEcho(i32),
    Bl4SkillMassInput(i32),
    Bl4SkillApplyAll,
    Bl4SkillMaxAll,
    Bl4SkillResetAll,
    Bl4ClearMissions,
    Bl4SkillNodePoints {
        tree_index: usize,
        node_index: usize,
        value: i32,
    },
    Bl4SkillNodeActivationLevel {
        tree_index: usize,
        node_index: usize,
        value: i32,
    },
    Bl4SkillNodeToggle {
        tree_index: usize,
        node_index: usize,
        value: bool,
    },
}

#[derive(Debug, Default)]
pub struct CharacterGearState {
    pub unlock_grenade_slot: bool,
    pub unlock_shield_slot: bool,
    pub unlock_weapon_1_slot: bool,
    pub unlock_weapon_2_slot: bool,
    pub unlock_weapon_3_slot: bool,
    pub unlock_weapon_4_slot: bool,
    pub unlock_artifact_slot: bool,
    pub unlock_class_mod_slot: bool,
}

#[derive(Debug, Clone)]
pub enum CharacterSkinSelectedMessage {
    HeadSkin(GameDataKv),
    CharacterSkin(GameDataKv),
    EchoTheme(GameDataKv),
}

#[derive(Debug, Clone)]
pub enum CharacterGearUnlockedMessage {
    Grenade(bool),
    Shield(bool),
    Weapon1(bool),
    Weapon2(bool),
    Weapon3(bool),
    Weapon4(bool),
    Artifact(bool),
    ClassMod(bool),
}

#[derive(Debug, Clone)]
pub enum CharacterSduMessage {
    Backpack(i32),
    Sniper(i32),
    Shotgun(i32),
    Pistol(i32),
    Grenade(i32),
    Smg(i32),
    AssaultRifle(i32),
    Heavy(i32),
}

#[derive(Debug, Clone)]
pub enum CharacterAmmoMessage {
    Sniper(i32),
    Shotgun(i32),
    Pistol(i32),
    Grenade(i32),
    Smg(i32),
    AssaultRifle(i32),
    Heavy(i32),
}

const LABEL_COLOR: Color = Color {
    r: 242.0 / 255.0,
    g: 203.0 / 255.0,
    b: 5.0 / 255.0,
    a: 1.0,
};

const VALUE_COLOR: Color = Color {
    r: 220.0 / 255.0,
    g: 220.0 / 255.0,
    b: 220.0 / 255.0,
    a: 1.0,
};

fn bl4_slot_name(slot: i32) -> &'static str {
    match slot {
        0 => "Weapon Slot 1",
        1 => "Weapon Slot 2",
        2 => "Weapon Slot 3",
        3 => "Weapon Slot 4",
        4 => "Grenade Slot",
        5 => "Shield Slot",
        6 => "Class Mod Slot",
        7 => "Artifact Slot",
        8 => "Enhancement Slot",
        _ => "Unknown Slot",
    }
}

fn build_bl4_cosmetics_overview(summary: &Bl4SaveSummary) -> Option<Container<Bl3Message>> {
    let cosmetics = &summary.cosmetics;
    let mut entries: Vec<(&str, &str)> = Vec::new();

    if let Some(value) = cosmetics.body.as_deref() {
        entries.push(("Body", value));
    }
    if let Some(value) = cosmetics.head.as_deref() {
        entries.push(("Head", value));
    }
    if let Some(value) = cosmetics.skin.as_deref() {
        entries.push(("Skin", value));
    }
    if let Some(value) = cosmetics.primary_color.as_deref() {
        entries.push(("Primary Color", value));
    }
    if let Some(value) = cosmetics.secondary_color.as_deref() {
        entries.push(("Secondary Color", value));
    }
    if let Some(value) = cosmetics.tertiary_color.as_deref() {
        entries.push(("Tertiary Color", value));
    }
    if let Some(value) = cosmetics.echo_body.as_deref() {
        entries.push(("ECHO Body", value));
    }
    if let Some(value) = cosmetics.echo_attachment.as_deref() {
        entries.push(("ECHO Attachment", value));
    }
    if let Some(value) = cosmetics.echo_skin.as_deref() {
        entries.push(("ECHO Skin", value));
    }
    if let Some(value) = cosmetics.vehicle_skin.as_deref() {
        entries.push(("Vehicle Cosmetic", value));
    }

    if entries.is_empty() {
        return None;
    }

    let mut column = Column::new().spacing(4).push(
        Text::new("Cosmetics")
            .font(JETBRAINS_MONO_BOLD)
            .size(20)
            .color(LABEL_COLOR),
    );

    for (label, value) in entries {
        column = column.push(
            Text::new(format!("{label}: {value}"))
                .font(JETBRAINS_MONO)
                .size(16)
                .color(VALUE_COLOR),
        );
    }

    Some(
        Container::new(column)
            .padding(10)
            .width(Length::Fill)
            .style(Bl3UiStyle),
    )
}

fn build_bl4_gear_section(summary: &Bl4SaveSummary) -> Container<Bl3Message> {
    let mut column = Column::new().spacing(6).push(
        Text::new("Gear & Rewards")
            .font(JETBRAINS_MONO_BOLD)
            .size(17)
            .color(LABEL_COLOR),
    );

    if summary.equip_slots_unlocked.is_empty() {
        column = column.push(
            Text::new("No slot unlock data detected.")
                .font(JETBRAINS_MONO)
                .size(15)
                .color(VALUE_COLOR),
        );
    } else {
        for slot in &summary.equip_slots_unlocked {
            column = column.push(
                Text::new(format!("{} (#{})", bl4_slot_name(*slot), slot))
                    .font(JETBRAINS_MONO)
                    .size(15)
                    .color(VALUE_COLOR),
            );
        }
    }

    if !summary.unique_rewards.is_empty() {
        column = column.push(
            Text::new("")
                .font(JETBRAINS_MONO)
                .size(12)
                .color(VALUE_COLOR),
        );

        column = column.push(
            Text::new("Notable Rewards")
                .font(JETBRAINS_MONO_BOLD)
                .size(16)
                .color(LABEL_COLOR),
        );

        for reward in summary.unique_rewards.iter().take(10) {
            column = column.push(
                Text::new(reward.clone())
                    .font(JETBRAINS_MONO)
                    .size(14)
                    .color(VALUE_COLOR),
            );
        }

        if summary.unique_rewards.len() > 10 {
            column = column.push(
                Text::new(format!(
                    "... plus {} more",
                    summary.unique_rewards.len() - 10
                ))
                .font(JETBRAINS_MONO)
                .size(13)
                .color(VALUE_COLOR),
            );
        }
    }

    Container::new(column)
        .padding(15)
        .width(Length::Fill)
        .style(Bl3UiStyle)
}

pub fn view<'a>(
    character_state: &'a mut CharacterState,
    bl4_summary: Option<&'a Bl4SaveSummary>,
    view_mode: Bl4ViewMode,
) -> Container<'a, Bl3Message> {
    if let (Some(summary), Bl4ViewMode::Summary) = (bl4_summary, view_mode) {
        let labelled_row = |label: &str, value: String| {
            Container::new(
                Row::new()
                    .push(
                        Text::new(label)
                            .font(JETBRAINS_MONO_BOLD)
                            .size(17)
                            .width(Length::Units(160))
                            .color(LABEL_COLOR),
                    )
                    .push(
                        Text::new(value)
                            .font(JETBRAINS_MONO)
                            .size(17)
                            .width(Length::Fill)
                            .color(VALUE_COLOR),
                    )
                    .spacing(15)
                    .align_items(Alignment::Center),
            )
            .padding(10)
            .width(Length::Fill)
            .style(Bl3UiStyle)
        };

        let char_name = summary
            .char_name
            .as_ref()
            .cloned()
            .unwrap_or_else(|| "Unknown".to_string());

        let class = summary
            .class
            .as_ref()
            .cloned()
            .unwrap_or_else(|| "Unknown".to_string());

        let level = summary
            .character_level
            .map(|v| v.to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        let experience = summary
            .character_experience
            .map(|v| v.to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        let spec_level = summary
            .specialization_level
            .map(|v| v.to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        let spec_points = summary
            .specialization_points
            .map(|v| v.to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        let mut info = Column::new()
            .spacing(15)
            .push(labelled_row("Name", char_name))
            .push(labelled_row("Class", class))
            .push(labelled_row("Level", level))
            .push(labelled_row("Experience", experience))
            .push(labelled_row("Spec Level", spec_level))
            .push(labelled_row("Spec Points", spec_points))
            .push(
                Container::new(
                    Text::new(
                        "Skill trees, gear, skins, SDUs and ammo are not yet editable for Borderlands 4 saves.",
                    )
                    .font(JETBRAINS_MONO)
                    .size(15)
                    .color(VALUE_COLOR),
                )
                .padding(10)
                .width(Length::Fill)
                .style(Bl3UiStyle),
            );

        if summary.point_pools.character_progress.is_some()
            || summary.point_pools.specialization_tokens.is_some()
            || summary.point_pools.echo_tokens.is_some()
            || !summary.point_pools.other.is_empty()
        {
            let mut pools = Column::new().spacing(4).push(
                Text::new("Point Pools")
                    .font(JETBRAINS_MONO_BOLD)
                    .size(20)
                    .color(LABEL_COLOR),
            );

            if let Some(value) = summary.point_pools.character_progress {
                pools = pools.push(
                    Text::new(format!("Character Progress: {value}"))
                        .font(JETBRAINS_MONO)
                        .size(16)
                        .color(VALUE_COLOR),
                );
            }
            if let Some(value) = summary.point_pools.specialization_tokens {
                pools = pools.push(
                    Text::new(format!("Specialization Tokens: {value}"))
                        .font(JETBRAINS_MONO)
                        .size(16)
                        .color(VALUE_COLOR),
                );
            }
            if let Some(value) = summary.point_pools.echo_tokens {
                pools = pools.push(
                    Text::new(format!("ECHO Tokens: {value}"))
                        .font(JETBRAINS_MONO)
                        .size(16)
                        .color(VALUE_COLOR),
                );
            }
            for (name, value) in &summary.point_pools.other {
                pools = pools.push(
                    Text::new(format!("{name}: {value}"))
                        .font(JETBRAINS_MONO)
                        .size(16)
                        .color(VALUE_COLOR),
                );
            }

            info = info.push(
                Container::new(pools)
                    .padding(10)
                    .width(Length::Fill)
                    .style(Bl3UiStyle),
            );
        }

        let sdu_pairs = [
            ("Backpack", summary.sdu_levels.backpack),
            ("Pistol Ammo", summary.sdu_levels.pistol),
            ("SMG Ammo", summary.sdu_levels.smg),
            ("Assault Rifle Ammo", summary.sdu_levels.assault_rifle),
            ("Shotgun Ammo", summary.sdu_levels.shotgun),
            ("Sniper Ammo", summary.sdu_levels.sniper),
            ("Heavy Ammo", summary.sdu_levels.heavy),
            ("Grenade Ammo", summary.sdu_levels.grenade),
            ("Bank Slots", summary.sdu_levels.bank),
            ("Lost Loot Slots", summary.sdu_levels.lost_loot),
        ];

        if sdu_pairs.iter().any(|(_, level)| *level > 0) {
            let mut sdu_section = Column::new().spacing(4).push(
                Text::new("SDU Levels")
                    .font(JETBRAINS_MONO_BOLD)
                    .size(20)
                    .color(LABEL_COLOR),
            );

            for (label, level) in sdu_pairs {
                if level > 0 {
                    sdu_section = sdu_section.push(
                        Text::new(format!("{label}: {level}"))
                            .font(JETBRAINS_MONO)
                            .size(16)
                            .color(VALUE_COLOR),
                    );
                }
            }

            info = info.push(
                Container::new(sdu_section)
                    .padding(10)
                    .width(Length::Fill)
                    .style(Bl3UiStyle),
            );
        }

        let skill_trees: Vec<_> = summary
            .skill_trees
            .iter()
            .filter(|tree| tree.name != "sdu_upgrades")
            .collect();

        if !skill_trees.is_empty() {
            let mut skills = Column::new().spacing(4).push(
                Text::new("Skill Trees")
                    .font(JETBRAINS_MONO_BOLD)
                    .size(20)
                    .color(LABEL_COLOR),
            );

            for tree in skill_trees.iter().take(12) {
                let total_points: i32 =
                    tree.nodes.iter().filter_map(|node| node.points_spent).sum();
                let active = tree
                    .nodes
                    .iter()
                    .filter(|node| node.is_activated.unwrap_or(false))
                    .count();

                skills = skills.push(
                    Text::new(format!(
                        "{} — {} nodes, {} points, {} active",
                        tree.name,
                        tree.nodes.len(),
                        total_points,
                        active
                    ))
                    .font(JETBRAINS_MONO)
                    .size(16)
                    .color(VALUE_COLOR),
                );
            }

            if skill_trees.len() > 12 {
                skills = skills.push(
                    Text::new(format!("... and {} more trees", skill_trees.len() - 12))
                        .font(JETBRAINS_MONO)
                        .size(15)
                        .color(VALUE_COLOR),
                );
            }

            info = info.push(
                Container::new(skills)
                    .padding(10)
                    .width(Length::Fill)
                    .style(Bl3UiStyle),
            );
        }

        if summary.equip_slots_unlocked.iter().any(|slot| *slot >= 0) {
            let mut slot_section = Column::new().spacing(4).push(
                Text::new("Unlocked Gear Slots")
                    .font(JETBRAINS_MONO_BOLD)
                    .size(20)
                    .color(LABEL_COLOR),
            );

            for slot in &summary.equip_slots_unlocked {
                slot_section = slot_section.push(
                    Text::new(format!("{} (#{})", bl4_slot_name(*slot), slot))
                        .font(JETBRAINS_MONO)
                        .size(16)
                        .color(VALUE_COLOR),
                );
            }

            info = info.push(
                Container::new(slot_section)
                    .padding(10)
                    .width(Length::Fill)
                    .style(Bl3UiStyle),
            );
        }

        if let Some(cosmetics_section) = build_bl4_cosmetics_overview(summary) {
            info = info.push(cosmetics_section);
        }

        if !summary.unique_rewards.is_empty() {
            let mut rewards = Column::new().spacing(4).push(
                Text::new("Unique Rewards")
                    .font(JETBRAINS_MONO_BOLD)
                    .size(20)
                    .color(LABEL_COLOR),
            );

            for reward in summary.unique_rewards.iter().take(15) {
                rewards = rewards.push(
                    Text::new(reward.clone())
                        .font(JETBRAINS_MONO)
                        .size(16)
                        .color(VALUE_COLOR),
                );
            }

            if summary.unique_rewards.len() > 15 {
                rewards = rewards.push(
                    Text::new(format!(
                        "... and {} additional rewards",
                        summary.unique_rewards.len() - 15
                    ))
                    .font(JETBRAINS_MONO)
                    .size(15)
                    .color(VALUE_COLOR),
                );
            }

            info = info.push(
                Container::new(rewards)
                    .padding(10)
                    .width(Length::Fill)
                    .style(Bl3UiStyle),
            );
        }

        return Container::new(info).padding(30);
    }

    let selected_class = character_state.player_class_selected_class;
    let show_action_tab = view_mode == Bl4ViewMode::Classic;

    if show_action_tab && character_state.detail_tab == CharacterDetailTab::ActionSkills {
        let metadata = resolve_bl4_skill_metadata(
            character_state.bl4_class_selected.as_deref(),
            bl4_summary,
        );
        let action_view = build_bl4_action_skills_tab(
            Bl4ActionSkillsContext {
                skill_trees: &mut character_state.bl4_skill_trees,
                tree_selector_state: &mut character_state.bl4_action_tree_selector,
                selected_tree: &mut character_state.bl4_action_tree_selected,
                available_points: character_state.bl4_progress_character_input,
                mass_input_state: &mut character_state.bl4_skill_points_mass_input_state,
                mass_input_value: character_state.bl4_skill_points_mass_input,
                apply_button_state: &mut character_state.bl4_skill_apply_button_state,
                max_button_state: &mut character_state.bl4_skill_max_button_state,
                reset_button_state: &mut character_state.bl4_skill_reset_button_state,
                clear_missions_button_state: &mut character_state.bl4_clear_missions_button_state,
            },
            metadata,
        );
        let tab_row = build_character_detail_tabs(
            character_state.detail_tab,
            &mut character_state.detail_tab_overview_button_state,
            Some(&mut character_state.detail_tab_action_button_state),
        );
        let content = Column::new()
            .spacing(20)
            .push(tab_row)
            .push(action_view);
        let scrollable = Scrollable::new(&mut character_state.scroll).push(content);
        return Container::new(scrollable)
            .padding(30)
            .width(Length::Fill)
            .height(Length::Fill);
    }

    let detail_tab_row = if show_action_tab {
        Some(build_character_detail_tabs(
            character_state.detail_tab,
            &mut character_state.detail_tab_overview_button_state,
            Some(&mut character_state.detail_tab_action_button_state),
        ))
    } else {
        None
    };

    let character_name = Container::new(
        LabelledElement::create(
            "Name",
            Length::Units(75),
            TextInputLimited::new(
                &mut character_state.name_input_state,
                "FL4K",
                &character_state.name_input,
                500,
                |c| {
                    InteractionMessage::ManageSaveInteraction(
                        ManageSaveInteractionMessage::Character(
                            SaveCharacterInteractionMessage::Name(c),
                        ),
                    )
                },
            )
            .0
            .font(JETBRAINS_MONO)
            .padding(10)
            .size(17)
            .style(Bl3UiStyle)
            .into_element(),
        )
        .align_items(Alignment::Center),
    )
    .width(Length::FillPortion(3))
    .height(Length::Shrink)
    .style(Bl3UiStyle);

    let player_class = if matches!(bl4_summary, Some(_)) && view_mode == Bl4ViewMode::Classic {
        let options: Vec<String> = BL4_CLASSES.iter().map(|c| (*c).to_string()).collect();
        Container::new(
            LabelledElement::create(
                "Class",
                Length::Units(65),
                PickList::new(
                    &mut character_state.bl4_class_selector,
                    options,
                    character_state.bl4_class_selected.clone(),
                    |value| {
                        InteractionMessage::ManageSaveInteraction(
                            ManageSaveInteractionMessage::Character(
                                SaveCharacterInteractionMessage::Bl4ClassChanged(value),
                            ),
                        )
                    },
                )
                .font(JETBRAINS_MONO)
                .text_size(17)
                .width(Length::Fill)
                .padding(10)
                .style(Bl3UiStyle)
                .into_element(),
            )
            .align_items(Alignment::Center),
        )
        .width(Length::FillPortion(1))
        .height(Length::Shrink)
        .style(Bl3UiStyle)
    } else {
        Container::new(
            LabelledElement::create(
                "Class",
                Length::Units(65),
                PickList::new(
                    &mut character_state.player_class_selector,
                    &PlayerClass::ALL[..],
                    Some(selected_class),
                    |c| {
                        InteractionMessage::ManageSaveInteraction(
                            ManageSaveInteractionMessage::Character(
                                SaveCharacterInteractionMessage::PlayerClassSelected(c),
                            ),
                        )
                    },
                )
                .font(JETBRAINS_MONO)
                .text_size(17)
                .width(Length::Fill)
                .padding(10)
                .style(Bl3UiStyle)
                .into_element(),
            )
            .align_items(Alignment::Center),
        )
        .width(Length::FillPortion(1))
        .height(Length::Shrink)
        .style(Bl3UiStyle)
    };

    let name_class_row = Row::new()
        .push(character_name)
        .push(player_class)
        .spacing(20);

    let level = Container::new(
        LabelledElement::create(
            "Level",
            Length::Units(60),
            Tooltip::new(
                NumberInput::new(
                    &mut character_state.xp_level_input_state,
                    character_state.level_input,
                    1,
                    Some(MAX_CHARACTER_LEVEL as i32),
                    |v| {
                        InteractionMessage::ManageSaveInteraction(
                            ManageSaveInteractionMessage::Character(
                                SaveCharacterInteractionMessage::Level(v),
                            ),
                        )
                    },
                )
                .0
                .font(JETBRAINS_MONO)
                .padding(10)
                .size(17)
                .style(Bl3UiStyle)
                .into_element(),
                format!("Level must be between 1 and {}", MAX_CHARACTER_LEVEL),
                tooltip::Position::Top,
            )
            .gap(10)
            .padding(10)
            .font(JETBRAINS_MONO)
            .size(17)
            .style(Bl3UiTooltipStyle),
        )
        .spacing(15)
        .align_items(Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Shrink)
    .style(Bl3UiStyle);

    let experience_points = Container::new(
        LabelledElement::create(
            "Experience",
            Length::Units(95),
            Tooltip::new(
                NumberInput::new(
                    &mut character_state.experience_points_input_state,
                    character_state.experience_points_input,
                    0,
                    Some(REQUIRED_XP_LIST[MAX_CHARACTER_LEVEL - 1][0]),
                    |v| {
                        InteractionMessage::ManageSaveInteraction(
                            ManageSaveInteractionMessage::Character(
                                SaveCharacterInteractionMessage::ExperiencePoints(v),
                            ),
                        )
                    },
                )
                .0
                .font(JETBRAINS_MONO)
                .padding(10)
                .size(17)
                .style(Bl3UiStyle)
                .into_element(),
                "Experience must be between 0 and 9,520,932",
                tooltip::Position::Top,
            )
            .gap(10)
            .padding(10)
            .font(JETBRAINS_MONO)
            .size(17)
            .style(Bl3UiTooltipStyle),
        )
        .spacing(15)
        .align_items(Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Shrink)
    .style(Bl3UiStyle);

    let ability_points = Container::new(
        LabelledElement::create(
            "Skill Points",
            Length::Units(130),
            NumberInput::new(
                &mut character_state.ability_points_input_state,
                character_state.ability_points_input,
                0,
                Some(i32::MAX),
                |v| {
                    InteractionMessage::ManageSaveInteraction(
                        ManageSaveInteractionMessage::Character(
                            SaveCharacterInteractionMessage::AbilityPoints(v),
                        ),
                    )
                },
            )
            .0
            .font(JETBRAINS_MONO)
            .padding(10)
            .size(17)
            .style(Bl3UiStyle)
            .into_element(),
        )
        .spacing(15)
        .align_items(Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Shrink)
    .style(Bl3UiStyle);

    let experience_and_level_row = Row::new()
        .push(level)
        .push(experience_points)
        .push(ability_points)
        .spacing(20);

    let all_contents = if let Some(_summary) = bl4_summary {
        let cosmetics_editor = {
            let header = Text::new("Cosmetics Editor")
                .font(JETBRAINS_MONO_BOLD)
                .size(18)
                .color(LABEL_COLOR);

            let body_input = LabelledElement::create(
                "Body",
                Length::Units(70),
                TextInput::new(
                    &mut character_state.bl4_body_input_state,
                    "Cosmetics_Player_Body",
                    &character_state.bl4_body_input,
                    |value| {
                        InteractionMessage::ManageSaveInteraction(
                            ManageSaveInteractionMessage::Character(
                                SaveCharacterInteractionMessage::Bl4CosmeticBody(value),
                            ),
                        )
                    },
                )
                .font(JETBRAINS_MONO)
                .padding(10)
                .size(16)
                .style(Bl3UiStyle)
                .into_element(),
            )
            .spacing(10)
            .width(Length::FillPortion(1));

            let head_input = LabelledElement::create(
                "Head",
                Length::Units(70),
                TextInput::new(
                    &mut character_state.bl4_head_input_state,
                    "Cosmetics_Player_Head",
                    &character_state.bl4_head_input,
                    |value| {
                        InteractionMessage::ManageSaveInteraction(
                            ManageSaveInteractionMessage::Character(
                                SaveCharacterInteractionMessage::Bl4CosmeticHead(value),
                            ),
                        )
                    },
                )
                .font(JETBRAINS_MONO)
                .padding(10)
                .size(16)
                .style(Bl3UiStyle)
                .into_element(),
            )
            .spacing(10)
            .width(Length::FillPortion(1));

            let skin_input = LabelledElement::create(
                "Skin",
                Length::Units(70),
                TextInput::new(
                    &mut character_state.bl4_skin_input_state,
                    "Cosmetics_Player_Skin",
                    &character_state.bl4_skin_input,
                    |value| {
                        InteractionMessage::ManageSaveInteraction(
                            ManageSaveInteractionMessage::Character(
                                SaveCharacterInteractionMessage::Bl4CosmeticSkin(value),
                            ),
                        )
                    },
                )
                .font(JETBRAINS_MONO)
                .padding(10)
                .size(16)
                .style(Bl3UiStyle)
                .into_element(),
            )
            .spacing(10)
            .width(Length::FillPortion(1));

            let row_one = Row::new()
                .push(body_input)
                .push(head_input)
                .push(skin_input)
                .spacing(15);

            let primary_input = LabelledElement::create(
                "Primary",
                Length::Units(90),
                TextInput::new(
                    &mut character_state.bl4_primary_color_input_state,
                    "Cosmetics_Colorization_Primary",
                    &character_state.bl4_primary_color_input,
                    |value| {
                        InteractionMessage::ManageSaveInteraction(
                            ManageSaveInteractionMessage::Character(
                                SaveCharacterInteractionMessage::Bl4PrimaryColor(value),
                            ),
                        )
                    },
                )
                .font(JETBRAINS_MONO)
                .padding(10)
                .size(16)
                .style(Bl3UiStyle)
                .into_element(),
            )
            .spacing(10)
            .width(Length::FillPortion(1));

            let secondary_input = LabelledElement::create(
                "Secondary",
                Length::Units(90),
                TextInput::new(
                    &mut character_state.bl4_secondary_color_input_state,
                    "Cosmetics_Colorization_Secondary",
                    &character_state.bl4_secondary_color_input,
                    |value| {
                        InteractionMessage::ManageSaveInteraction(
                            ManageSaveInteractionMessage::Character(
                                SaveCharacterInteractionMessage::Bl4SecondaryColor(value),
                            ),
                        )
                    },
                )
                .font(JETBRAINS_MONO)
                .padding(10)
                .size(16)
                .style(Bl3UiStyle)
                .into_element(),
            )
            .spacing(10)
            .width(Length::FillPortion(1));

            let tertiary_input = LabelledElement::create(
                "Tertiary",
                Length::Units(90),
                TextInput::new(
                    &mut character_state.bl4_tertiary_color_input_state,
                    "Cosmetics_Colorization_Tertiary",
                    &character_state.bl4_tertiary_color_input,
                    |value| {
                        InteractionMessage::ManageSaveInteraction(
                            ManageSaveInteractionMessage::Character(
                                SaveCharacterInteractionMessage::Bl4TertiaryColor(value),
                            ),
                        )
                    },
                )
                .font(JETBRAINS_MONO)
                .padding(10)
                .size(16)
                .style(Bl3UiStyle)
                .into_element(),
            )
            .spacing(10)
            .width(Length::FillPortion(1));

            let row_two = Row::new()
                .push(primary_input)
                .push(secondary_input)
                .push(tertiary_input)
                .spacing(15);

            let echo_body_input = LabelledElement::create(
                "ECHO Body",
                Length::Units(110),
                TextInput::new(
                    &mut character_state.bl4_echo_body_input_state,
                    "Cosmetics_Echo4_Body",
                    &character_state.bl4_echo_body_input,
                    |value| {
                        InteractionMessage::ManageSaveInteraction(
                            ManageSaveInteractionMessage::Character(
                                SaveCharacterInteractionMessage::Bl4EchoBody(value),
                            ),
                        )
                    },
                )
                .font(JETBRAINS_MONO)
                .padding(10)
                .size(16)
                .style(Bl3UiStyle)
                .into_element(),
            )
            .spacing(10)
            .width(Length::FillPortion(1));

            let echo_attachment_input = LabelledElement::create(
                "ECHO Attachment",
                Length::Units(150),
                TextInput::new(
                    &mut character_state.bl4_echo_attachment_input_state,
                    "Cosmetics_Echo4_Attachment",
                    &character_state.bl4_echo_attachment_input,
                    |value| {
                        InteractionMessage::ManageSaveInteraction(
                            ManageSaveInteractionMessage::Character(
                                SaveCharacterInteractionMessage::Bl4EchoAttachment(value),
                            ),
                        )
                    },
                )
                .font(JETBRAINS_MONO)
                .padding(10)
                .size(16)
                .style(Bl3UiStyle)
                .into_element(),
            )
            .spacing(10)
            .width(Length::FillPortion(1));

            let echo_skin_input = LabelledElement::create(
                "ECHO Skin",
                Length::Units(110),
                TextInput::new(
                    &mut character_state.bl4_echo_skin_input_state,
                    "Cosmetics_Echo4_Skin",
                    &character_state.bl4_echo_skin_input,
                    |value| {
                        InteractionMessage::ManageSaveInteraction(
                            ManageSaveInteractionMessage::Character(
                                SaveCharacterInteractionMessage::Bl4EchoSkin(value),
                            ),
                        )
                    },
                )
                .font(JETBRAINS_MONO)
                .padding(10)
                .size(16)
                .style(Bl3UiStyle)
                .into_element(),
            )
            .spacing(10)
            .width(Length::FillPortion(1));

            let row_three = Row::new()
                .push(echo_body_input)
                .push(echo_attachment_input)
                .push(echo_skin_input)
                .spacing(15);

            let vehicle_skin_input = LabelledElement::create(
                "Vehicle Skin",
                Length::Units(120),
                TextInput::new(
                    &mut character_state.bl4_vehicle_skin_input_state,
                    "Cosmetics_Vehicle_Skin",
                    &character_state.bl4_vehicle_skin_input,
                    |value| {
                        InteractionMessage::ManageSaveInteraction(
                            ManageSaveInteractionMessage::Character(
                                SaveCharacterInteractionMessage::Bl4VehicleSkin(value),
                            ),
                        )
                    },
                )
                .font(JETBRAINS_MONO)
                .padding(10)
                .size(16)
                .style(Bl3UiStyle)
                .into_element(),
            )
            .spacing(10)
            .width(Length::Fill);

            Container::new(
                Column::new()
                    .push(header)
                    .push(row_one)
                    .push(row_two)
                    .push(row_three)
                    .push(vehicle_skin_input)
                    .spacing(15),
            )
            .padding(15)
            .width(Length::Fill)
            .style(Bl3UiStyle)
        };

        let equip_editor = {
            let header = Text::new("Unlocked Gear Slots")
                .font(JETBRAINS_MONO_BOLD)
                .size(17)
                .color(LABEL_COLOR);

            let input = TextInput::new(
                &mut character_state.bl4_equip_slots_input_state,
                "0,1,2,3,4,5,6,7",
                &character_state.bl4_equip_slots_input,
                |value| {
                    InteractionMessage::ManageSaveInteraction(
                        ManageSaveInteractionMessage::Character(
                            SaveCharacterInteractionMessage::Bl4EquipSlots(value),
                        ),
                    )
                },
            )
            .font(JETBRAINS_MONO)
            .padding(10)
            .size(16)
            .style(Bl3UiStyle)
            .width(Length::Fill)
            .into_element();

            Container::new(
                Column::new()
                    .push(header)
                    .push(
                        Text::new("Provide comma-separated slot indices to mark as unlocked.")
                            .font(JETBRAINS_MONO)
                            .size(13)
                            .color(VALUE_COLOR),
                    )
                    .push(input)
                    .spacing(10),
            )
            .padding(15)
            .width(Length::FillPortion(3))
            .style(Bl3UiStyle)
        };

        let ammo_editor = character_state
            .ammo_setter
            .view()
            .width(Length::FillPortion(2));

        let sdu_editor = character_state
            .sdu_unlocker
            .view()
            .width(Length::FillPortion(2));

        let progression_editor = {
            let header = Text::new("Progression")
                .font(JETBRAINS_MONO_BOLD)
                .size(17)
                .color(LABEL_COLOR);

            let character_points = LabelledElement::create(
                "Character",
                Length::Units(95),
                NumberInput::new(
                    &mut character_state.bl4_progress_character_input_state,
                    character_state.bl4_progress_character_input,
                    0,
                    None,
                    |v| {
                        InteractionMessage::ManageSaveInteraction(
                            ManageSaveInteractionMessage::Character(
                                SaveCharacterInteractionMessage::Bl4ProgressCharacter(v),
                            ),
                        )
                    },
                )
                .0
                .font(JETBRAINS_MONO)
                .padding(10)
                .size(16)
                .style(Bl3UiStyle)
                .into_element(),
            )
            .spacing(12)
            .align_items(Alignment::Center);

            let specialization_points = LabelledElement::create(
                "Specialization",
                Length::Units(130),
                NumberInput::new(
                    &mut character_state.bl4_progress_specialization_input_state,
                    character_state.bl4_progress_specialization_input,
                    0,
                    None,
                    |v| {
                        InteractionMessage::ManageSaveInteraction(
                            ManageSaveInteractionMessage::Character(
                                SaveCharacterInteractionMessage::Bl4ProgressSpecialization(v),
                            ),
                        )
                    },
                )
                .0
                .font(JETBRAINS_MONO)
                .padding(10)
                .size(16)
                .style(Bl3UiStyle)
                .into_element(),
            )
            .spacing(12)
            .align_items(Alignment::Center);

            let echo_points = LabelledElement::create(
                "ECHO",
                Length::Units(60),
                NumberInput::new(
                    &mut character_state.bl4_progress_echo_input_state,
                    character_state.bl4_progress_echo_input,
                    0,
                    None,
                    |v| {
                        InteractionMessage::ManageSaveInteraction(
                            ManageSaveInteractionMessage::Character(
                                SaveCharacterInteractionMessage::Bl4ProgressEcho(v),
                            ),
                        )
                    },
                )
                .0
                .font(JETBRAINS_MONO)
                .padding(10)
                .size(16)
                .style(Bl3UiStyle)
                .into_element(),
            )
            .spacing(12)
            .align_items(Alignment::Center);

            let apply_all_button = Button::new(
                &mut character_state.bl4_skill_apply_button_state,
                Text::new("Apply To All")
                    .font(JETBRAINS_MONO_BOLD)
                    .size(14)
                    .color(VALUE_COLOR),
            )
            .on_press(InteractionMessage::ManageSaveInteraction(
                ManageSaveInteractionMessage::Character(
                    SaveCharacterInteractionMessage::Bl4SkillApplyAll,
                ),
            ))
            .padding(8)
            .style(Bl3UiStyle)
            .into_element();

            let max_all_button = Button::new(
                &mut character_state.bl4_skill_max_button_state,
                Text::new("Max Out (5)")
                    .font(JETBRAINS_MONO_BOLD)
                    .size(14)
                    .color(VALUE_COLOR),
            )
            .on_press(InteractionMessage::ManageSaveInteraction(
                ManageSaveInteractionMessage::Character(
                    SaveCharacterInteractionMessage::Bl4SkillMaxAll,
                ),
            ))
            .padding(8)
            .style(Bl3UiStyle)
            .into_element();

            let skill_mass_input = NumberInput::new(
                &mut character_state.bl4_skill_points_mass_input_state,
                character_state.bl4_skill_points_mass_input,
                0,
                Some(999),
                |v| {
                    InteractionMessage::ManageSaveInteraction(
                        ManageSaveInteractionMessage::Character(
                            SaveCharacterInteractionMessage::Bl4SkillMassInput(v),
                        ),
                    )
                },
            )
            .0
            .font(JETBRAINS_MONO)
            .padding(8)
            .size(15)
            .width(Length::Units(100))
            .style(Bl3UiStyle)
            .into_element();

            let reset_all_button = Button::new(
                &mut character_state.bl4_skill_reset_button_state,
                Text::new("Reset All")
                    .font(JETBRAINS_MONO_BOLD)
                    .size(14)
                    .color(VALUE_COLOR),
            )
            .on_press(InteractionMessage::ManageSaveInteraction(
                ManageSaveInteractionMessage::Character(
                    SaveCharacterInteractionMessage::Bl4SkillResetAll,
                ),
            ))
            .padding(8)
            .style(Bl3UiStyle)
            .into_element();

            Container::new(
                Column::new()
                    .push(header)
                    .push(character_points)
                    .push(specialization_points)
                    .push(echo_points)
                    .spacing(12),
            )
            .padding(15)
            .width(Length::FillPortion(2))
            .style(Bl3UiStyle)
        };

        let unique_rewards_editor = {
            let header = Text::new("Unique Rewards")
                .font(JETBRAINS_MONO_BOLD)
                .size(17)
                .color(LABEL_COLOR);

            let input = TextInput::new(
                &mut character_state.bl4_unique_rewards_input_state,
                "reward_a, reward_b",
                &character_state.bl4_unique_rewards_input,
                |value| {
                    InteractionMessage::ManageSaveInteraction(
                        ManageSaveInteractionMessage::Character(
                            SaveCharacterInteractionMessage::Bl4UniqueRewards(value),
                        ),
                    )
                },
            )
            .font(JETBRAINS_MONO)
            .padding(10)
            .size(16)
            .style(Bl3UiStyle)
            .width(Length::Fill)
            .into_element();

            Container::new(
                Column::new()
                    .push(header)
                    .push(
                        Text::new("Comma-separated reward asset paths.")
                            .font(JETBRAINS_MONO)
                            .size(13)
                            .color(VALUE_COLOR),
                    )
                    .push(input)
                    .spacing(10),
            )
            .padding(15)
            .width(Length::Fill)
            .style(Bl3UiStyle)
        };

        // Two-column layout to prevent large empty gaps:
        // Left column stacks wide editors; right column stacks compact editors.
        Column::new()
            .push(name_class_row)
            .push(experience_and_level_row)
            .push(cosmetics_editor)
            .push(
                Row::new()
                    .push(
                        Column::new()
                            .push(equip_editor)
                            .push(progression_editor)
                            .spacing(20)
                            .width(Length::FillPortion(3)),
                    )
                    .push(
                        Column::new()
                            .push(ammo_editor)
                            .push(sdu_editor)
                            .spacing(20)
                            .width(Length::FillPortion(2)),
                    )
                    .spacing(20),
            )
            .push(unique_rewards_editor)
            .spacing(20)
    } else {
        let skin_unlocker = character_state.skin_selectors.view(&selected_class);
        let gear_unlocker = character_state
            .gear_unlocker
            .view()
            .width(Length::FillPortion(3));
        let ammo_setter = character_state
            .ammo_setter
            .view()
            .width(Length::FillPortion(2));
        let sdu_unlocker = character_state
            .sdu_unlocker
            .view()
            .width(Length::FillPortion(2));

        Column::new()
            .push(
                Row::new()
                    .push(
                        Column::new()
                            .push(name_class_row)
                            .push(experience_and_level_row)
                            .push(skin_unlocker)
                            .push(gear_unlocker)
                            .spacing(20)
                            .width(Length::FillPortion(3)),
                    )
                    .push(
                        Column::new()
                            .push(ammo_setter)
                            .push(sdu_unlocker)
                            .spacing(20)
                            .width(Length::FillPortion(2)),
                    )
                    .spacing(20),
            )
            .spacing(20)
    };

    let final_contents = if let Some(row) = detail_tab_row {
        Column::new().spacing(20).push(row).push(all_contents)
    } else {
        all_contents
    };

    let scrollable = Scrollable::new(&mut character_state.scroll).push(final_contents);
    Container::new(scrollable)
        .padding(30)
        .width(Length::Fill)
        .height(Length::Fill)
}

fn format_skill_tree_label(raw: &str) -> String {
    raw.split('.')
        .last()
        .unwrap_or(raw)
        .replace('_', " ")
        .to_title_case()
}

fn is_primary_skill_tree_name(name: &str) -> bool {
    if name.eq_ignore_ascii_case("sdu_upgrades") {
        return false;
    }
    if name.starts_with("ProgressGraph_Specializations") {
        return false;
    }
    true
}

fn build_character_detail_tabs<'a>(
    detail_tab: CharacterDetailTab,
    overview_button_state: &'a mut button::State,
    action_button_state: Option<&'a mut button::State>,
) -> Element<'a, Bl3Message> {
    let overview_selected = detail_tab == CharacterDetailTab::Overview;
    let overview_button = {
        let mut button = Button::new(
            overview_button_state,
            Text::new("Overview")
                .font(JETBRAINS_MONO_BOLD)
                .size(15)
                .color(if overview_selected { LABEL_COLOR } else { VALUE_COLOR }),
        )
        .padding(6)
        .style(Bl3UiStyle);
        if !overview_selected {
            button = button.on_press(InteractionMessage::ManageSaveInteraction(
                ManageSaveInteractionMessage::Character(
                    SaveCharacterInteractionMessage::DetailTabChanged(
                        CharacterDetailTab::Overview,
                    ),
                ),
            ));
        }
        button
    };

    let mut row = Row::new().spacing(10).push(overview_button);

    if let Some(action_button_state) = action_button_state {
        let action_selected = detail_tab == CharacterDetailTab::ActionSkills;
        let mut action_button = Button::new(
            action_button_state,
            Text::new("Action Skills")
                .font(JETBRAINS_MONO_BOLD)
                .size(15)
                .color(if action_selected { LABEL_COLOR } else { VALUE_COLOR }),
        )
        .padding(6)
        .style(Bl3UiStyle);
        if !action_selected {
            action_button = action_button.on_press(InteractionMessage::ManageSaveInteraction(
                ManageSaveInteractionMessage::Character(
                    SaveCharacterInteractionMessage::DetailTabChanged(
                        CharacterDetailTab::ActionSkills,
                    ),
                ),
            ));
        }
        row = row.push(action_button);
    }

    row.into_element()
}

fn build_bl4_action_skills_tab<'a>(
    ctx: Bl4ActionSkillsContext<'a>,
    metadata: Option<&'a VaultHunterEntry>,
) -> Container<'a, Bl3Message> {
    let Bl4ActionSkillsContext {
        skill_trees,
        tree_selector_state,
        selected_tree,
        available_points,
        mass_input_state,
        mass_input_value,
        apply_button_state,
        max_button_state,
        reset_button_state,
        clear_missions_button_state,
    } = ctx;
    let mut column = Column::new().spacing(12);

    if skill_trees.is_empty() {
        column = column.push(
            Text::new("No skill graph data detected in this save.")
                .font(JETBRAINS_MONO)
                .size(13)
                .color(VALUE_COLOR),
        );
        return Container::new(column)
            .padding(10)
            .width(Length::Fill)
            .style(Bl3UiStyle);
    }

    let Some(metadata) = metadata else {
        column = column.push(
            Text::new("Select a BL4 class to view action skills.")
                .font(JETBRAINS_MONO)
                .size(13)
                .color(VALUE_COLOR),
        );
        return Container::new(column)
            .padding(10)
            .width(Length::Fill)
            .style(Bl3UiStyle);
    };

    let mut skill_summary_column = Column::new().spacing(6);
    let total_available_points = available_points.max(0);
    let total_spent_points = bl4_total_points_spent(skill_trees);
    let remaining_points = (total_available_points - total_spent_points).max(0);
    skill_summary_column = skill_summary_column.push(
        Text::new(format!(
            "Total Points: {} / {} ({} remaining)",
            total_spent_points, total_available_points, remaining_points
        ))
        .font(JETBRAINS_MONO_BOLD)
        .size(13)
        .color(VALUE_COLOR),
    );
    if skill_trees.is_empty() {
        skill_summary_column = skill_summary_column.push(
            Text::new("No progression data detected in this save.")
                .font(JETBRAINS_MONO)
                .size(13)
                .color(VALUE_COLOR),
        );
    } else {
        for tree in skill_trees.iter() {
            let node_count = tree.nodes.len();
            let total_points: i32 = tree
                .nodes
                .iter()
                .map(|node| node.points_spent.unwrap_or(0))
                .sum();
            let active_nodes = tree
                .nodes
                .iter()
                .filter(|node| {
                    node.is_activated.unwrap_or(false) || node.points_spent.unwrap_or(0) > 0
                })
                .count();
            skill_summary_column = skill_summary_column.push(
                Text::new(format!(
                    "{} — {} nodes, {} points, {} active",
                    format_skill_tree_label(&tree.name),
                    node_count,
                    total_points,
                    active_nodes
                ))
                .font(JETBRAINS_MONO)
                .size(13)
                .color(VALUE_COLOR),
            );
        }
    }

    let skill_mass_input = NumberInput::new(
        mass_input_state,
        mass_input_value,
        0,
        Some(999),
        |v| {
            InteractionMessage::ManageSaveInteraction(
                ManageSaveInteractionMessage::Character(
                    SaveCharacterInteractionMessage::Bl4SkillMassInput(v),
                ),
            )
        },
    )
    .0
    .font(JETBRAINS_MONO)
    .padding(8)
    .size(15)
    .width(Length::Units(100))
    .style(Bl3UiStyle)
    .into_element();

    let reset_all_button = Button::new(
        reset_button_state,
        Text::new("Reset All")
            .font(JETBRAINS_MONO_BOLD)
            .size(14)
            .color(VALUE_COLOR),
    )
    .on_press(InteractionMessage::ManageSaveInteraction(
        ManageSaveInteractionMessage::Character(
            SaveCharacterInteractionMessage::Bl4SkillResetAll,
        ),
    ))
    .padding(8)
    .style(Bl3UiStyle)
    .into_element();

    let apply_all_button = Button::new(
        apply_button_state,
        Text::new("Apply To All")
            .font(JETBRAINS_MONO_BOLD)
            .size(14)
            .color(VALUE_COLOR),
    )
    .on_press(InteractionMessage::ManageSaveInteraction(
        ManageSaveInteractionMessage::Character(
            SaveCharacterInteractionMessage::Bl4SkillApplyAll,
        ),
    ))
    .padding(8)
    .style(Bl3UiStyle)
    .into_element();

    let max_all_button = Button::new(
        max_button_state,
        Text::new("Max Out (5)")
            .font(JETBRAINS_MONO_BOLD)
            .size(14)
            .color(VALUE_COLOR),
    )
    .on_press(InteractionMessage::ManageSaveInteraction(
        ManageSaveInteractionMessage::Character(
            SaveCharacterInteractionMessage::Bl4SkillMaxAll,
        ),
    ))
    .padding(8)
    .style(Bl3UiStyle)
    .into_element();

    let skill_bulk_controls = Row::new()
        .spacing(12)
        .align_items(Alignment::Center)
        .push(
            Text::new("Set All")
                .font(JETBRAINS_MONO_BOLD)
                .size(16)
                .color(LABEL_COLOR)
                .width(Length::Units(90)),
        )
        .push(skill_mass_input)
        .push(reset_all_button)
        .push(apply_all_button)
        .push(max_all_button);

    let clear_missions_button = Button::new(
        clear_missions_button_state,
        Text::new("Clear Mission Progress")
            .font(JETBRAINS_MONO_BOLD)
            .size(14)
            .color(VALUE_COLOR),
    )
    .on_press(InteractionMessage::ManageSaveInteraction(
        ManageSaveInteractionMessage::Character(
            SaveCharacterInteractionMessage::Bl4ClearMissions,
        ),
    ))
    .padding(8)
    .style(Bl3UiStyle)
    .into_element();

    let skill_controls = Container::new(
        Column::new()
            .spacing(10)
            .push(skill_bulk_controls)
            .push(
                Text::new(
                    "Apply uses the value above; Max Out forces 5 points and activates every node.",
                )
                .font(JETBRAINS_MONO)
                .size(12)
                .color(VALUE_COLOR),
            )
            .push(Row::new().spacing(12).push(clear_missions_button))
            .push(
                Container::new(skill_summary_column)
                    .padding(10)
                    .width(Length::Fill)
                    .style(Bl3UiStyle),
            ),
    )
    .padding(10)
    .style(Bl3UiStyle);

    column = column.push(skill_controls);

    sync_bl4_skill_trees_with_metadata(skill_trees, metadata);
    let tree_names = metadata.tree_names();
    if tree_names.is_empty() {
        column = column.push(
            Text::new("No skill tree metadata available for this class.")
                .font(JETBRAINS_MONO)
                .size(13)
                .color(VALUE_COLOR),
        );
        return Container::new(column)
            .padding(10)
            .width(Length::Fill)
            .style(Bl3UiStyle);
    }

    if selected_tree
        .as_ref()
        .map(|selected| !tree_names.contains(selected))
        .unwrap_or(true)
    {
        *selected_tree = tree_names.first().cloned();
    }
    let selected_tree_name = selected_tree
        .clone()
        .unwrap_or_else(|| tree_names[0].clone());

    let tree_selector = PickList::new(
        tree_selector_state,
        tree_names.clone(),
        selected_tree.clone(),
        |value| {
            InteractionMessage::ManageSaveInteraction(
                ManageSaveInteractionMessage::Character(
                    SaveCharacterInteractionMessage::Bl4ActionSkillTreeChanged(value),
                ),
            )
        },
    )
    .font(JETBRAINS_MONO)
    .text_size(14)
    .width(Length::FillPortion(2))
    .padding(6)
    .style(Bl3UiStyle)
    .into_element();

    let Some(tree_metadata) = metadata.tree_by_name(&selected_tree_name) else {
        column = column.push(
            Text::new("Tree metadata not found.")
                .font(JETBRAINS_MONO)
                .size(13)
                .color(VALUE_COLOR),
        );
        return Container::new(column)
            .padding(10)
            .width(Length::Fill)
            .style(Bl3UiStyle);
    };

    let total_spent_all = bl4_total_points_spent(skill_trees);

    // Pick the tree that best matches the selected metadata tree so we render real save data.
    let mut best_idx: Option<usize> = None;
    let mut best_score: usize = 0;
    for (idx, tree_state) in skill_trees.iter().enumerate() {
        let mut score = 0;
        let name_match = tree_name_matches_metadata(&tree_state.name, &tree_metadata.name)
            || tree_state
                .group_def_name
                .as_deref()
                .map(|name| tree_name_matches_metadata(name, &tree_metadata.name))
                .unwrap_or(false);
        if name_match {
            score += 1_000;
        }
        let overlap = tree_state
            .nodes
            .iter()
            .filter(|node| tree_metadata.find_skill(&node.name).is_some())
            .count();
        score += overlap;
        if score > best_score {
            best_score = score;
            best_idx = Some(idx);
        }
    }

    let tree_index = best_idx.unwrap_or(0);
    let Some(tree_state) = skill_trees.get_mut(tree_index) else {
        column = column.push(
            Text::new("No skill graph data detected in this save.")
                .font(JETBRAINS_MONO)
                .size(13)
                .color(VALUE_COLOR),
        );
        return Container::new(column)
            .padding(10)
            .width(Length::Fill)
            .style(Bl3UiStyle);
    };

    let mut action_cards: Vec<Container<'a, Bl3Message>> = Vec::new();
    let mut augment_cards: Vec<Container<'a, Bl3Message>> = Vec::new();
    let mut capstone_cards: Vec<Container<'a, Bl3Message>> = Vec::new();
    let mut other_cards: Vec<Container<'a, Bl3Message>> = Vec::new();
    let skill_lookup: HashMap<String, &SkillEntry> = tree_metadata
        .skills
        .iter()
        .map(|entry| (entry.name.to_ascii_lowercase(), entry))
        .collect();

    for (node_index, node_state) in tree_state.nodes.iter_mut().enumerate() {
        let key = node_state.name.to_ascii_lowercase();
        let skill_meta = skill_lookup.get(&key).copied();
        let display_type = skill_meta.map(|m| m.skill_type).unwrap_or(SkillType::Other);
        let max_points = skill_meta.map(|m| m.max_rank.max(0));
        let card = render_action_node_card(
            node_state,
            &node_state.name,
            display_type,
            max_points,
            tree_index,
            node_index,
        );
        match display_type {
            SkillType::ActionSkill => action_cards.push(card),
            SkillType::Augment => augment_cards.push(card),
            SkillType::Capstone => capstone_cards.push(card),
            _ => other_cards.push(card),
        }
    }

    if action_cards.is_empty() && augment_cards.is_empty() && capstone_cards.is_empty() && other_cards.is_empty() {
        column = column.push(
            Text::new("No nodes for this action skill tree were found in the save.")
                .font(JETBRAINS_MONO)
                .size(13)
                .color(VALUE_COLOR),
        );
        return Container::new(column)
            .padding(10)
            .width(Length::Fill)
            .style(Bl3UiStyle);
    }

    column = column
        .push(
            Row::new()
                .spacing(10)
                .align_items(Alignment::Center)
                .push(
                    Text::new(tree_metadata.name.clone())
                        .font(JETBRAINS_MONO_BOLD)
                        .size(18)
                        .color(LABEL_COLOR),
                )
                .push(tree_selector),
        )
        .push(
            Text::new(format!(
                "Points Remaining: {}",
                (available_points - total_spent_all).max(0)
            ))
            .font(JETBRAINS_MONO)
            .size(13)
            .color(VALUE_COLOR),
        );

    if let Some(section) = render_action_group("Action Skills", action_cards, true)
    {
        column = column.push(section);
    }
    if let Some(section) = render_action_group("Augments", augment_cards, true) {
        column = column.push(section);
    }
    if let Some(section) = render_action_group("Capstones", capstone_cards, true) {
        column = column.push(section);
    }
    if let Some(section) = render_action_group("Additional Skills", other_cards, false)
    {
        column = column.push(section);
    }

    Container::new(column)
        .padding(10)
        .width(Length::Fill)
        .style(Bl3UiStyle)
}

fn render_action_group<'a>(
    title: &str,
    nodes: Vec<Container<'a, Bl3Message>>,
    emphasize: bool,
) -> Option<Element<'a, Bl3Message>> {
    if nodes.is_empty() {
        return None;
    }
    let mut section = Column::new().spacing(8).push(
        Text::new(title)
            .font(JETBRAINS_MONO_BOLD)
            .size(if emphasize { 16 } else { 15 })
            .color(LABEL_COLOR),
    );
    let mut iter = nodes.into_iter();
    while let Some(first) = iter.next() {
        let mut row = Row::new().spacing(10).push(first);
        for _ in 0..3 {
            if let Some(extra) = iter.next() {
                row = row.push(extra);
            } else {
                break;
            }
        }
        section = section.push(row);
    }

    Some(
        Container::new(section)
            .padding(6)
            .width(Length::Fill)
            .style(Bl3UiStyle)
            .into(),
    )
}

fn render_action_node_card<'a>(
    node_state: &'a mut Bl4SkillNodeState,
    display_name: &str,
    display_type: SkillType,
    max_points: Option<i32>,
    tree_index: usize,
    node_index: usize,
) -> Container<'a, Bl3Message> {
    let mut column = Column::new()
        .spacing(4)
        .push(
            Text::new(display_name.to_string())
                .font(JETBRAINS_MONO_BOLD)
                .size(13)
                .color(VALUE_COLOR),
        )
        .push(
            Text::new(format_skill_type(display_type))
                .font(JETBRAINS_MONO)
                .size(11)
                .color(LABEL_COLOR),
        );

    if !is_toggle_type(display_type) {
        let points_input = NumberInput::new(
            &mut node_state.points_input_state,
            node_state.points_spent.unwrap_or(0).max(0),
            0,
            max_points,
            move |value| {
                InteractionMessage::ManageSaveInteraction(
                    ManageSaveInteractionMessage::Character(
                        SaveCharacterInteractionMessage::Bl4SkillNodePoints {
                            tree_index,
                            node_index,
                            value,
                        },
                    ),
                )
            },
        )
        .0
        .font(JETBRAINS_MONO)
        .size(13)
        .padding(6)
        .width(Length::Units(80))
        .style(Bl3UiStyle)
        .into_element();
        column = column.push(points_input);
    } else {
        let toggle = Checkbox::new(
            node_state.is_activated.unwrap_or(false),
            match display_type {
                SkillType::Augment => "Augment",
                SkillType::Capstone => "Capstone",
                _ => "Active",
            },
            move |value| {
                InteractionMessage::ManageSaveInteraction(
                    ManageSaveInteractionMessage::Character(
                        SaveCharacterInteractionMessage::Bl4SkillNodeToggle {
                            tree_index,
                            node_index,
                            value,
                        },
                    ),
                )
            },
        )
        .text_size(13)
        .text_color(LABEL_COLOR)
        .style(Bl3UiStyle)
        .into_element();
        column = column.push(toggle);
    }

    Container::new(column)
        .padding(8)
        .width(Length::FillPortion(1))
        .style(Bl3UiStyle)
}

fn format_skill_type(skill_type: SkillType) -> String {
    match skill_type {
        SkillType::Passive => "Passive".to_string(),
        SkillType::KillSkill => "Kill Skill".to_string(),
        SkillType::ActionSkill => "Action Skill".to_string(),
        SkillType::Augment => "Augment".to_string(),
        SkillType::Capstone => "Capstone".to_string(),
        SkillType::Other => "Other".to_string(),
    }
}

fn graph_matches_tree(graph_name: &str, friendly: &str) -> bool {
    let Some(label) = graph_tree_label(graph_name) else {
        return false;
    };
    let normalized = normalize_tree_label(&label);
    normalized == friendly || friendly.contains(&normalized) || normalized.contains(friendly)
}

fn graph_tree_label(graph_name: &str) -> Option<String> {
    if let Some(idx) = graph_name.rfind("_Trunk_") {
        return Some(graph_name[idx + "_Trunk_".len()..].to_string());
    }
    if let Some(idx) = graph_name.rfind("_Branch_") {
        let remainder = &graph_name[idx + "_Branch_".len()..];
        if let Some(end) = remainder.rfind('_') {
            return Some(remainder[..end].to_string());
        }
        return Some(remainder.to_string());
    }
    None
}

fn normalize_tree_label(label: &str) -> String {
    label
        .chars()
        .filter(|c| !c.is_ascii_whitespace() && *c != '_' && *c != '-')
        .collect::<String>()
        .to_ascii_lowercase()
}

pub fn tree_name_matches_metadata(tree_name: &str, metadata_name: &str) -> bool {
    let friendly = normalize_tree_label(metadata_name);
    if graph_matches_tree(tree_name, &friendly) {
        return true;
    }
    let normalized_tree = normalize_tree_label(tree_name);
    normalized_tree == friendly
        || normalized_tree.contains(&friendly)
        || friendly.contains(&normalized_tree)
}

#[derive(Clone, Copy)]
struct Bl4ClassGraphInfo {
    prefix: &'static str,
    group_name: &'static str,
}

fn bl4_class_graph_info(metadata: &VaultHunterEntry) -> Option<Bl4ClassGraphInfo> {
    let class = metadata.class.to_ascii_lowercase();
    match class.as_str() {
        "forgeknight" | "paladin" => Some(Bl4ClassGraphInfo {
            prefix: "PLD",
            group_name: "ProgressGroup_Paladin",
        }),
        "siren" | "darksiren" => Some(Bl4ClassGraphInfo {
            prefix: "DS",
            group_name: "ProgressGroup_DarkSiren",
        }),
        "gravitar" => Some(Bl4ClassGraphInfo {
            prefix: "Grav",
            group_name: "progress_group_gravitar",
        }),
        "exo soldier" | "exosoldier" => Some(Bl4ClassGraphInfo {
            prefix: "EXO",
            group_name: "progress_group_exo",
        }),
        _ => None,
    }
}

fn canonical_tree_graph_name(prefix: &str, tree_name: &str) -> String {
    let slug = canonical_tree_slug(tree_name);
    format!("Progress_{}_Trunk_{}", prefix, slug)
}

fn canonical_tree_slug(label: &str) -> String {
    let mut slug = String::new();
    for part in label
        .split(|c: char| !c.is_alphanumeric())
        .filter(|part| !part.is_empty())
    {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            slug.push(first.to_ascii_uppercase());
            for ch in chars {
                slug.push(ch.to_ascii_lowercase());
            }
        }
    }
    if slug.is_empty() {
        slug.push_str("Tree");
    }
    slug
}

pub fn sync_bl4_skill_trees_with_metadata(
    skill_trees: &mut Vec<Bl4SkillTreeState>,
    metadata: &VaultHunterEntry,
) {
    let class_info = bl4_class_graph_info(metadata);
    for tree_meta in &metadata.trees {
        // Pick the best-matching existing tree by name/group or node overlap to avoid duplicates.
        let mut best_idx: Option<usize> = None;
        let mut best_score: usize = 0;
        for (idx, tree_state) in skill_trees.iter().enumerate() {
            let mut score = 0;
            let name_match = tree_name_matches_metadata(&tree_state.name, &tree_meta.name)
                || tree_state
                    .group_def_name
                    .as_deref()
                    .map(|name| tree_name_matches_metadata(name, &tree_meta.name))
                    .unwrap_or(false);
            if name_match {
                score += 1_000; // strong preference for explicit name/group matches
            }
            let overlap = tree_state
                .nodes
                .iter()
                .filter(|node| tree_meta.find_skill(&node.name).is_some())
                .count();
            score += overlap;
            if score > best_score {
                best_score = score;
                best_idx = Some(idx);
            }
        }

        let target_idx = if best_score > 0 {
            best_idx
        } else {
            // No reasonable match; create a canonical tree so the user can CRUD missing trunks.
            let (name, group_def_name) = if let Some(info) = class_info {
                let canonical = canonical_tree_graph_name(info.prefix, &tree_meta.name);
                (canonical, Some(info.group_name.to_string()))
            } else {
                (tree_meta.name.clone(), Some(tree_meta.name.clone()))
            };
            if !skill_trees
                .iter()
                .any(|tree| tree.name.eq_ignore_ascii_case(&name))
            {
                skill_trees.push(Bl4SkillTreeState {
                    name,
                    group_def_name,
                    nodes: Vec::new(),
                });
            }
            skill_trees
                .iter()
                .position(|tree| tree_name_matches_metadata(&tree.name, &tree_meta.name))
        };

        let Some(tree_state) = target_idx.and_then(|idx| skill_trees.get_mut(idx)) else {
            continue;
        };

        for skill in &tree_meta.skills {
            if !tree_state
                .nodes
                .iter()
                .any(|node| node.name.eq_ignore_ascii_case(&skill.name))
            {
                tree_state.nodes.push(Bl4SkillNodeState {
                    name: skill.name.clone(),
                    ..Bl4SkillNodeState::default()
                });
            }
        }

        tree_state.nodes.sort_by(|a, b| {
            let idx_a = tree_meta
                .skills
                .iter()
                .position(|skill| skill.name.eq_ignore_ascii_case(&a.name))
                .unwrap_or(usize::MAX);
            let idx_b = tree_meta
                .skills
                .iter()
                .position(|skill| skill.name.eq_ignore_ascii_case(&b.name))
                .unwrap_or(usize::MAX);
            idx_a
                .cmp(&idx_b)
                .then_with(|| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()))
        });
    }
}

fn is_toggle_type(skill_type: SkillType) -> bool {
    matches!(
        skill_type,
        SkillType::ActionSkill | SkillType::Augment | SkillType::Capstone
    )
}

pub fn resolve_bl4_skill_metadata(
    selected_class: Option<&str>,
    summary: Option<&Bl4SaveSummary>,
) -> Option<&'static VaultHunterEntry> {
    let identifier = selected_class
        .or_else(|| summary.and_then(|s| s.class.as_deref()))
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())?;
    BL4_SKILL_METADATA.hunter_by_identifier(identifier)
}

pub fn bl4_metadata_for_state(
    state: &CharacterState,
    summary: Option<&Bl4SaveSummary>,
) -> Option<&'static VaultHunterEntry> {
    resolve_bl4_skill_metadata(state.bl4_class_selected.as_deref(), summary)
}

pub fn bl4_total_points_spent(skill_trees: &[Bl4SkillTreeState]) -> i32 {
    skill_trees
        .iter()
        .flat_map(|tree| tree.nodes.iter())
        .map(|node| node.points_spent.unwrap_or(0).max(0))
        .sum()
}

pub fn bl4_handle_skill_points_change(
    character_state: &mut CharacterState,
    metadata: Option<&VaultHunterEntry>,
    tree_index: usize,
    node_index: usize,
    requested: i32,
) {
    let Some(node_name) = character_state
        .bl4_skill_trees
        .get(tree_index)
        .and_then(|tree| tree.nodes.get(node_index))
        .map(|node| node.name.clone())
    else {
        return;
    };

    let Some(metadata) = metadata else {
        if let Some(node_state) = character_state.bl4_skill_trees.get_mut(tree_index).and_then(|tree| tree.nodes.get_mut(node_index)) {
            node_state.points_spent = Some(requested.max(0));
        }
        return;
    };

    let Some((tree_meta, skill_meta)) = metadata
        .trees
        .iter()
        .find_map(|tree| tree.find_skill(&node_name).map(|skill| (tree, skill)))
    else {
        if let Some(node_state) = character_state.bl4_skill_trees.get_mut(tree_index).and_then(|tree| tree.nodes.get_mut(node_index)) {
            node_state.points_spent = Some(requested.max(0));
        }
        return;
    };

    if is_toggle_type(skill_meta.skill_type) {
        if let Some(node_state) = character_state.bl4_skill_trees.get_mut(tree_index).and_then(|tree| tree.nodes.get_mut(node_index)) {
            node_state.is_activated = Some(requested > 0);
        }
        return;
    }

    let available = character_state.bl4_progress_character_input.max(0);
    let mut other_sum = 0;
    for tree in &character_state.bl4_skill_trees {
        for node in &tree.nodes {
            if node.name == node_name {
                continue;
            }
            if tree_meta.find_skill(&node.name).is_some() {
                other_sum += node.points_spent.unwrap_or(0);
            }
        }
    }
    let max_allowed = (available - other_sum).max(0);
    let clamped = requested.clamp(0, skill_meta.max_rank).min(max_allowed);
    if let Some(node_state) = character_state.bl4_skill_trees.get_mut(tree_index).and_then(|tree| tree.nodes.get_mut(node_index)) {
        node_state.points_spent = Some(clamped);
    }
}

pub fn bl4_handle_skill_toggle(
    character_state: &mut CharacterState,
    metadata: Option<&VaultHunterEntry>,
    tree_index: usize,
    node_index: usize,
    value: bool,
) {
    let Some(node_name) = character_state
        .bl4_skill_trees
        .get(tree_index)
        .and_then(|tree| tree.nodes.get(node_index))
        .map(|node| node.name.clone())
    else {
        return;
    };

    let Some(metadata) = metadata else {
        if let Some(node_state) = character_state.bl4_skill_trees.get_mut(tree_index).and_then(|tree| tree.nodes.get_mut(node_index)) {
            node_state.is_activated = Some(value);
        }
        return;
    };

    let Some((tree_meta, skill_meta)) = metadata
        .trees
        .iter()
        .find_map(|tree| tree.find_skill(&node_name).map(|skill| (tree, skill)))
    else {
        if let Some(node_state) = character_state.bl4_skill_trees.get_mut(tree_index).and_then(|tree| tree.nodes.get_mut(node_index)) {
            node_state.is_activated = Some(value);
        }
        return;
    };

    if !is_toggle_type(skill_meta.skill_type) {
        if let Some(node_state) = character_state.bl4_skill_trees.get_mut(tree_index).and_then(|tree| tree.nodes.get_mut(node_index)) {
            node_state.is_activated = Some(value);
        }
        return;
    }

    if value {
        for tree in &mut character_state.bl4_skill_trees {
            for node in &mut tree.nodes {
                if node.name != node_name {
                    if let Some(other_meta) = tree_meta.find_skill(&node.name) {
                        if other_meta.skill_type == skill_meta.skill_type {
                            node.is_activated = Some(false);
                        }
                    }
                }
            }
        }
    }
    if let Some(node_state) = character_state.bl4_skill_trees.get_mut(tree_index).and_then(|tree| tree.nodes.get_mut(node_index)) {
        node_state.is_activated = Some(value);
    }
}
