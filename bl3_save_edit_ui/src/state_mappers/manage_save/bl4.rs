use std::collections::BTreeMap;

use iced::{button, scrollable, text_input};

use bl3_save_edit_core::{
    bl4::{self, Bl4EditState, Bl4InventoryItem, Bl4PointPools, Bl4SaveSummary, Bl4SkillNode, Bl4SkillTree},
    bl4_skill_table::BL4_SKILL_METADATA,
};

use crate::views::manage_save::character::{Bl4SkillNodeState, Bl4SkillTreeState};
use crate::views::manage_save::inventory::{Bl4InventoryDetailTab, Bl4InventoryEntry};
use crate::views::manage_save::vehicle::{
    vehicle_unlocker::VehicleUnlocker, Bl4UnlockableCategoryState,
};
use crate::views::manage_save::ManageSaveState;
use tracing::warn;

fn clamp_i64_to_i32(value: i64) -> i32 {
    if value > i64::from(i32::MAX) {
        i32::MAX
    } else if value < i64::from(i32::MIN) {
        i32::MIN
    } else {
        value as i32
    }
}

pub fn map_summary_to_states(state: &mut ManageSaveState, summary: &Bl4SaveSummary) {
    let general_state = &mut state.save_view_state.general_state;
    general_state.filename_input = summary.file_name.clone();
    general_state.guid_input = summary.char_guid.clone().unwrap_or_default();
    general_state.bl4_difficulty_input = summary.player_difficulty.clone().unwrap_or_default();
    general_state.bl4_tracked_missions_input = summary.tracked_missions.join(", ");

    if let Some(slot_digits) = summary
        .file_name
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse::<u32>()
        .ok()
    {
        general_state.slot_input = slot_digits.max(1);
    } else {
        general_state.slot_input = 1;
    }

    let character_state = &mut state.save_view_state.character_state;
    character_state.name_input = summary.char_name.clone().unwrap_or_default();
    character_state.bl4_class_selected = summary.class.clone();
    if let Some(level) = summary.character_level {
        character_state.level_input = level;
    }
    if let Some(experience) = summary.character_experience {
        character_state.experience_points_input = clamp_i64_to_i32(experience);
    }
    if let Some(tokens) = summary.point_pools.specialization_tokens {
        character_state.ability_points_input = clamp_i64_to_i32(tokens);
    } else if let Some(points) = summary.specialization_points {
        character_state.ability_points_input = clamp_i64_to_i32(points);
    } else if let Some(unspent) = summary.point_pools.character_progress {
        character_state.ability_points_input = clamp_i64_to_i32(unspent);
    }

    character_state.ammo_setter.set_bl4_mode(true);
    if let Some(value) = summary.ammo.get("sniper") {
        character_state.ammo_setter.sniper.set_bl4_origin(*value);
    }
    if let Some(value) = summary.ammo.get("heavy") {
        character_state.ammo_setter.heavy.set_bl4_origin(*value);
    }
    if let Some(value) = summary.ammo.get("shotgun") {
        character_state.ammo_setter.shotgun.set_bl4_origin(*value);
    }
    if let Some(value) = summary.ammo.get("grenade") {
        character_state.ammo_setter.grenade.set_bl4_origin(*value);
    }
    if let Some(value) = summary.ammo.get("smg") {
        character_state.ammo_setter.smg.set_bl4_origin(*value);
    }
    if let Some(value) = summary.ammo.get("assaultrifle") {
        character_state
            .ammo_setter
            .assault_rifle
            .set_bl4_origin(*value);
    }
    if let Some(value) = summary.ammo.get("pistol") {
        character_state.ammo_setter.pistol.set_bl4_origin(*value);
    }

    character_state.sdu_unlocker.backpack.input = summary.sdu_levels.backpack;
    character_state.sdu_unlocker.sniper.input = summary.sdu_levels.sniper;
    character_state.sdu_unlocker.heavy.input = summary.sdu_levels.heavy;
    character_state.sdu_unlocker.shotgun.input = summary.sdu_levels.shotgun;
    character_state.sdu_unlocker.grenade.input = summary.sdu_levels.grenade;
    character_state.sdu_unlocker.smg.input = summary.sdu_levels.smg;
    character_state.sdu_unlocker.assault_rifle.input = summary.sdu_levels.assault_rifle;
    character_state.sdu_unlocker.pistol.input = summary.sdu_levels.pistol;
    character_state.bl4_skill_tree_selected = None;
    character_state.bl4_body_input = summary.cosmetics.body.clone().unwrap_or_default();
    character_state.bl4_head_input = summary.cosmetics.head.clone().unwrap_or_default();
    character_state.bl4_skin_input = summary.cosmetics.skin.clone().unwrap_or_default();
    character_state.bl4_primary_color_input =
        summary.cosmetics.primary_color.clone().unwrap_or_default();
    character_state.bl4_secondary_color_input = summary
        .cosmetics
        .secondary_color
        .clone()
        .unwrap_or_default();
    character_state.bl4_tertiary_color_input =
        summary.cosmetics.tertiary_color.clone().unwrap_or_default();
    character_state.bl4_echo_body_input = summary.cosmetics.echo_body.clone().unwrap_or_default();
    character_state.bl4_echo_attachment_input = summary
        .cosmetics
        .echo_attachment
        .clone()
        .unwrap_or_default();
    character_state.bl4_echo_skin_input = summary.cosmetics.echo_skin.clone().unwrap_or_default();
    character_state.bl4_vehicle_skin_input =
        summary.cosmetics.vehicle_skin.clone().unwrap_or_default();
    character_state.bl4_unique_rewards_input = summary.unique_rewards.join(", ");
    character_state.bl4_equip_slots_input = summary
        .equip_slots_unlocked
        .iter()
        .map(|slot| slot.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    character_state.bl4_progress_character_input = summary
        .point_pools
        .character_progress
        .map(clamp_i64_to_i32)
        .unwrap_or(0);
    if character_state.bl4_progress_character_input <= 0 {
        if let Some(level) = summary.character_level {
            character_state.bl4_progress_character_input = (level - 1).max(0);
        }
    }
    character_state.bl4_progress_specialization_input = summary
        .point_pools
        .specialization_tokens
        .map(clamp_i64_to_i32)
        .unwrap_or(0);
    character_state.bl4_progress_echo_input = summary
        .point_pools
        .echo_tokens
        .map(clamp_i64_to_i32)
        .unwrap_or(0);

    character_state.bl4_skill_trees = summary
        .skill_trees
        .iter()
        .filter(|tree| tree.name != "sdu_upgrades")
        .map(|tree| Bl4SkillTreeState {
            name: tree.name.clone(),
            group_def_name: tree.group_def_name.clone(),
            nodes: tree
                .nodes
                .iter()
                .map(|node| Bl4SkillNodeState {
                    name: node.name.clone(),
                    points_spent: node.points_spent,
                    activation_level: node.activation_level,
                    is_activated: node.is_activated,
                    ..Bl4SkillNodeState::default()
                })
                .collect(),
        })
        .collect();
    if character_state.bl4_skill_points_mass_input == 0 {
        character_state.bl4_skill_points_mass_input = 5;
    }

    let currency_state = &mut state.save_view_state.currency_state;
    if let Some(cash) = summary.currencies.get("cash") {
        currency_state.money_input = clamp_i64_to_i32(*cash);
    }
    if let Some(eridium) = summary.currencies.get("eridium") {
        currency_state.eridium_input = clamp_i64_to_i32(*eridium);
    }

    let bl4_inventory_state = &mut state.save_view_state.inventory_state.bl4_inventory;
    bl4_inventory_state.entries = summary
        .inventory
        .iter()
        .map(Bl4InventoryEntry::from_inventory_item)
        .collect();
    bl4_inventory_state.list_button_states =
        vec![button::State::default(); bl4_inventory_state.entries.len()];
    bl4_inventory_state.duplicate_button_states =
        vec![button::State::default(); bl4_inventory_state.entries.len()];
    bl4_inventory_state.share_button_states =
        vec![button::State::default(); bl4_inventory_state.entries.len()];
    bl4_inventory_state.delete_button_states =
        vec![button::State::default(); bl4_inventory_state.entries.len()];
    bl4_inventory_state.selected_index = if bl4_inventory_state.entries.is_empty() {
        None
    } else {
        Some(0)
    };
    bl4_inventory_state.detail_scroll_state = scrollable::State::default();
    bl4_inventory_state.list_scroll_state = scrollable::State::default();
    bl4_inventory_state.detail_tab = Bl4InventoryDetailTab::Overview;
    bl4_inventory_state.detail_tab_button_states =
        vec![button::State::default(); Bl4InventoryDetailTab::all().len()];
    bl4_inventory_state.lootlemon_search.clear();
    bl4_inventory_state.lootlemon_results = bl4::artifact_catalog(60);
    bl4_inventory_state.lootlemon_import_button_states =
        vec![button::State::default(); bl4_inventory_state.lootlemon_results.len()];
    bl4_inventory_state.lootlemon_open_button_states =
        vec![button::State::default(); bl4_inventory_state.lootlemon_results.len()];
    bl4_inventory_state.search_input.clear();
    bl4_inventory_state.search_input_state = text_input::State::default();
    bl4_inventory_state.available_parts_search.clear();
    bl4_inventory_state.available_parts_search_state = text_input::State::default();

    let vehicle_state = &mut state.save_view_state.vehicle_state;
    vehicle_state.bl4_personal_vehicle_input = summary
        .vehicle_loadout
        .personal_vehicle
        .clone()
        .unwrap_or_default();
    vehicle_state.bl4_hover_drive_input = summary
        .vehicle_loadout
        .hover_drive
        .clone()
        .unwrap_or_default();
    vehicle_state.bl4_weapon_slot_input = summary
        .vehicle_loadout
        .vehicle_weapon_slot
        .map(|slot| slot.to_string())
        .unwrap_or_default();
    vehicle_state.bl4_vehicle_cosmetic_input = summary
        .vehicle_loadout
        .vehicle_cosmetic
        .clone()
        .unwrap_or_default();

    vehicle_state.unlocker = VehicleUnlocker::default();
    vehicle_state.bl4_unlockables.clear();
    for (category, entries) in &summary.unlockables {
        let mut state = Bl4UnlockableCategoryState::new();
        for entry in entries {
            state.known_entries.insert(entry.clone());
            state.entries.insert(entry.clone());
        }
        vehicle_state
            .bl4_unlockables
            .insert(category.clone(), state);
    }
}

pub fn build_edit_state(state: &ManageSaveState) -> Bl4EditState {
    let mut edits = Bl4EditState::default();

    let general_state = &state.save_view_state.general_state;
    if !general_state.guid_input.is_empty() {
        edits.char_guid = Some(general_state.guid_input.clone());
    }

    if !general_state.bl4_difficulty_input.trim().is_empty() {
        edits.player_difficulty = Some(general_state.bl4_difficulty_input.trim().to_string());
    }

    let character_state = &state.save_view_state.character_state;
    if !character_state.name_input.is_empty() {
        edits.char_name = Some(character_state.name_input.clone());
    }
    if let Some(class_value) = character_state.bl4_class_selected.clone() {
        if !class_value.trim().is_empty() {
            edits.class_name = Some(class_value.trim().to_string());
        }
    }
    edits.character_level = Some(character_state.level_input);
    edits.character_experience = Some(character_state.experience_points_input as i64);

    let ability_points_value = i64::from(character_state.ability_points_input);
    edits.point_pools.specialization_tokens = Some(ability_points_value);

    let mut tracked_mission_entries: Vec<String> = general_state
        .bl4_tracked_missions_input
        .split(',')
        .filter_map(|entry| {
            let trimmed = entry.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .collect();
    tracked_mission_entries.sort();
    tracked_mission_entries.dedup();

    let to_option = |value: &str| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    };

    edits.cosmetics.body = to_option(&character_state.bl4_body_input);
    edits.cosmetics.head = to_option(&character_state.bl4_head_input);
    edits.cosmetics.skin = to_option(&character_state.bl4_skin_input);
    edits.cosmetics.primary_color = to_option(&character_state.bl4_primary_color_input);
    edits.cosmetics.secondary_color = to_option(&character_state.bl4_secondary_color_input);
    edits.cosmetics.tertiary_color = to_option(&character_state.bl4_tertiary_color_input);
    edits.cosmetics.echo_body = to_option(&character_state.bl4_echo_body_input);
    edits.cosmetics.echo_attachment = to_option(&character_state.bl4_echo_attachment_input);
    edits.cosmetics.echo_skin = to_option(&character_state.bl4_echo_skin_input);
    edits.cosmetics.vehicle_skin = to_option(&character_state.bl4_vehicle_skin_input);

    let vehicle_state = &state.save_view_state.vehicle_state;

    let parsed_unique_rewards: Vec<String> = character_state
        .bl4_unique_rewards_input
        .split(',')
        .filter_map(|entry| {
            let trimmed = entry.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .collect();
    if let Some(file) = &state.bl4_file {
        let mut baseline = file.summary.unique_rewards.clone();
        baseline.sort();
        baseline.dedup();

        let mut desired = parsed_unique_rewards.clone();
        desired.sort();
        desired.dedup();

        if desired != baseline {
            edits.unique_rewards = desired;
            edits.unique_rewards_dirty = true;
        }
    } else if !parsed_unique_rewards.is_empty() {
        let mut desired = parsed_unique_rewards.clone();
        desired.sort();
        desired.dedup();
        edits.unique_rewards = desired;
        edits.unique_rewards_dirty = true;
    }

    if !character_state.bl4_equip_slots_input.trim().is_empty() {
        edits.equip_slots_unlocked = character_state
            .bl4_equip_slots_input
            .split(',')
            .filter_map(|entry| entry.trim().parse::<i32>().ok())
            .collect();
    }

    edits.vehicle_loadout.personal_vehicle = to_option(&vehicle_state.bl4_personal_vehicle_input);
    edits.vehicle_loadout.hover_drive = to_option(&vehicle_state.bl4_hover_drive_input);
    edits.vehicle_loadout.vehicle_cosmetic = to_option(&vehicle_state.bl4_vehicle_cosmetic_input);
    edits.vehicle_loadout.vehicle_weapon_slot = vehicle_state
        .bl4_weapon_slot_input
        .trim()
        .parse::<i32>()
        .ok();

    let mut currencies = BTreeMap::new();
    currencies.insert(
        "cash".to_string(),
        i64::from(state.save_view_state.currency_state.money_input),
    );
    currencies.insert(
        "eridium".to_string(),
        i64::from(state.save_view_state.currency_state.eridium_input),
    );
    edits.currencies = currencies;

    let bl4_mode = state.bl4_file.is_some();
    let ammo_setter = &character_state.ammo_setter;
    let mut ammo = BTreeMap::new();
    ammo.insert(
        "sniper".to_string(),
        if bl4_mode {
            ammo_setter.sniper.effective_level()
        } else {
            ammo_setter.sniper.input
        },
    );
    ammo.insert(
        "heavy".to_string(),
        if bl4_mode {
            ammo_setter.heavy.effective_level()
        } else {
            ammo_setter.heavy.input
        },
    );
    ammo.insert(
        "shotgun".to_string(),
        if bl4_mode {
            ammo_setter.shotgun.effective_level()
        } else {
            ammo_setter.shotgun.input
        },
    );
    ammo.insert(
        "grenade".to_string(),
        if bl4_mode {
            ammo_setter.grenade.effective_level()
        } else {
            ammo_setter.grenade.input
        },
    );
    ammo.insert(
        "smg".to_string(),
        if bl4_mode {
            ammo_setter.smg.effective_level()
        } else {
            ammo_setter.smg.input
        },
    );
    ammo.insert(
        "assaultrifle".to_string(),
        if bl4_mode {
            ammo_setter.assault_rifle.effective_level()
        } else {
            ammo_setter.assault_rifle.input
        },
    );
    ammo.insert(
        "pistol".to_string(),
        if bl4_mode {
            ammo_setter.pistol.effective_level()
        } else {
            ammo_setter.pistol.input
        },
    );
    edits.ammo = ammo;

    edits.sdu_levels.backpack = character_state.sdu_unlocker.backpack.input;
    edits.sdu_levels.sniper = character_state.sdu_unlocker.sniper.input;
    edits.sdu_levels.heavy = character_state.sdu_unlocker.heavy.input;
    edits.sdu_levels.shotgun = character_state.sdu_unlocker.shotgun.input;
    edits.sdu_levels.grenade = character_state.sdu_unlocker.grenade.input;
    edits.sdu_levels.smg = character_state.sdu_unlocker.smg.input;
    edits.sdu_levels.assault_rifle = character_state.sdu_unlocker.assault_rifle.input;
    edits.sdu_levels.pistol = character_state.sdu_unlocker.pistol.input;
    // Preserve non-exposed SDU slots (bank/lost_loot) from the currently loaded BL4 file
    // and compute a dirty flag when any SDU value changes vs baseline.
    if let Some(file) = &state.bl4_file {
        let base = &file.summary.sdu_levels;
        // carry forward bank and lost_loot so we don't unintentionally reset them
        edits.sdu_levels.bank = base.bank;
        edits.sdu_levels.lost_loot = base.lost_loot;

        let desired = &edits.sdu_levels;
        let changed = desired.backpack != base.backpack
            || desired.pistol != base.pistol
            || desired.smg != base.smg
            || desired.assault_rifle != base.assault_rifle
            || desired.shotgun != base.shotgun
            || desired.sniper != base.sniper
            || desired.heavy != base.heavy
            || desired.grenade != base.grenade
            || desired.bank != base.bank
            || desired.lost_loot != base.lost_loot;
        edits.sdu_levels_dirty = changed;
    } else {
        // No BL4 baseline available; mark dirty if any provided SDU value is non-zero
        let desired = &edits.sdu_levels;
        let any_non_zero = desired.backpack != 0
            || desired.pistol != 0
            || desired.smg != 0
            || desired.assault_rifle != 0
            || desired.shotgun != 0
            || desired.sniper != 0
            || desired.heavy != 0
            || desired.grenade != 0
            || desired.bank != 0
            || desired.lost_loot != 0;
        edits.sdu_levels_dirty = any_non_zero;
    }
    edits.point_pools.character_progress =
        Some(i64::from(character_state.bl4_progress_character_input));
    edits.point_pools.specialization_tokens =
        Some(i64::from(character_state.bl4_progress_specialization_input));
    edits.point_pools.echo_tokens = Some(i64::from(character_state.bl4_progress_echo_input));

    edits.inventory = state
        .save_view_state
        .inventory_state
        .bl4_inventory
        .entries
        .iter()
        .map(|entry| match entry.to_inventory_item() {
            Ok(item) => item,
            Err(err) => {
                warn!(
                    "Failed to encode BL4 inventory item {}: {}",
                    entry.slot, err
                );
                Bl4InventoryItem {
                    slot: entry.slot.clone(),
                    serial: entry.serial_input.clone(),
                    state_flags: entry.state_flags,
                }
            }
        })
        .collect();

    let mut unlockables_map: BTreeMap<String, Vec<String>> = state
        .save_view_state
        .vehicle_state
        .bl4_unlockables
        .iter()
        .map(|(category, state)| {
            let mut entries: Vec<String> = state.entries.iter().cloned().collect();
            entries.sort();
            (category.clone(), entries)
        })
        .collect();
    if let Some(file) = &state.bl4_file {
        if unlockables_map == file.summary.unlockables {
            unlockables_map.clear();
        }
    }
    edits.unlockables = unlockables_map;

    edits.skill_trees = state
        .save_view_state
        .character_state
        .bl4_skill_trees
        .iter()
        .map(|tree| Bl4SkillTree {
            name: tree.name.clone(),
            group_def_name: tree.group_def_name.clone(),
            nodes: tree
                .nodes
                .iter()
                .map(|node| Bl4SkillNode {
                    name: node.name.clone(),
                    points_spent: node.points_spent,
                    activation_level: node.activation_level,
                    is_activated: node.is_activated,
                })
                .collect(),
        })
        .collect();

    if let Some(file) = &state.bl4_file {
        edits.progression_in_state = file.summary.progression_in_state;
        edits.missions_in_state = file.summary.missions_in_state;
        edits.tracked_missions_need_none = file.summary.tracked_missions_need_none;
        if general_state
            .bl4_tracked_missions_input
            .trim()
            .is_empty()
        {
            edits.tracked_missions = file.summary.tracked_missions.clone();
            edits.tracked_missions_dirty = false;
        } else {
            edits.tracked_missions = tracked_mission_entries.clone();
            edits.tracked_missions_dirty = edits.tracked_missions != file.summary.tracked_missions;
        }
        if edits.unique_rewards.is_empty() {
            edits.unique_rewards = file.summary.unique_rewards.clone();
        }
        if edits.equip_slots_unlocked.is_empty() {
            edits.equip_slots_unlocked = file.summary.equip_slots_unlocked.clone();
        }
        if edits.cosmetics.body.is_none() {
            edits.cosmetics.body = file.summary.cosmetics.body.clone();
        }
        if edits.cosmetics.head.is_none() {
            edits.cosmetics.head = file.summary.cosmetics.head.clone();
        }
        if edits.cosmetics.skin.is_none() {
            edits.cosmetics.skin = file.summary.cosmetics.skin.clone();
        }
        if edits.cosmetics.primary_color.is_none() {
            edits.cosmetics.primary_color = file.summary.cosmetics.primary_color.clone();
        }
        if edits.cosmetics.secondary_color.is_none() {
            edits.cosmetics.secondary_color = file.summary.cosmetics.secondary_color.clone();
        }
        if edits.cosmetics.tertiary_color.is_none() {
            edits.cosmetics.tertiary_color = file.summary.cosmetics.tertiary_color.clone();
        }
        if edits.cosmetics.echo_body.is_none() {
            edits.cosmetics.echo_body = file.summary.cosmetics.echo_body.clone();
        }
        if edits.cosmetics.echo_attachment.is_none() {
            edits.cosmetics.echo_attachment = file.summary.cosmetics.echo_attachment.clone();
        }
        if edits.cosmetics.echo_skin.is_none() {
            edits.cosmetics.echo_skin = file.summary.cosmetics.echo_skin.clone();
        }
        if edits.cosmetics.vehicle_skin.is_none() {
            edits.cosmetics.vehicle_skin = file.summary.cosmetics.vehicle_skin.clone();
        }
        if edits.vehicle_loadout.personal_vehicle.is_none() {
            edits.vehicle_loadout.personal_vehicle =
                file.summary.vehicle_loadout.personal_vehicle.clone();
        }
        if edits.vehicle_loadout.hover_drive.is_none() {
            edits.vehicle_loadout.hover_drive = file.summary.vehicle_loadout.hover_drive.clone();
        }
        if edits.vehicle_loadout.vehicle_weapon_slot.is_none() {
            edits.vehicle_loadout.vehicle_weapon_slot =
                file.summary.vehicle_loadout.vehicle_weapon_slot;
        }
        if edits.vehicle_loadout.vehicle_cosmetic.is_none() {
            edits.vehicle_loadout.vehicle_cosmetic =
                file.summary.vehicle_loadout.vehicle_cosmetic.clone();
        }
        if edits.sdu_levels.bank == 0 {
            edits.sdu_levels.bank = file.summary.sdu_levels.bank;
        }
        if edits.sdu_levels.lost_loot == 0 {
            edits.sdu_levels.lost_loot = file.summary.sdu_levels.lost_loot;
        }
        edits.sdu_levels_dirty = edits.sdu_levels != file.summary.sdu_levels;
        if edits.specialization_level.is_none() {
            edits.specialization_level = file.summary.specialization_level;
        }
        if edits.specialization_points.is_none() {
            edits.specialization_points = file.summary.specialization_points;
        }
        if edits.point_pools == Bl4PointPools::default() {
            edits.point_pools = file.summary.point_pools.clone();
        }
        // If user pressed Clear Mission Progress in Character view
        if state
            .save_view_state
            .character_state
            .bl4_clear_missions_pressed
        {
            edits.tracked_missions = vec!["Mission_Main_Beach".to_string()];
            edits.tracked_missions_need_none = true;
            edits.tracked_missions_dirty = true;
            edits.missions = file
                .summary
                .missions
                .iter()
                .map(|m| bl4::Bl4MissionStatus {
                    set: m.set.clone(),
                    mission: m.mission.clone(),
                    status: None,
                })
                .collect();
        }
    } else {
        edits.progression_in_state = true;
        edits.missions_in_state = true;
        edits.tracked_missions_need_none = false;
        edits.tracked_missions = tracked_mission_entries.clone();
        edits.tracked_missions_dirty = !edits.tracked_missions.is_empty();
        edits.sdu_levels_dirty = true;
    }

    edits.tracked_missions.sort();
    edits.tracked_missions.dedup();
    edits.unique_rewards.sort();
    edits.unique_rewards.dedup();
    edits.equip_slots_unlocked.sort_unstable();
    edits.equip_slots_unlocked.dedup();

    edits
}
