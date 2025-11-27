use std::mem;
use std::path::{Path, PathBuf};

use iced::alignment::Horizontal;
use iced::{
    button, pick_list, svg, tooltip, Alignment, Application, Button, Color, Column, Command,
    Container, Element, Length, PickList, Row, Space, Svg, Text, Tooltip,
};
use tracing::{error, info};

use bl3_save_edit_core::bl3_profile::sdu::ProfileSduSlot;
use bl3_save_edit_core::bl3_profile::Bl3Profile;
use bl3_save_edit_core::bl3_save::ammo::AmmoPool;
use bl3_save_edit_core::bl3_save::sdu::SaveSduSlot;
use bl3_save_edit_core::bl3_save::util::{experience_to_level, REQUIRED_XP_LIST};
use bl3_save_edit_core::bl3_save::Bl3Save;
use bl3_save_edit_core::file_helper::Bl3FileType;
use bl3_save_edit_core::parser::HeaderType;

use crate::bl3_ui_style::{
    Bl3UiContentStyle, Bl3UiMenuBarStyle, Bl3UiPositiveButtonStyle, Bl3UiStyle, Bl3UiTooltipStyle,
};
use crate::bl4_ammo::bl4_totals_for_pool;
use crate::commands::{initialization, interaction};
use crate::config::{Bl3Config, ConfigMessage};
use crate::resources::fonts::{
    JETBRAINS_MONO, JETBRAINS_MONO_BOLD, JETBRAINS_MONO_NL_EXTRA_BOLD_ITALIC,
};
use crate::resources::svgs::{REFRESH, SETTINGS};
use crate::state_mappers::{manage_profile, manage_save};
use crate::update::Release;
use crate::util::{self, ErrorExt};
use crate::views::choose_save_directory::{
    ChooseSaveDirectoryState, ChooseSaveInteractionMessage, ChooseSaveMessage,
};
use crate::views::initialization::InitializationMessage;
use crate::views::item_editor::ItemEditorFileType;
use crate::views::manage_profile::bank::ProfileBankInteractionMessage;
use crate::views::manage_profile::general::ProfileGeneralInteractionMessage;
use crate::views::manage_profile::keys::ProfileKeysInteractionMessage;
use crate::views::manage_profile::main::{ProfileTabBarInteractionMessage, ProfileTabBarView};
use crate::views::manage_profile::profile::{
    GuardianRewardMessage, ProfileInteractionMessage, SduMessage, SkinUnlockedMessage,
};
use crate::views::manage_profile::{
    ManageProfileInteractionMessage, ManageProfileState, ManageProfileView,
};
use crate::views::manage_save::character::{
    bl4_handle_skill_points_change, bl4_handle_skill_toggle, bl4_metadata_for_state,
    tree_name_matches_metadata, CharacterAmmoMessage, CharacterGearUnlockedMessage, CharacterSduMessage,
    CharacterSkinSelectedMessage, SaveCharacterInteractionMessage,
};
use crate::views::manage_save::currency::SaveCurrencyInteractionMessage;
use crate::views::manage_save::general::SaveGeneralInteractionMessage;
use crate::views::manage_save::inventory::{
    Bl4InventoryDetailTab, Bl4InventoryEntry, SaveInventoryInteractionMessage,
};
use crate::views::manage_save::main::{SaveTabBarInteractionMessage, SaveTabBarView};
use crate::views::manage_save::vehicle::{
    Bl4UnlockableCategoryState, SaveVehicleInteractionMessage, VehicleUnlockedMessage,
};
use crate::views::manage_save::{ManageSaveInteractionMessage, ManageSaveState, ManageSaveView};
use crate::views::settings::{SettingsInteractionMessage, SettingsState};
use crate::views::InteractionExt;
use crate::widgets::notification::{Notification, NotificationSentiment};
use crate::{state_mappers, update, views, VERSION};
use bl3_save_edit_core::bl4::{self, Bl4LoadedSave, DecryptOutcome, EncryptOutcome};

#[derive(Debug, Default)]
pub struct Bl3Application {
    pub config: Bl3Config,
    pub view_state: ViewState,
    choose_save_directory_state: ChooseSaveDirectoryState,
    pub manage_save_state: ManageSaveState,
    pub manage_profile_state: ManageProfileState,
    loaded_files_selector: pick_list::State<Bl3FileType>,
    pub loaded_files_selected: Box<Bl3FileType>,
    loaded_files: Vec<Bl3FileType>,
    refresh_button_state: button::State,
    update_button_state: button::State,
    save_file_button_state: button::State,
    settings_top_button_state: button::State,
    notification: Option<Notification>,
    latest_release: Option<Release>,
    is_updating: bool,
    is_reloading_saves: bool,
    settings_state: SettingsState,
}

#[derive(Debug, Clone)]
pub enum Bl3Message {
    Initialization(InitializationMessage),
    LatestRelease(MessageResult<Release>),
    UpdateToLatestRelease,
    UpdateToLatestReleaseCompleted(MessageResult<()>),
    Config(ConfigMessage),
    Interaction(InteractionMessage),
    ChooseSave(ChooseSaveMessage),
    SaveFileCompleted(MessageResult<Bl3Save>),
    Bl4SaveCompleted(MessageResult<Bl4LoadedSave>),
    SaveProfileCompleted(MessageResult<Bl3Profile>),
    FilesLoadedAfterSave(MessageResult<(Bl3FileType, Vec<Bl3FileType>)>),
    ClearNotification,
}

#[derive(Debug, Clone)]
pub enum MessageResult<T> {
    Success(T),
    Error(String),
}

impl<T> MessageResult<T> {
    pub fn handle_result(result: anyhow::Result<T>) -> MessageResult<T> {
        match result {
            Ok(v) => MessageResult::Success(v),
            Err(e) => MessageResult::Error(e.to_string()),
        }
    }
}

impl ErrorExt for MessageResult<()> {
    fn handle_ui_error(&self, message: &str, notification: &mut Option<Notification>) {
        if let MessageResult::Error(e) = self {
            let message = format!("{}: {}.", message, e.to_string());

            error!("{}", message);

            *notification = Some(Notification::new(message, NotificationSentiment::Negative));
        }
    }
}

#[derive(Debug, Clone)]
pub enum InteractionMessage {
    ChooseSaveInteraction(ChooseSaveInteractionMessage),
    ManageSaveInteraction(ManageSaveInteractionMessage),
    ManageProfileInteraction(ManageProfileInteractionMessage),
    SettingsInteraction(SettingsInteractionMessage),
    LoadedFileSelected(Box<Bl3FileType>),
    RefreshSavesDirectory,
    Ignore,
}

#[derive(Debug, PartialEq)]
pub enum ViewState {
    Initializing,
    Loading,
    ChooseSaveDirectory,
    ManageSave(ManageSaveView),
    ManageProfile(ManageProfileView),
}

impl std::default::Default for ViewState {
    fn default() -> Self {
        Self::ChooseSaveDirectory
    }
}

impl Bl3Application {
    fn is_current_bl4(&self) -> bool {
        matches!(self.loaded_files_selected.as_ref(), Bl3FileType::Bl4Save(_))
            || self.manage_save_state.bl4_file.is_some()
    }

    fn bl4_points_from_level(level: i32) -> i32 {
        (level - 1).max(0)
    }
}

fn is_primary_skill_tree(name: &str) -> bool {
    if name.eq_ignore_ascii_case("sdu_upgrades") {
        return false;
    }
    if name.starts_with("ProgressGraph_Specializations") {
        return false;
    }
    true
}

impl Application for Bl3Application {
    type Executor = tokio::runtime::Runtime;
    type Message = Bl3Message;
    type Flags = Bl3Config;

    fn new(config: Self::Flags) -> (Self, Command<Self::Message>) {
        let startup_commands = [
            Command::perform(initialization::load_lazy_data(), |_| {
                Bl3Message::Initialization(InitializationMessage::LoadSaves)
            }),
            Command::perform(update::get_latest_release(), |r| {
                Bl3Message::LatestRelease(MessageResult::handle_result(r))
            }),
        ];

        let config_dir_input = config.config_dir().to_string_lossy().to_string();
        let saves_dir_input = config.saves_dir().to_string_lossy().to_string();
        let backup_dir_input = config.backup_dir().to_string_lossy().to_string();
        let ui_scale_factor = config.ui_scale_factor();
        let bl4_steamid = config
            .bl4_user_id()
            .map(|s| s.to_string())
            .unwrap_or_default();

        (
            Bl3Application {
                config,
                view_state: ViewState::Initializing,
                settings_state: SettingsState {
                    config_dir_input,
                    backup_dir_input,
                    saves_dir_input,
                    ui_scale_factor,
                    bl4_steamid,
                    ..SettingsState::default()
                },
                ..Bl3Application::default()
            },
            Command::batch(startup_commands),
        )
    }

    fn title(&self) -> String {
        if self.is_current_bl4() {
            format!("Borderlands 4 Save Editor - v{}", VERSION)
        } else {
            format!("Borderlands 3 Save Editor - v{}", VERSION)
        }
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match message {
            Bl3Message::Initialization(initialization_msg) => match initialization_msg {
                InitializationMessage::LoadSaves => {
                    if self.config.saves_dir().exists() {
                        return Command::perform(
                            interaction::choose_save_directory::load_files_in_directory(
                                self.config.saves_dir().to_path_buf(),
                                self.config.bl4_user_id().cloned(),
                            ),
                            |r| {
                                Bl3Message::ChooseSave(ChooseSaveMessage::FilesLoaded(
                                    MessageResult::handle_result(r),
                                ))
                            },
                        );
                    } else if *self.config.saves_dir() != PathBuf::default() {
                        let msg = "Failed to load your previously selected Save/Profile folder. Please select another folder.";

                        self.notification =
                            Some(Notification::new(msg, NotificationSentiment::Negative));
                    }

                    self.view_state = ViewState::ChooseSaveDirectory;
                }
            },
            Bl3Message::LatestRelease(res) => match res {
                MessageResult::Success(r) => {
                    self.latest_release = Some(r);
                }
                MessageResult::Error(e) => {
                    error!("Failed to get latest update: {}", e);
                }
            },
            Bl3Message::UpdateToLatestRelease => {
                if let Some(latest_release) = &self.latest_release {
                    self.is_updating = true;

                    return Command::perform(
                        update::download_release(latest_release.clone()),
                        |r| {
                            Bl3Message::UpdateToLatestReleaseCompleted(
                                MessageResult::handle_result(r),
                            )
                        },
                    );
                } else {
                    error!("Expected latest release to not be None when updating.");
                }
            }
            Bl3Message::UpdateToLatestReleaseCompleted(res) => {
                self.is_updating = false;

                match res {
                    MessageResult::Success(_) => {
                        info!("Successfully updated, exiting older version");

                        std::process::exit(0);
                    }
                    MessageResult::Error(e) => {
                        let msg = format!("Failed to update to latest release: {}.", e);

                        error!("{}", msg);

                        self.notification =
                            Some(Notification::new(msg, NotificationSentiment::Negative));
                    }
                }
            }
            Bl3Message::Config(config_msg) => match config_msg {
                ConfigMessage::SaveCompleted(res) => match res {
                    MessageResult::Success(_) => {
                        info!("Successfully saved config.");
                    }
                    MessageResult::Error(e) => error!("Failed to save config: {}", e),
                },
            },
            Bl3Message::Interaction(interaction_msg) => {
                self.notification = None;

                match interaction_msg {
                    InteractionMessage::ChooseSaveInteraction(choose_save_msg) => {
                        return match choose_save_msg {
                            ChooseSaveInteractionMessage::ChooseDirPressed => {
                                self.choose_save_directory_state.choose_dir_window_open = true;

                                Command::perform(
                                    interaction::choose_save_directory::choose(
                                        self.config.saves_dir().to_path_buf(),
                                    ),
                                    |r| {
                                        Bl3Message::ChooseSave(
                                            ChooseSaveMessage::ChooseDirCompleted(
                                                MessageResult::handle_result(r),
                                            ),
                                        )
                                    },
                                )
                            }
                        };
                    }
                    InteractionMessage::ManageSaveInteraction(manage_save_msg) => {
                        match manage_save_msg {
                            ManageSaveInteractionMessage::SetBl4ViewMode(mode) => {
                                self.manage_save_state.bl4_view_mode = mode;
                            }
                            ManageSaveInteractionMessage::TabBar(tab_bar_msg) => {
                                match tab_bar_msg {
                                    SaveTabBarInteractionMessage::General => {
                                        self.view_state = ViewState::ManageSave(
                                            ManageSaveView::TabBar(SaveTabBarView::General),
                                        )
                                    }
                                    SaveTabBarInteractionMessage::Character => {
                                        self.view_state = ViewState::ManageSave(
                                            ManageSaveView::TabBar(SaveTabBarView::Character),
                                        )
                                    }
                                    SaveTabBarInteractionMessage::Inventory => {
                                        self.view_state = ViewState::ManageSave(
                                            ManageSaveView::TabBar(SaveTabBarView::Inventory),
                                        )
                                    }
                                    SaveTabBarInteractionMessage::Currency => {
                                        self.view_state = ViewState::ManageSave(
                                            ManageSaveView::TabBar(SaveTabBarView::Currency),
                                        )
                                    }
                                    SaveTabBarInteractionMessage::Vehicle => {
                                        self.view_state = ViewState::ManageSave(
                                            ManageSaveView::TabBar(SaveTabBarView::Vehicle),
                                        )
                                    }
                                    SaveTabBarInteractionMessage::Settings => {
                                        self.view_state = ViewState::ManageSave(
                                            ManageSaveView::TabBar(SaveTabBarView::Settings),
                                        )
                                    }
                                }
                            }
                            ManageSaveInteractionMessage::General(general_msg) => match general_msg
                            {
                                SaveGeneralInteractionMessage::Guid(guid) => {
                                    self.manage_save_state
                                        .save_view_state
                                        .general_state
                                        .guid_input = guid;
                                }
                                SaveGeneralInteractionMessage::Slot(slot) => {
                                    let filename = format!("{:x}.sav", slot);

                                    self.manage_save_state
                                        .save_view_state
                                        .general_state
                                        .slot_input = slot;

                                    self.manage_save_state
                                        .save_view_state
                                        .general_state
                                        .filename_input = filename.clone();

                                    self.manage_save_state.current_file.file_name = filename;
                                    if let Some(bl4_file) = self.manage_save_state.bl4_file.as_mut()
                                    {
                                        bl4_file.summary.file_name = self
                                            .manage_save_state
                                            .save_view_state
                                            .general_state
                                            .filename_input
                                            .clone();
                                    }
                                }
                                SaveGeneralInteractionMessage::GenerateGuidPressed => {
                                    let guid =
                                        interaction::manage_save::general::generate_random_guid();

                                    self.manage_save_state
                                        .save_view_state
                                        .general_state
                                        .guid_input = guid;
                                }
                                SaveGeneralInteractionMessage::SaveTypeSelected(save_type) => {
                                    self.manage_save_state
                                        .save_view_state
                                        .general_state
                                        .save_type_selected = save_type;
                                }
                                SaveGeneralInteractionMessage::Bl4DifficultyChanged(difficulty) => {
                                    self.manage_save_state
                                        .save_view_state
                                        .general_state
                                        .bl4_difficulty_input = difficulty;
                                }
                                SaveGeneralInteractionMessage::Bl4TrackedMissionsChanged(
                                    tracked,
                                ) => {
                                    self.manage_save_state
                                        .save_view_state
                                        .general_state
                                        .bl4_tracked_missions_input = tracked;
                                }
                            },
                            ManageSaveInteractionMessage::Character(character_msg) => {
                                match character_msg {
                                    SaveCharacterInteractionMessage::Name(name_input) => {
                                        self.manage_save_state
                                            .save_view_state
                                            .character_state
                                            .name_input = name_input;
                                    }
                                    SaveCharacterInteractionMessage::Level(level) => {
                                        let xp_points = if level > 0 {
                                            REQUIRED_XP_LIST[level as usize - 1][0]
                                        } else {
                                            0
                                        };

                                        let character_state = &mut self
                                            .manage_save_state
                                            .save_view_state
                                            .character_state;

                                        character_state.level_input = level;

                                        character_state.experience_points_input = xp_points;
                                        character_state.bl4_progress_character_input =
                                            Bl3Application::bl4_points_from_level(level);
                                    }
                                    SaveCharacterInteractionMessage::ExperiencePoints(xp) => {
                                        let level = experience_to_level(xp as i32).unwrap_or(1);

                                        let character_state = &mut self
                                            .manage_save_state
                                            .save_view_state
                                            .character_state;

                                        character_state.experience_points_input = xp;

                                        character_state.level_input = level;
                                        character_state.bl4_progress_character_input =
                                            Bl3Application::bl4_points_from_level(level);
                                    }
                                    SaveCharacterInteractionMessage::AbilityPoints(points) => {
                                        let character_state = &mut self
                                            .manage_save_state
                                            .save_view_state
                                            .character_state;
                                        character_state.ability_points_input = points;
                                        character_state.bl4_progress_specialization_input = points;
                                    }
                                    SaveCharacterInteractionMessage::DetailTabChanged(tab) => {
                                        self.manage_save_state
                                            .save_view_state
                                            .character_state
                                            .detail_tab = tab;
                                    }
                                    SaveCharacterInteractionMessage::Bl4CosmeticBody(value) => {
                                        self.manage_save_state
                                            .save_view_state
                                            .character_state
                                            .bl4_body_input = value;
                                    }
                                    SaveCharacterInteractionMessage::Bl4CosmeticHead(value) => {
                                        self.manage_save_state
                                            .save_view_state
                                            .character_state
                                            .bl4_head_input = value;
                                    }
                                    SaveCharacterInteractionMessage::Bl4CosmeticSkin(value) => {
                                        self.manage_save_state
                                            .save_view_state
                                            .character_state
                                            .bl4_skin_input = value;
                                    }
                                    SaveCharacterInteractionMessage::Bl4PrimaryColor(value) => {
                                        self.manage_save_state
                                            .save_view_state
                                            .character_state
                                            .bl4_primary_color_input = value;
                                    }
                                    SaveCharacterInteractionMessage::Bl4SecondaryColor(value) => {
                                        self.manage_save_state
                                            .save_view_state
                                            .character_state
                                            .bl4_secondary_color_input = value;
                                    }
                                    SaveCharacterInteractionMessage::Bl4TertiaryColor(value) => {
                                        self.manage_save_state
                                            .save_view_state
                                            .character_state
                                            .bl4_tertiary_color_input = value;
                                    }
                                    SaveCharacterInteractionMessage::Bl4EchoBody(value) => {
                                        self.manage_save_state
                                            .save_view_state
                                            .character_state
                                            .bl4_echo_body_input = value;
                                    }
                                    SaveCharacterInteractionMessage::Bl4EchoAttachment(value) => {
                                        self.manage_save_state
                                            .save_view_state
                                            .character_state
                                            .bl4_echo_attachment_input = value;
                                    }
                                    SaveCharacterInteractionMessage::Bl4EchoSkin(value) => {
                                        self.manage_save_state
                                            .save_view_state
                                            .character_state
                                            .bl4_echo_skin_input = value;
                                    }
                                    SaveCharacterInteractionMessage::Bl4VehicleSkin(value) => {
                                        self.manage_save_state
                                            .save_view_state
                                            .character_state
                                            .bl4_vehicle_skin_input = value;
                                    }
                                    SaveCharacterInteractionMessage::Bl4EquipSlots(value) => {
                                        self.manage_save_state
                                            .save_view_state
                                            .character_state
                                            .bl4_equip_slots_input = value;
                                    }
                                    SaveCharacterInteractionMessage::Bl4UniqueRewards(value) => {
                                        self.manage_save_state
                                            .save_view_state
                                            .character_state
                                            .bl4_unique_rewards_input = value;
                                    }
                                    SaveCharacterInteractionMessage::Bl4ProgressCharacter(
                                        value,
                                    ) => {
                                        self.manage_save_state
                                            .save_view_state
                                            .character_state
                                            .bl4_progress_character_input = value;
                                    }
                                    SaveCharacterInteractionMessage::Bl4ProgressSpecialization(
                                        value,
                                    ) => {
                                        let character_state = &mut self
                                            .manage_save_state
                                            .save_view_state
                                            .character_state;
                                        character_state.bl4_progress_specialization_input = value;
                                        character_state.ability_points_input = value;
                                    }
                                    SaveCharacterInteractionMessage::Bl4ProgressEcho(value) => {
                                        self.manage_save_state
                                            .save_view_state
                                            .character_state
                                            .bl4_progress_echo_input = value;
                                    }
                                    SaveCharacterInteractionMessage::Bl4SkillMassInput(value) => {
                                        self.manage_save_state
                                            .save_view_state
                                            .character_state
                                            .bl4_skill_points_mass_input = value.max(0).min(999);
                                    }
                                    SaveCharacterInteractionMessage::Bl4SkillApplyAll => {
                                        let metadata = {
                                            let summary = self
                                                .manage_save_state
                                                .bl4_file
                                                .as_ref()
                                                .map(|file| &file.summary);
                                            bl4_metadata_for_state(
                                                &self.manage_save_state.save_view_state.character_state,
                                                summary,
                                            )
                                        };
                                        let character_state = &mut self
                                            .manage_save_state
                                            .save_view_state
                                            .character_state;
                                        let value =
                                            character_state.bl4_skill_points_mass_input.max(0);
                                        let selected_tree_name = character_state
                                            .bl4_action_tree_selected
                                            .clone()
                                            .or_else(|| metadata.and_then(|m| m.tree_names().first().cloned()));
                                        if let (Some(meta), Some(selected_tree_name)) = (metadata, selected_tree_name) {
                                            if let Some(tree_meta) = meta.tree_by_name(&selected_tree_name) {
                                                let matching_indices: Vec<usize> = character_state
                                                    .bl4_skill_trees
                                                    .iter()
                                                    .enumerate()
                                                    .filter_map(|(idx, tree_state)| {
                                                        let matches = tree_name_matches_metadata(
                                                            &tree_state.name,
                                                            &tree_meta.name,
                                                        ) || tree_state
                                                            .group_def_name
                                                            .as_deref()
                                                            .map(|name| tree_name_matches_metadata(name, &tree_meta.name))
                                                            .unwrap_or(false);
                                                        if matches { Some(idx) } else { None }
                                                    })
                                                    .collect();
                                                for tree_index in matching_indices {
                                                    let node_len =
                                                        character_state.bl4_skill_trees[tree_index].nodes.len();
                                                    for node_index in 0..node_len {
                                                        bl4_handle_skill_points_change(
                                                            character_state,
                                                            metadata,
                                                            tree_index,
                                                            node_index,
                                                            value,
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    SaveCharacterInteractionMessage::Bl4SkillMaxAll => {
                                        let metadata = {
                                            let summary = self
                                                .manage_save_state
                                                .bl4_file
                                                .as_ref()
                                                .map(|file| &file.summary);
                                            bl4_metadata_for_state(
                                                &self.manage_save_state.save_view_state.character_state,
                                                summary,
                                            )
                                        };
                                        let character_state = &mut self
                                            .manage_save_state
                                            .save_view_state
                                            .character_state;
                                        character_state.bl4_skill_points_mass_input = 5;
                                        let selected_tree_name = character_state
                                            .bl4_action_tree_selected
                                            .clone()
                                            .or_else(|| metadata.and_then(|m| m.tree_names().first().cloned()));
                                        if let (Some(meta), Some(selected_tree_name)) = (metadata, selected_tree_name) {
                                            if let Some(tree_meta) = meta.tree_by_name(&selected_tree_name) {
                                                // Collect matching indices first to avoid holding a mutable borrow across handler calls.
                                                let matching_indices: Vec<usize> = character_state
                                                    .bl4_skill_trees
                                                    .iter()
                                                    .enumerate()
                                                    .filter_map(|(idx, tree_state)| {
                                                        let matches = tree_name_matches_metadata(
                                                            &tree_state.name,
                                                            &tree_meta.name,
                                                        ) || tree_state
                                                            .group_def_name
                                                            .as_deref()
                                                            .map(|name| tree_name_matches_metadata(name, &tree_meta.name))
                                                            .unwrap_or(false);
                                                        if matches { Some(idx) } else { None }
                                                    })
                                                    .collect();
                                                for tree_index in matching_indices {
                                                    let node_len = character_state.bl4_skill_trees[tree_index].nodes.len();
                                                    for node_index in 0..node_len {
                                                        bl4_handle_skill_points_change(
                                                            character_state,
                                                            metadata,
                                                            tree_index,
                                                            node_index,
                                                            5,
                                                        );
                                                        bl4_handle_skill_toggle(
                                                            character_state,
                                                            metadata,
                                                            tree_index,
                                                            node_index,
                                                            true,
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    SaveCharacterInteractionMessage::Bl4SkillResetAll => {
                                        let metadata = {
                                            let summary = self
                                                .manage_save_state
                                                .bl4_file
                                                .as_ref()
                                                .map(|file| &file.summary);
                                            bl4_metadata_for_state(
                                                &self.manage_save_state.save_view_state.character_state,
                                                summary,
                                            )
                                        };
                                        let character_state = &mut self
                                            .manage_save_state
                                            .save_view_state
                                            .character_state;
                                        for tree_index in 0..character_state.bl4_skill_trees.len() {
                                            let node_len =
                                                character_state.bl4_skill_trees[tree_index].nodes.len();
                                            for node_index in 0..node_len {
                                                bl4_handle_skill_points_change(
                                                    character_state,
                                                    metadata,
                                                    tree_index,
                                                    node_index,
                                                    0,
                                                );
                                            }
                                        }
                                    }
                                    SaveCharacterInteractionMessage::Bl4ClearMissions => {
                                        let character_state = &mut self
                                            .manage_save_state
                                            .save_view_state
                                            .character_state;
                                        character_state.bl4_clear_missions_pressed = true;
                                        // Normalize tracked missions display
                                        self.manage_save_state
                                            .save_view_state
                                            .general_state
                                            .bl4_tracked_missions_input =
                                            "Mission_Main_Beach, none, none".to_string();
                                    }
                                    SaveCharacterInteractionMessage::Bl4ClassChanged(value) => {
                                        let character_state = &mut self
                                            .manage_save_state
                                            .save_view_state
                                            .character_state;
                                        character_state.bl4_class_selected =
                                            if value.is_empty() { None } else { Some(value) };
                                        character_state.bl4_skill_tree_selected = None;
                                        character_state.bl4_action_tree_selected = None;
                                    }
                                    SaveCharacterInteractionMessage::Bl4SkillTreeChanged(value) => {
                                        self.manage_save_state
                                            .save_view_state
                                            .character_state
                                            .bl4_skill_tree_selected =
                                            if value.is_empty() { None } else { Some(value) };
                                    }
                                    SaveCharacterInteractionMessage::Bl4ActionSkillTreeChanged(
                                        value,
                                    ) => {
                                        self.manage_save_state
                                            .save_view_state
                                            .character_state
                                            .bl4_action_tree_selected =
                                            if value.is_empty() { None } else { Some(value) };
                                    }
                                    SaveCharacterInteractionMessage::Bl4SkillNodePoints {
                                        tree_index,
                                        node_index,
                                        value,
                                    } => {
                                        let metadata = {
                                            let summary = self
                                                .manage_save_state
                                                .bl4_file
                                                .as_ref()
                                                .map(|file| &file.summary);
                                            bl4_metadata_for_state(
                                                &self.manage_save_state.save_view_state.character_state,
                                                summary,
                                            )
                                        };
                                        bl4_handle_skill_points_change(
                                            &mut self
                                                .manage_save_state
                                                .save_view_state
                                                .character_state,
                                            metadata,
                                            tree_index,
                                            node_index,
                                            value,
                                        );
                                    }
                                    SaveCharacterInteractionMessage::Bl4SkillNodeActivationLevel {
                                        tree_index,
                                        node_index,
                                        value,
                                    } => {
                                        if let Some(tree) = self
                                            .manage_save_state
                                            .save_view_state
                                            .character_state
                                            .bl4_skill_trees
                                            .get_mut(tree_index)
                                        {
                                            if let Some(node) = tree.nodes.get_mut(node_index) {
                                                node.activation_level = Some(value);
                                            }
                                        }
                                    }
                                    SaveCharacterInteractionMessage::Bl4SkillNodeToggle {
                                        tree_index,
                                        node_index,
                                        value,
                                    } => {
                                        let metadata = {
                                            let summary = self
                                                .manage_save_state
                                                .bl4_file
                                                .as_ref()
                                                .map(|file| &file.summary);
                                            bl4_metadata_for_state(
                                                &self.manage_save_state.save_view_state.character_state,
                                                summary,
                                            )
                                        };
                                        bl4_handle_skill_toggle(
                                            &mut self
                                                .manage_save_state
                                                .save_view_state
                                                .character_state,
                                            metadata,
                                            tree_index,
                                            node_index,
                                            value,
                                        );
                                    }
                                    SaveCharacterInteractionMessage::SduMessage(sdu_message) => {
                                        let sdu_unlocker = &mut self
                                            .manage_save_state
                                            .save_view_state
                                            .character_state
                                            .sdu_unlocker;

                                        match sdu_message {
                                            CharacterSduMessage::Backpack(level) => {
                                                sdu_unlocker.backpack.input = level;
                                            }
                                            CharacterSduMessage::Sniper(level) => {
                                                sdu_unlocker.sniper.input = level;
                                            }
                                            CharacterSduMessage::Shotgun(level) => {
                                                sdu_unlocker.shotgun.input = level;
                                            }
                                            CharacterSduMessage::Pistol(level) => {
                                                sdu_unlocker.pistol.input = level;
                                            }
                                            CharacterSduMessage::Grenade(level) => {
                                                sdu_unlocker.grenade.input = level;
                                            }
                                            CharacterSduMessage::Smg(level) => {
                                                sdu_unlocker.smg.input = level;
                                            }
                                            CharacterSduMessage::AssaultRifle(level) => {
                                                sdu_unlocker.assault_rifle.input = level;
                                            }
                                            CharacterSduMessage::Heavy(level) => {
                                                sdu_unlocker.heavy.input = level;
                                            }
                                        }
                                    }
                                    SaveCharacterInteractionMessage::MaxSduSlotsPressed => {
                                        let sdu_unlocker = &mut self
                                            .manage_save_state
                                            .save_view_state
                                            .character_state
                                            .sdu_unlocker;

                                        if self.manage_save_state.bl4_file.is_some() {
                                            sdu_unlocker.backpack.input = 8;
                                            sdu_unlocker.sniper.input = 7;
                                            sdu_unlocker.shotgun.input = 7;
                                            sdu_unlocker.pistol.input = 7;
                                            sdu_unlocker.grenade.input = 7;
                                            sdu_unlocker.smg.input = 7;
                                            sdu_unlocker.assault_rifle.input = 7;
                                            sdu_unlocker.heavy.input = 7;
                                        } else {
                                            sdu_unlocker.backpack.input =
                                                SaveSduSlot::Backpack.maximum();

                                            sdu_unlocker.sniper.input =
                                                SaveSduSlot::Sniper.maximum();

                                            sdu_unlocker.shotgun.input =
                                                SaveSduSlot::Shotgun.maximum();

                                            sdu_unlocker.pistol.input =
                                                SaveSduSlot::Pistol.maximum();

                                            sdu_unlocker.grenade.input =
                                                SaveSduSlot::Grenade.maximum();

                                            sdu_unlocker.smg.input = SaveSduSlot::Smg.maximum();

                                            sdu_unlocker.assault_rifle.input =
                                                SaveSduSlot::Ar.maximum();

                                            sdu_unlocker.heavy.input = SaveSduSlot::Heavy.maximum();
                                        }
                                    }
                                    SaveCharacterInteractionMessage::AmmoMessage(ammo_message) => {
                                        let ammo_setter = &mut self
                                            .manage_save_state
                                            .save_view_state
                                            .character_state
                                            .ammo_setter;

                                        match ammo_message {
                                            CharacterAmmoMessage::Sniper(amount) => {
                                                ammo_setter.sniper.input = amount;
                                            }
                                            CharacterAmmoMessage::Shotgun(amount) => {
                                                ammo_setter.shotgun.mark_dirty(amount);
                                            }
                                            CharacterAmmoMessage::Pistol(amount) => {
                                                ammo_setter.pistol.mark_dirty(amount);
                                            }
                                            CharacterAmmoMessage::Grenade(amount) => {
                                                ammo_setter.grenade.mark_dirty(amount);
                                            }
                                            CharacterAmmoMessage::Smg(amount) => {
                                                ammo_setter.smg.mark_dirty(amount);
                                            }
                                            CharacterAmmoMessage::AssaultRifle(amount) => {
                                                ammo_setter.assault_rifle.mark_dirty(amount);
                                            }
                                            CharacterAmmoMessage::Heavy(amount) => {
                                                ammo_setter.heavy.mark_dirty(amount);
                                            }
                                        }
                                    }
                                    SaveCharacterInteractionMessage::MaxAmmoAmountsPressed => {
                                        let ammo_setter = &mut self
                                            .manage_save_state
                                            .save_view_state
                                            .character_state
                                            .ammo_setter;

                                        if self.manage_save_state.bl4_file.is_some() {
                                            if let Some(max_total) =
                                                bl4_totals_for_pool(&ammo_setter.sniper.ammo_pool)
                                                    .and_then(|totals| totals.last().copied())
                                            {
                                                ammo_setter.sniper.mark_dirty(max_total);
                                            } else {
                                                ammo_setter
                                                    .sniper
                                                    .mark_dirty(AmmoPool::Sniper.maximum());
                                            }

                                            if let Some(max_total) =
                                                bl4_totals_for_pool(&ammo_setter.shotgun.ammo_pool)
                                                    .and_then(|totals| totals.last().copied())
                                            {
                                                ammo_setter.shotgun.mark_dirty(max_total);
                                            } else {
                                                ammo_setter
                                                    .shotgun
                                                    .mark_dirty(AmmoPool::Shotgun.maximum());
                                            }

                                            if let Some(max_total) =
                                                bl4_totals_for_pool(&ammo_setter.pistol.ammo_pool)
                                                    .and_then(|totals| totals.last().copied())
                                            {
                                                ammo_setter.pistol.mark_dirty(max_total);
                                            } else {
                                                ammo_setter
                                                    .pistol
                                                    .mark_dirty(AmmoPool::Pistol.maximum());
                                            }

                                            if let Some(max_total) =
                                                bl4_totals_for_pool(&ammo_setter.grenade.ammo_pool)
                                                    .and_then(|totals| totals.last().copied())
                                            {
                                                ammo_setter.grenade.mark_dirty(max_total);
                                            } else {
                                                ammo_setter
                                                    .grenade
                                                    .mark_dirty(AmmoPool::Grenade.maximum());
                                            }

                                            if let Some(max_total) =
                                                bl4_totals_for_pool(&ammo_setter.smg.ammo_pool)
                                                    .and_then(|totals| totals.last().copied())
                                            {
                                                ammo_setter.smg.mark_dirty(max_total);
                                            } else {
                                                ammo_setter.smg.mark_dirty(AmmoPool::Smg.maximum());
                                            }

                                            if let Some(max_total) = bl4_totals_for_pool(
                                                &ammo_setter.assault_rifle.ammo_pool,
                                            )
                                            .and_then(|totals| totals.last().copied())
                                            {
                                                ammo_setter.assault_rifle.mark_dirty(max_total);
                                            } else {
                                                ammo_setter
                                                    .assault_rifle
                                                    .mark_dirty(AmmoPool::Ar.maximum());
                                            }

                                            if let Some(max_total) =
                                                bl4_totals_for_pool(&ammo_setter.heavy.ammo_pool)
                                                    .and_then(|totals| totals.last().copied())
                                            {
                                                ammo_setter.heavy.mark_dirty(max_total);
                                            } else {
                                                ammo_setter
                                                    .heavy
                                                    .mark_dirty(AmmoPool::Heavy.maximum());
                                            }
                                        } else {
                                            ammo_setter.sniper.input = AmmoPool::Sniper.maximum();

                                            ammo_setter.shotgun.input = AmmoPool::Shotgun.maximum();

                                            ammo_setter.pistol.input = AmmoPool::Pistol.maximum();

                                            ammo_setter.grenade.input = AmmoPool::Grenade.maximum();

                                            ammo_setter.smg.input = AmmoPool::Smg.maximum();

                                            ammo_setter.assault_rifle.input =
                                                AmmoPool::Ar.maximum();

                                            ammo_setter.heavy.input = AmmoPool::Heavy.maximum();
                                        }
                                    }
                                    SaveCharacterInteractionMessage::PlayerClassSelected(
                                        player_class,
                                    ) => {
                                        self.manage_save_state
                                            .save_view_state
                                            .character_state
                                            .player_class_selected_class = player_class;
                                    }
                                    SaveCharacterInteractionMessage::SkinMessage(skin_message) => {
                                        let skin_selectors = &mut self
                                            .manage_save_state
                                            .save_view_state
                                            .character_state
                                            .skin_selectors;

                                        match skin_message {
                                            CharacterSkinSelectedMessage::HeadSkin(selected) => {
                                                skin_selectors.head_skin.selected = selected;
                                            }
                                            CharacterSkinSelectedMessage::CharacterSkin(
                                                selected,
                                            ) => {
                                                skin_selectors.character_skin.selected = selected;
                                            }
                                            CharacterSkinSelectedMessage::EchoTheme(selected) => {
                                                skin_selectors.echo_theme.selected = selected;
                                            }
                                        }
                                    }
                                    SaveCharacterInteractionMessage::GearMessage(gear_msg) => {
                                        let gear_unlocker = &mut self
                                            .manage_save_state
                                            .save_view_state
                                            .character_state
                                            .gear_unlocker;

                                        match gear_msg {
                                            CharacterGearUnlockedMessage::Grenade(b) => {
                                                gear_unlocker.grenade.is_unlocked = b;
                                            }
                                            CharacterGearUnlockedMessage::Shield(b) => {
                                                gear_unlocker.shield.is_unlocked = b;
                                            }
                                            CharacterGearUnlockedMessage::Weapon1(b) => {
                                                gear_unlocker.weapon_1.is_unlocked = b;
                                            }
                                            CharacterGearUnlockedMessage::Weapon2(b) => {
                                                gear_unlocker.weapon_2.is_unlocked = b;
                                            }
                                            CharacterGearUnlockedMessage::Weapon3(b) => {
                                                gear_unlocker.weapon_3.is_unlocked = b;
                                            }
                                            CharacterGearUnlockedMessage::Weapon4(b) => {
                                                gear_unlocker.weapon_4.is_unlocked = b;
                                            }
                                            CharacterGearUnlockedMessage::Artifact(b) => {
                                                gear_unlocker.artifact.is_unlocked = b;
                                            }
                                            CharacterGearUnlockedMessage::ClassMod(b) => {
                                                gear_unlocker.class_mod.is_unlocked = b;
                                            }
                                        }
                                    }
                                }
                            }
                            ManageSaveInteractionMessage::Inventory(inventory_msg) => {
                                match inventory_msg {
                                    SaveInventoryInteractionMessage::Editor(
                                        item_editor_message,
                                    ) => {
                                        let res = item_editor_message.update_state(
                                            &mut self
                                                .manage_save_state
                                                .save_view_state
                                                .inventory_state
                                                .item_editor_state,
                                            ItemEditorFileType::Save(
                                                &mut self.manage_save_state.current_file,
                                            ),
                                        );

                                        self.notification = res.notification;

                                        if let Some(command) = res.command {
                                            return command.map(|m| {
                                                Bl3Message::Interaction(
                                                    InteractionMessage::ManageSaveInteraction(
                                                        ManageSaveInteractionMessage::Inventory(
                                                            SaveInventoryInteractionMessage::Editor(
                                                                m,
                                                            ),
                                                        ),
                                                    ),
                                                )
                                            });
                                        }
                                    }
                                    SaveInventoryInteractionMessage::Bl4SerialChanged(
                                        index,
                                        value,
                                    ) => {
                                        if let Some(entry) = self
                                            .manage_save_state
                                            .save_view_state
                                            .inventory_state
                                            .bl4_inventory
                                            .entries
                                            .get_mut(index)
                                        {
                                            entry.on_serial_changed(value);
                                        }
                                    }
                                    SaveInventoryInteractionMessage::Bl4AddItem => {
                                        let inventory_state = &mut self
                                            .manage_save_state
                                            .save_view_state
                                            .inventory_state
                                            .bl4_inventory;
                                        let next_index = inventory_state
                                            .entries
                                            .iter()
                                            .filter_map(|entry| {
                                                entry
                                                    .slot
                                                    .strip_prefix("slot_")
                                                    .and_then(|s| s.parse::<usize>().ok())
                                            })
                                            .max()
                                            .map(|max| max + 1)
                                            .unwrap_or(inventory_state.entries.len());
                                        let slot_name = format!("slot_{}", next_index);
                                        inventory_state
                                            .entries
                                            .push(Bl4InventoryEntry::empty(slot_name));
                                        inventory_state
                                            .list_button_states
                                            .push(button::State::default());
                                        inventory_state
                                            .duplicate_button_states
                                            .push(button::State::default());
                                        inventory_state
                                            .share_button_states
                                            .push(button::State::default());
                                        inventory_state
                                            .delete_button_states
                                            .push(button::State::default());
                                        inventory_state.selected_index =
                                            Some(inventory_state.entries.len().saturating_sub(1));
                                        inventory_state.detail_scroll_state = Default::default();
                                    }
                                    SaveInventoryInteractionMessage::Bl4RemoveItem(index) => {
                                        let inventory_state = &mut self
                                            .manage_save_state
                                            .save_view_state
                                            .inventory_state
                                            .bl4_inventory;
                                        if index < inventory_state.entries.len() {
                                            let previous_selection = inventory_state.selected_index;
                                            inventory_state.entries.remove(index);
                                            if index < inventory_state.list_button_states.len() {
                                                inventory_state.list_button_states.remove(index);
                                            }
                                            if index < inventory_state.duplicate_button_states.len()
                                            {
                                                inventory_state
                                                    .duplicate_button_states
                                                    .remove(index);
                                            }
                                            if index < inventory_state.share_button_states.len() {
                                                inventory_state.share_button_states.remove(index);
                                            }
                                            if index < inventory_state.delete_button_states.len() {
                                                inventory_state.delete_button_states.remove(index);
                                            }
                                            if inventory_state.entries.is_empty() {
                                                inventory_state.selected_index = None;
                                            } else {
                                                let candidate = previous_selection.map(|sel| {
                                                    if sel == index {
                                                        sel.saturating_sub(1)
                                                    } else if sel > index {
                                                        sel - 1
                                                    } else {
                                                        sel
                                                    }
                                                });
                                                let capped = candidate
                                                    .unwrap_or(0)
                                                    .min(inventory_state.entries.len() - 1);
                                                inventory_state.selected_index = Some(capped);
                                            }
                                            inventory_state.detail_scroll_state =
                                                Default::default();
                                        }
                                    }
                                    SaveInventoryInteractionMessage::Bl4SelectItem(index) => {
                                        let inventory_state = &mut self
                                            .manage_save_state
                                            .save_view_state
                                            .inventory_state
                                            .bl4_inventory;
                                        if index < inventory_state.entries.len() {
                                            inventory_state.selected_index = Some(index);
                                            inventory_state.detail_scroll_state =
                                                Default::default();
                                            inventory_state.detail_tab =
                                                Bl4InventoryDetailTab::Overview;
                                        }
                                    }
                                    SaveInventoryInteractionMessage::Bl4LevelChanged(
                                        index,
                                        value,
                                    ) => {
                                        if let Some(entry) = self
                                            .manage_save_state
                                            .save_view_state
                                            .inventory_state
                                            .bl4_inventory
                                            .entries
                                            .get_mut(index)
                                        {
                                            entry.on_level_changed(value);
                                        }
                                    }
                                    SaveInventoryInteractionMessage::Bl4ManufacturerChanged(
                                        index,
                                        value,
                                    ) => {
                                        if let Some(entry) = self
                                            .manage_save_state
                                            .save_view_state
                                            .inventory_state
                                            .bl4_inventory
                                            .entries
                                            .get_mut(index)
                                        {
                                            entry.on_manufacturer_changed(value);
                                        }
                                    }
                                    SaveInventoryInteractionMessage::Bl4ItemTypeSelected(
                                        index,
                                        choice,
                                    ) => {
                                        if let Some(entry) = self
                                            .manage_save_state
                                            .save_view_state
                                            .inventory_state
                                            .bl4_inventory
                                            .entries
                                            .get_mut(index)
                                        {
                                            entry.on_item_type_selected(choice);
                                        }
                                    }
                                    SaveInventoryInteractionMessage::Bl4StateFlagToggled(
                                        index,
                                        flag,
                                        enabled,
                                    ) => {
                                        if let Some(entry) = self
                                            .manage_save_state
                                            .save_view_state
                                            .inventory_state
                                            .bl4_inventory
                                            .entries
                                            .get_mut(index)
                                        {
                                            entry.toggle_state_flag(flag, enabled);
                                        }
                                    }
                                    SaveInventoryInteractionMessage::Bl4PartIndexChanged(
                                        entry_index,
                                        part_index,
                                        value,
                                    ) => {
                                        if let Some(entry) = self
                                            .manage_save_state
                                            .save_view_state
                                            .inventory_state
                                            .bl4_inventory
                                            .entries
                                            .get_mut(entry_index)
                                        {
                                            entry.on_part_index_changed(part_index, value);
                                        }
                                    }
                                    SaveInventoryInteractionMessage::Bl4PartOptionSelected(
                                        entry_index,
                                        part_index,
                                        choice,
                                    ) => {
                                        if let Some(entry) = self
                                            .manage_save_state
                                            .save_view_state
                                            .inventory_state
                                            .bl4_inventory
                                            .entries
                                            .get_mut(entry_index)
                                        {
                                            entry.on_part_option_selected(part_index, choice);
                                        }
                                    }
                                    SaveInventoryInteractionMessage::Bl4PartValuesChanged(
                                        entry_index,
                                        part_index,
                                        value,
                                    ) => {
                                        if let Some(entry) = self
                                            .manage_save_state
                                            .save_view_state
                                            .inventory_state
                                            .bl4_inventory
                                            .entries
                                            .get_mut(entry_index)
                                        {
                                            entry.on_part_values_changed(part_index, value);
                                        }
                                    }
                                    SaveInventoryInteractionMessage::Bl4RemovePart(
                                        entry_index,
                                        part_index,
                                    ) => {
                                        if let Some(entry) = self
                                            .manage_save_state
                                            .save_view_state
                                            .inventory_state
                                            .bl4_inventory
                                            .entries
                                            .get_mut(entry_index)
                                        {
                                            entry.remove_part(part_index);
                                        }
                                    }
                                    SaveInventoryInteractionMessage::Bl4ApplyAvailablePart(
                                        entry_index,
                                        choice,
                                    ) => {
                                        if let Some(entry) = self
                                            .manage_save_state
                                            .save_view_state
                                            .inventory_state
                                            .bl4_inventory
                                            .entries
                                            .get_mut(entry_index)
                                        {
                                            entry.apply_available_part(&choice);
                                        }
                                    }
                                    SaveInventoryInteractionMessage::Bl4AvailablePartsSearch(
                                        query,
                                    ) => {
                                        self.manage_save_state
                                            .save_view_state
                                            .inventory_state
                                            .bl4_inventory
                                            .available_parts_search = query;
                                    }
                                    SaveInventoryInteractionMessage::Bl4SelectDetailTab(tab) => {
                                        let inventory_state = &mut self
                                            .manage_save_state
                                            .save_view_state
                                            .inventory_state
                                            .bl4_inventory;
                                        inventory_state.detail_tab = tab;
                                        inventory_state.detail_scroll_state = Default::default();
                                    }
                                    SaveInventoryInteractionMessage::Bl4SearchChanged(query) => {
                                        self.manage_save_state
                                            .save_view_state
                                            .inventory_state
                                            .bl4_inventory
                                            .search_input = query;
                                    }
                                    SaveInventoryInteractionMessage::Bl4DuplicateItem(index) => {
                                        let inventory_state = &mut self
                                            .manage_save_state
                                            .save_view_state
                                            .inventory_state
                                            .bl4_inventory;
                                        if index < inventory_state.entries.len() {
                                            let base_slot =
                                                inventory_state.entries[index].slot.clone();
                                            let base_serial =
                                                inventory_state.entries[index].serial_input.clone();
                                            let state_flags =
                                                inventory_state.entries[index].state_flags;

                                            let mut candidate = format!("{}_copy", base_slot);
                                            let mut counter = 1;
                                            while inventory_state
                                                .entries
                                                .iter()
                                                .any(|entry| entry.slot == candidate)
                                            {
                                                counter += 1;
                                                candidate =
                                                    format!("{}_copy{}", base_slot, counter);
                                            }

                                            let new_entry = Bl4InventoryEntry::new(
                                                candidate,
                                                base_serial,
                                                state_flags,
                                            );
                                            inventory_state.entries.insert(index + 1, new_entry);
                                            inventory_state
                                                .list_button_states
                                                .insert(index + 1, button::State::default());
                                            inventory_state
                                                .duplicate_button_states
                                                .insert(index + 1, button::State::default());
                                            inventory_state
                                                .share_button_states
                                                .insert(index + 1, button::State::default());
                                            inventory_state
                                                .delete_button_states
                                                .insert(index + 1, button::State::default());
                                            inventory_state.selected_index = Some(index + 1);
                                            inventory_state.detail_scroll_state =
                                                Default::default();
                                        }
                                    }
                                    SaveInventoryInteractionMessage::Bl4ShareItem(index) => {
                                        let inventory_state = &mut self
                                            .manage_save_state
                                            .save_view_state
                                            .inventory_state
                                            .bl4_inventory;
                                        if let Some(entry) = inventory_state.entries.get(index) {
                                            match util::set_clipboard_contents(
                                                entry.serial_input.clone(),
                                            ) {
                                                Ok(_) => {
                                                    self.notification = Some(Notification::new(
                                                        "Serial copied to clipboard.".to_string(),
                                                        NotificationSentiment::Positive,
                                                    ));
                                                }
                                                Err(e) => {
                                                    let msg = format!(
                                                        "Failed to copy serial to clipboard: {}",
                                                        e
                                                    );
                                                    error!("{}", msg);
                                                    self.notification = Some(Notification::new(
                                                        msg,
                                                        NotificationSentiment::Negative,
                                                    ));
                                                }
                                            }
                                        }
                                    }
                                    SaveInventoryInteractionMessage::Bl4LootlemonSearchChanged(
                                        query,
                                    ) => {
                                        let inventory_state = &mut self
                                            .manage_save_state
                                            .save_view_state
                                            .inventory_state
                                            .bl4_inventory;
                                        inventory_state.lootlemon_search = query;
                                        let results = bl4::search_artifact_catalog(
                                            &inventory_state.lootlemon_search,
                                            60,
                                        );
                                        inventory_state.lootlemon_results = results;
                                        inventory_state.lootlemon_import_button_states = vec![
                                            button::State::default();
                                            inventory_state.lootlemon_results.len()
                                        ];
                                        inventory_state.lootlemon_open_button_states = vec![
                                            button::State::default();
                                            inventory_state.lootlemon_results.len()
                                        ];
                                    }
                                    SaveInventoryInteractionMessage::Bl4LootlemonImport(index) => {
                                        let inventory_state = &mut self
                                            .manage_save_state
                                            .save_view_state
                                            .inventory_state
                                            .bl4_inventory;
                                        if let (Some(selected_idx), Some(catalog)) = (
                                            inventory_state.selected_index,
                                            inventory_state.lootlemon_results.get(index),
                                        ) {
                                            if let Some(entry) =
                                                inventory_state.entries.get_mut(selected_idx)
                                            {
                                                entry.apply_catalog_item(catalog.clone());
                                                inventory_state.detail_scroll_state =
                                                    Default::default();
                                            }
                                        }
                                    }
                                    SaveInventoryInteractionMessage::Bl4LootlemonOpen(index) => {
                                        let inventory_state = &mut self
                                            .manage_save_state
                                            .save_view_state
                                            .inventory_state
                                            .bl4_inventory;
                                        if let Some(catalog) =
                                            inventory_state.lootlemon_results.get(index)
                                        {
                                            let query = catalog.label().replace(' ', "+");
                                            let url =
                                                format!("https://lootlemon.com/search?q={query}");
                                            return Command::perform(
                                                interaction::manage_save::item_editor::open_website(
                                                    url,
                                                ),
                                                |_| {
                                                    Bl3Message::Interaction(
                                                        InteractionMessage::Ignore,
                                                    )
                                                },
                                            );
                                        }
                                    }
                                }
                            }
                            ManageSaveInteractionMessage::Currency(currency_msg) => {
                                match currency_msg {
                                    SaveCurrencyInteractionMessage::Money(money) => {
                                        self.manage_save_state
                                            .save_view_state
                                            .currency_state
                                            .money_input = money;
                                    }
                                    SaveCurrencyInteractionMessage::Eridium(eridium) => {
                                        self.manage_save_state
                                            .save_view_state
                                            .currency_state
                                            .eridium_input = eridium;
                                    }
                                    SaveCurrencyInteractionMessage::MaxMoneyPressed => {
                                        self.manage_save_state
                                            .save_view_state
                                            .currency_state
                                            .money_input = i32::MAX;
                                    }
                                    SaveCurrencyInteractionMessage::MaxEridiumPressed => {
                                        self.manage_save_state
                                            .save_view_state
                                            .currency_state
                                            .eridium_input = i32::MAX;
                                    }
                                }
                            }
                            ManageSaveInteractionMessage::Vehicle(vehicle_msg) => match vehicle_msg
                            {
                                SaveVehicleInteractionMessage::UnlockMessage(unlock_msg) => {
                                    let vehicle_unlocker = &mut self
                                        .manage_save_state
                                        .save_view_state
                                        .vehicle_state
                                        .unlocker;

                                    match unlock_msg {
                                        VehicleUnlockedMessage::OutrunnerChassis(selected) => {
                                            vehicle_unlocker.outrunner_chassis.is_unlocked =
                                                selected;
                                        }
                                        VehicleUnlockedMessage::OutrunnerParts(selected) => {
                                            vehicle_unlocker.outrunner_parts.is_unlocked = selected;
                                        }
                                        VehicleUnlockedMessage::OutrunnerSkins(selected) => {
                                            vehicle_unlocker.outrunner_skins.is_unlocked = selected;
                                        }
                                        VehicleUnlockedMessage::JetbeastChassis(selected) => {
                                            vehicle_unlocker.jetbeast_chassis.is_unlocked =
                                                selected;
                                        }
                                        VehicleUnlockedMessage::JetbeastParts(selected) => {
                                            vehicle_unlocker.jetbeast_parts.is_unlocked = selected;
                                        }
                                        VehicleUnlockedMessage::JetbeastSkins(selected) => {
                                            vehicle_unlocker.jetbeast_skins.is_unlocked = selected;
                                        }
                                        VehicleUnlockedMessage::TechnicalChassis(selected) => {
                                            vehicle_unlocker.technical_chassis.is_unlocked =
                                                selected;
                                        }
                                        VehicleUnlockedMessage::TechnicalParts(selected) => {
                                            vehicle_unlocker.technical_parts.is_unlocked = selected;
                                        }
                                        VehicleUnlockedMessage::TechnicalSkins(selected) => {
                                            vehicle_unlocker.technical_skins.is_unlocked = selected;
                                        }
                                        VehicleUnlockedMessage::CycloneChassis(selected) => {
                                            vehicle_unlocker.cyclone_chassis.is_unlocked = selected;
                                        }
                                        VehicleUnlockedMessage::CycloneParts(selected) => {
                                            vehicle_unlocker.cyclone_parts.is_unlocked = selected;
                                        }
                                        VehicleUnlockedMessage::CycloneSkins(selected) => {
                                            vehicle_unlocker.cyclone_skins.is_unlocked = selected;
                                        }
                                    }
                                }
                                SaveVehicleInteractionMessage::Bl4PersonalVehicle(value) => {
                                    self.manage_save_state
                                        .save_view_state
                                        .vehicle_state
                                        .bl4_personal_vehicle_input = value;
                                }
                                SaveVehicleInteractionMessage::Bl4HoverDrive(value) => {
                                    self.manage_save_state
                                        .save_view_state
                                        .vehicle_state
                                        .bl4_hover_drive_input = value;
                                }
                                SaveVehicleInteractionMessage::Bl4VehicleWeaponSlot(value) => {
                                    self.manage_save_state
                                        .save_view_state
                                        .vehicle_state
                                        .bl4_weapon_slot_input = value;
                                }
                                SaveVehicleInteractionMessage::Bl4VehicleCosmetic(value) => {
                                    self.manage_save_state
                                        .save_view_state
                                        .vehicle_state
                                        .bl4_vehicle_cosmetic_input = value;
                                }
                                SaveVehicleInteractionMessage::Bl4UnlockableToggle {
                                    category,
                                    entry,
                                    enabled,
                                } => {
                                    let category_state = self
                                        .manage_save_state
                                        .save_view_state
                                        .vehicle_state
                                        .bl4_unlockables
                                        .entry(category.clone())
                                        .or_insert_with(Bl4UnlockableCategoryState::default);
                                    if enabled {
                                        category_state.entries.insert(entry.clone());
                                    } else {
                                        category_state.entries.remove(&entry);
                                    }
                                    category_state.known_entries.insert(entry);
                                }
                                SaveVehicleInteractionMessage::Bl4UnlockableToggleAll {
                                    category,
                                    enable,
                                } => {
                                    let category_state = self
                                        .manage_save_state
                                        .save_view_state
                                        .vehicle_state
                                        .bl4_unlockables
                                        .entry(category.clone())
                                        .or_insert_with(Bl4UnlockableCategoryState::default);
                                    if enable {
                                        if category_state.known_entries.is_empty() {
                                            // nothing new to add; keep current entries as-is
                                        } else {
                                            category_state.entries =
                                                category_state.known_entries.clone();
                                        }
                                    } else {
                                        category_state.entries.clear();
                                    }
                                }
                            },
                            ManageSaveInteractionMessage::SaveFilePressed => {
                                if self.manage_save_state.bl4_file.is_some() {
                                    let Some(steamid) = self.config.bl4_user_id() else {
                                        let msg = "Cannot save BL4 file: configure a Borderlands 4 user ID in Settings.";
                                        error!("{}", msg);
                                        self.notification = Some(Notification::new(
                                            msg.to_string(),
                                            NotificationSentiment::Negative,
                                        ));
                                        return Command::none();
                                    };

                                    let edits =
                                        manage_save::bl4::build_edit_state(&self.manage_save_state);

                                    let current_bl4 =
                                        self.manage_save_state.bl4_file.clone().unwrap();
                                    let mut working = current_bl4;
                                    working.summary.file_name = self
                                        .manage_save_state
                                        .save_view_state
                                        .general_state
                                        .filename_input
                                        .clone();

                                    if let Err(e) = bl4::apply_edit_state(&mut working.yaml, &edits)
                                    {
                                        let msg = format!(
                                            "Failed to prepare BL4 save: {}",
                                            e.to_string()
                                        );
                                        error!("{}", msg);
                                        self.notification = Some(Notification::new(
                                            msg,
                                            NotificationSentiment::Negative,
                                        ));
                                        return Command::none();
                                    }

                                    let updated_summary = bl4::summarize_from_value(
                                        Path::new(&working.summary.file_name),
                                        &working.yaml,
                                    );
                                    working.summary = updated_summary;

                                    let encrypted =
                                        match bl4::yaml_to_encrypted_sav(&working.yaml, steamid) {
                                            Ok(bytes) => bytes,
                                            Err(e) => {
                                                let msg = format!(
                                                    "Failed to encrypt Borderlands 4 save: {}",
                                                    e.to_string()
                                                );
                                                error!("{}", msg);
                                                self.notification = Some(Notification::new(
                                                    msg,
                                                    NotificationSentiment::Negative,
                                                ));
                                                return Command::none();
                                            }
                                        };

                                    let label_string = working
                                        .summary
                                        .char_name
                                        .clone()
                                        .filter(|s| !s.is_empty())
                                        .unwrap_or_else(|| working.summary.file_name.clone());

                                    let output_file =
                                        self.config.saves_dir().join(&working.summary.file_name);

                                    let backup_dir = self.config.backup_dir().to_path_buf();
                                    let saved_clone = working.clone();

                                    self.manage_save_state.bl4_file = Some(working.clone());
                                    manage_save::bl4::map_summary_to_states(
                                        &mut self.manage_save_state,
                                        &working.summary,
                                    );

                                    return Command::perform(
                                        interaction::file_save::save_bl4_file(
                                            backup_dir,
                                            output_file,
                                            encrypted,
                                            label_string.clone(),
                                        ),
                                        move |r| match r {
                                            Ok(()) => Bl3Message::Bl4SaveCompleted(
                                                MessageResult::Success(saved_clone.clone()),
                                            ),
                                            Err(e) => Bl3Message::Bl4SaveCompleted(
                                                MessageResult::Error(e.to_string()),
                                            ),
                                        },
                                    );
                                }

                                //Lets not make any modifications to the current file just in case we have any errors
                                let mut current_file = self.manage_save_state.current_file.clone();

                                if let Err(e) = manage_save::map_all_states_to_save(
                                    &mut self.manage_save_state,
                                    &mut current_file,
                                ) {
                                    let msg = format!("Failed to save file: {}", e);

                                    error!("{}", msg);

                                    self.notification = Some(Notification::new(
                                        msg,
                                        NotificationSentiment::Negative,
                                    ));

                                    return Command::none();
                                }

                                let output_file = self
                                    .config
                                    .saves_dir()
                                    .join(&self.manage_save_state.current_file.file_name);

                                match current_file.as_bytes() {
                                    Ok((output, save_file)) => {
                                        return Command::perform(
                                            interaction::file_save::save_file(
                                                self.config.backup_dir().to_path_buf(),
                                                output_file,
                                                output,
                                                self.manage_save_state.current_file.clone(),
                                                save_file,
                                            ),
                                            |r| {
                                                Bl3Message::SaveFileCompleted(
                                                    MessageResult::handle_result(r),
                                                )
                                            },
                                        );
                                    }
                                    Err(e) => {
                                        let msg = format!("Failed to save file: {}", e);

                                        error!("{}", msg);

                                        self.notification = Some(Notification::new(
                                            msg,
                                            NotificationSentiment::Negative,
                                        ));
                                    }
                                };
                            }
                        }
                    }
                    InteractionMessage::ManageProfileInteraction(manage_profile_msg) => {
                        match manage_profile_msg {
                            ManageProfileInteractionMessage::TabBar(tab_bar_msg) => {
                                match tab_bar_msg {
                                    ProfileTabBarInteractionMessage::General => {
                                        self.view_state = ViewState::ManageProfile(
                                            ManageProfileView::TabBar(ProfileTabBarView::General),
                                        )
                                    }
                                    ProfileTabBarInteractionMessage::Profile => {
                                        self.view_state = ViewState::ManageProfile(
                                            ManageProfileView::TabBar(ProfileTabBarView::Profile),
                                        )
                                    }
                                    ProfileTabBarInteractionMessage::Keys => {
                                        self.view_state = ViewState::ManageProfile(
                                            ManageProfileView::TabBar(ProfileTabBarView::Keys),
                                        )
                                    }
                                    ProfileTabBarInteractionMessage::Bank => {
                                        self.view_state = ViewState::ManageProfile(
                                            ManageProfileView::TabBar(ProfileTabBarView::Bank),
                                        )
                                    }
                                    ProfileTabBarInteractionMessage::Settings => {
                                        self.view_state = ViewState::ManageProfile(
                                            ManageProfileView::TabBar(ProfileTabBarView::Settings),
                                        )
                                    }
                                }
                            }
                            ManageProfileInteractionMessage::General(general_msg) => {
                                match general_msg {
                                    ProfileGeneralInteractionMessage::ProfileTypeSelected(
                                        profile_type,
                                    ) => {
                                        self.manage_profile_state
                                            .profile_view_state
                                            .general_state
                                            .profile_type_selected = profile_type;
                                    }
                                }
                            }
                            ManageProfileInteractionMessage::Profile(profile_msg) => {
                                match profile_msg {
                                    ProfileInteractionMessage::GuardianRankTokens(
                                        guardian_rank_tokens,
                                    ) => {
                                        self.manage_profile_state
                                            .profile_view_state
                                            .profile_state
                                            .guardian_rank_tokens_input = guardian_rank_tokens;
                                    }
                                    ProfileInteractionMessage::ScienceLevelSelected(
                                        science_level,
                                    ) => {
                                        self.manage_profile_state
                                            .profile_view_state
                                            .profile_state
                                            .science_level_selected = science_level;
                                    }
                                    ProfileInteractionMessage::ScienceTokens(
                                        science_level_tokens,
                                    ) => {
                                        self.manage_profile_state
                                            .profile_view_state
                                            .profile_state
                                            .science_tokens_input = science_level_tokens;
                                    }
                                    ProfileInteractionMessage::SkinMessage(skin_message) => {
                                        let skin_unlocker = &mut self
                                            .manage_profile_state
                                            .profile_view_state
                                            .profile_state
                                            .skin_unlocker;

                                        match skin_message {
                                            SkinUnlockedMessage::CharacterSkins(selected) => {
                                                skin_unlocker.character_skins.is_unlocked =
                                                    selected;
                                            }
                                            SkinUnlockedMessage::CharacterHeads(selected) => {
                                                skin_unlocker.character_heads.is_unlocked =
                                                    selected;
                                            }
                                            SkinUnlockedMessage::EchoThemes(selected) => {
                                                skin_unlocker.echo_themes.is_unlocked = selected;
                                            }
                                            SkinUnlockedMessage::Emotes(selected) => {
                                                skin_unlocker.emotes.is_unlocked = selected;
                                            }
                                            SkinUnlockedMessage::RoomDecorations(selected) => {
                                                skin_unlocker.room_decorations.is_unlocked =
                                                    selected;
                                            }
                                            SkinUnlockedMessage::WeaponSkins(selected) => {
                                                skin_unlocker.weapon_skins.is_unlocked = selected;
                                            }
                                            SkinUnlockedMessage::WeaponTrinkets(selected) => {
                                                skin_unlocker.weapon_trinkets.is_unlocked =
                                                    selected;
                                            }
                                        }
                                    }
                                    ProfileInteractionMessage::SduMessage(sdu_message) => {
                                        let sdu_unlocker = &mut self
                                            .manage_profile_state
                                            .profile_view_state
                                            .profile_state
                                            .sdu_unlocker;

                                        match sdu_message {
                                            SduMessage::Bank(level) => {
                                                sdu_unlocker.bank.input = level;
                                            }
                                            SduMessage::LostLoot(level) => {
                                                sdu_unlocker.lost_loot.input = level;
                                            }
                                        }
                                    }
                                    ProfileInteractionMessage::MaxSduSlotsPressed => {
                                        let sdu_unlocker = &mut self
                                            .manage_profile_state
                                            .profile_view_state
                                            .profile_state
                                            .sdu_unlocker;

                                        sdu_unlocker.bank.input = ProfileSduSlot::Bank.maximum();

                                        sdu_unlocker.lost_loot.input =
                                            ProfileSduSlot::LostLoot.maximum();
                                    }
                                    ProfileInteractionMessage::GuardianRewardMessage(
                                        guardian_message,
                                    ) => {
                                        let guardian_reward_unlocker = &mut self
                                            .manage_profile_state
                                            .profile_view_state
                                            .profile_state
                                            .guardian_reward_unlocker;

                                        match guardian_message {
                                            GuardianRewardMessage::Accuracy(r) => {
                                                guardian_reward_unlocker.accuracy.input = r;
                                            }
                                            GuardianRewardMessage::ActionSkillCooldown(r) => {
                                                guardian_reward_unlocker
                                                    .action_skill_cooldown
                                                    .input = r;
                                            }
                                            GuardianRewardMessage::CriticalDamage(r) => {
                                                guardian_reward_unlocker.critical_damage.input = r;
                                            }
                                            GuardianRewardMessage::ElementalDamage(r) => {
                                                guardian_reward_unlocker.elemental_damage.input = r;
                                            }
                                            GuardianRewardMessage::FFYLDuration(r) => {
                                                guardian_reward_unlocker.ffyl_duration.input = r;
                                            }
                                            GuardianRewardMessage::FFYLMovementSpeed(r) => {
                                                guardian_reward_unlocker
                                                    .ffyl_movement_speed
                                                    .input = r;
                                            }
                                            GuardianRewardMessage::GrenadeDamage(r) => {
                                                guardian_reward_unlocker.grenade_damage.input = r;
                                            }
                                            GuardianRewardMessage::GunDamage(r) => {
                                                guardian_reward_unlocker.gun_damage.input = r;
                                            }
                                            GuardianRewardMessage::GunFireRate(r) => {
                                                guardian_reward_unlocker.gun_fire_rate.input = r;
                                            }
                                            GuardianRewardMessage::MaxHealth(r) => {
                                                guardian_reward_unlocker.max_health.input = r;
                                            }
                                            GuardianRewardMessage::MeleeDamage(r) => {
                                                guardian_reward_unlocker.melee_damage.input = r;
                                            }
                                            GuardianRewardMessage::RarityRate(r) => {
                                                guardian_reward_unlocker.rarity_rate.input = r;
                                            }
                                            GuardianRewardMessage::RecoilReduction(r) => {
                                                guardian_reward_unlocker.recoil_reduction.input = r;
                                            }
                                            GuardianRewardMessage::ReloadSpeed(r) => {
                                                guardian_reward_unlocker.reload_speed.input = r;
                                            }
                                            GuardianRewardMessage::ShieldCapacity(r) => {
                                                guardian_reward_unlocker.shield_capacity.input = r;
                                            }
                                            GuardianRewardMessage::ShieldRechargeDelay(r) => {
                                                guardian_reward_unlocker
                                                    .shield_recharge_delay
                                                    .input = r;
                                            }
                                            GuardianRewardMessage::ShieldRechargeRate(r) => {
                                                guardian_reward_unlocker
                                                    .shield_recharge_rate
                                                    .input = r;
                                            }
                                            GuardianRewardMessage::VehicleDamage(r) => {
                                                guardian_reward_unlocker.vehicle_damage.input = r;
                                            }
                                        }
                                    }
                                    ProfileInteractionMessage::MaxGuardianRewardsPressed => {
                                        let guardian_reward_unlocker = &mut self
                                            .manage_profile_state
                                            .profile_view_state
                                            .profile_state
                                            .guardian_reward_unlocker;

                                        let tokens = i32::MAX;

                                        guardian_reward_unlocker.accuracy.input = tokens;
                                        guardian_reward_unlocker.action_skill_cooldown.input =
                                            tokens;
                                        guardian_reward_unlocker.critical_damage.input = tokens;
                                        guardian_reward_unlocker.elemental_damage.input = tokens;
                                        guardian_reward_unlocker.ffyl_duration.input = tokens;
                                        guardian_reward_unlocker.ffyl_movement_speed.input = tokens;
                                        guardian_reward_unlocker.grenade_damage.input = tokens;
                                        guardian_reward_unlocker.gun_damage.input = tokens;
                                        guardian_reward_unlocker.gun_fire_rate.input = tokens;
                                        guardian_reward_unlocker.max_health.input = tokens;
                                        guardian_reward_unlocker.melee_damage.input = tokens;
                                        guardian_reward_unlocker.rarity_rate.input = tokens;
                                        guardian_reward_unlocker.recoil_reduction.input = tokens;
                                        guardian_reward_unlocker.reload_speed.input = tokens;
                                        guardian_reward_unlocker.shield_capacity.input = tokens;
                                        guardian_reward_unlocker.shield_recharge_delay.input =
                                            tokens;
                                        guardian_reward_unlocker.shield_recharge_rate.input =
                                            tokens;
                                        guardian_reward_unlocker.vehicle_damage.input = tokens;
                                    }
                                }
                            }
                            ManageProfileInteractionMessage::Keys(keys_message) => {
                                let keys_state =
                                    &mut self.manage_profile_state.profile_view_state.keys_state;

                                match keys_message {
                                    ProfileKeysInteractionMessage::GoldenKeys(golden_keys) => {
                                        keys_state.golden_keys_input = golden_keys;
                                    }
                                    ProfileKeysInteractionMessage::DiamondKeys(diamond_keys) => {
                                        keys_state.diamond_keys_input = diamond_keys;
                                    }
                                    ProfileKeysInteractionMessage::VaultCard1Keys(
                                        vault_card_1_keys,
                                    ) => {
                                        keys_state.vault_card_1_keys_input = vault_card_1_keys;
                                    }
                                    ProfileKeysInteractionMessage::VaultCard1Chests(
                                        vault_card_1_chests,
                                    ) => {
                                        keys_state.vault_card_1_chests_input = vault_card_1_chests;
                                    }
                                    ProfileKeysInteractionMessage::VaultCard2Keys(
                                        vault_card_2_keys,
                                    ) => {
                                        keys_state.vault_card_2_keys_input = vault_card_2_keys;
                                    }
                                    ProfileKeysInteractionMessage::VaultCard2Chests(
                                        vault_card_2_chests,
                                    ) => {
                                        keys_state.vault_card_2_chests_input = vault_card_2_chests;
                                    }
                                    ProfileKeysInteractionMessage::VaultCard3Keys(
                                        vault_card_3_keys,
                                    ) => {
                                        keys_state.vault_card_3_keys_input = vault_card_3_keys;
                                    }
                                    ProfileKeysInteractionMessage::VaultCard3Chests(
                                        vault_card_3_chests,
                                    ) => {
                                        keys_state.vault_card_3_chests_input = vault_card_3_chests;
                                    }
                                    ProfileKeysInteractionMessage::MaxGoldenKeysPressed => {
                                        keys_state.golden_keys_input = i32::MAX;
                                    }
                                    ProfileKeysInteractionMessage::MaxDiamondKeysPressed => {
                                        keys_state.diamond_keys_input = i32::MAX;
                                    }
                                    ProfileKeysInteractionMessage::MaxVaultCard1KeysPressed => {
                                        keys_state.vault_card_1_keys_input = i32::MAX;
                                    }
                                    ProfileKeysInteractionMessage::MaxVaultCard1ChestsPressed => {
                                        keys_state.vault_card_1_chests_input = i32::MAX;
                                    }
                                    ProfileKeysInteractionMessage::MaxVaultCard2KeysPressed => {
                                        keys_state.vault_card_2_keys_input = i32::MAX;
                                    }
                                    ProfileKeysInteractionMessage::MaxVaultCard2ChestsPressed => {
                                        keys_state.vault_card_2_chests_input = i32::MAX;
                                    }
                                    ProfileKeysInteractionMessage::MaxVaultCard3KeysPressed => {
                                        keys_state.vault_card_3_keys_input = i32::MAX;
                                    }
                                    ProfileKeysInteractionMessage::MaxVaultCard3ChestsPressed => {
                                        keys_state.vault_card_3_chests_input = i32::MAX;
                                    }
                                }
                            }
                            ManageProfileInteractionMessage::Bank(bank_message) => {
                                match bank_message {
                                    ProfileBankInteractionMessage::Editor(item_editor_message) => {
                                        let res = item_editor_message.update_state(
                                            &mut self
                                                .manage_profile_state
                                                .profile_view_state
                                                .bank_state
                                                .item_editor_state,
                                            ItemEditorFileType::ProfileBank(
                                                &mut self.manage_profile_state.current_file,
                                            ),
                                        );

                                        self.notification = res.notification;

                                        if let Some(command) = res.command {
                                            return command.map(|m| {
                                                Bl3Message::Interaction(
                                                    InteractionMessage::ManageProfileInteraction(
                                                        ManageProfileInteractionMessage::Bank(
                                                            ProfileBankInteractionMessage::Editor(
                                                                m,
                                                            ),
                                                        ),
                                                    ),
                                                )
                                            });
                                        }
                                    }
                                }
                            }
                            ManageProfileInteractionMessage::SaveProfilePressed => {
                                //Lets not make any modifications to the current file just in case we have any errors
                                let mut current_file =
                                    self.manage_profile_state.current_file.clone();

                                let guardian_data_injection_required =
                                    match manage_profile::map_all_states_to_profile(
                                        &mut self.manage_profile_state,
                                        &mut current_file,
                                    ) {
                                        Ok(injection_required) => injection_required,
                                        Err(e) => {
                                            let msg = format!("Failed to save profile: {}", e);

                                            error!("{}", msg);

                                            self.notification = Some(Notification::new(
                                                msg,
                                                NotificationSentiment::Negative,
                                            ));

                                            return Command::none();
                                        }
                                    };

                                let output_file = self
                                    .config
                                    .saves_dir()
                                    .join(&self.manage_profile_state.current_file.file_name);

                                match current_file.as_bytes() {
                                    Ok((output, profile)) => {
                                        return Command::perform(
                                            interaction::file_save::save_profile(
                                                self.config.backup_dir().to_path_buf(),
                                                self.config.saves_dir().to_path_buf(),
                                                output_file,
                                                output,
                                                self.manage_profile_state.current_file.clone(),
                                                profile,
                                                guardian_data_injection_required,
                                            ),
                                            |r| {
                                                Bl3Message::SaveProfileCompleted(
                                                    MessageResult::handle_result(r),
                                                )
                                            },
                                        );
                                    }
                                    Err(e) => {
                                        let msg = format!("Failed to save file: {}", e);

                                        error!("{}", msg);

                                        self.notification = Some(Notification::new(
                                            msg,
                                            NotificationSentiment::Negative,
                                        ));
                                    }
                                };
                            }
                        }
                    }
                    InteractionMessage::SettingsInteraction(settings_msg) => match settings_msg {
                        SettingsInteractionMessage::Bl4SteamIdChanged(value) => {
                            self.settings_state.bl4_steamid = value;
                        }
                        SettingsInteractionMessage::Bl4SavInputChanged(value) => {
                            self.settings_state.bl4_sav_input = value.clone();
                            if self.settings_state.bl4_yaml_output.is_empty()
                                && !value.trim().is_empty()
                            {
                                let mut default = PathBuf::from(value.trim());
                                default.set_extension("yaml");
                                self.settings_state.bl4_yaml_output =
                                    default.to_string_lossy().to_string();
                            }
                        }
                        SettingsInteractionMessage::Bl4YamlOutputChanged(value) => {
                            self.settings_state.bl4_yaml_output = value;
                        }
                        SettingsInteractionMessage::Bl4YamlInputChanged(value) => {
                            self.settings_state.bl4_yaml_input = value.clone();
                            if self.settings_state.bl4_sav_output.is_empty()
                                && !value.trim().is_empty()
                            {
                                let mut default = PathBuf::from(value.trim());
                                default.set_extension("sav");
                                self.settings_state.bl4_sav_output =
                                    default.to_string_lossy().to_string();
                            }
                        }
                        SettingsInteractionMessage::Bl4SavOutputChanged(value) => {
                            self.settings_state.bl4_sav_output = value;
                        }

                        SettingsInteractionMessage::Bl4SelectSavInput => {
                            let hint = self.settings_state.bl4_sav_input.trim().to_string();
                            let initial = if hint.is_empty() {
                                None
                            } else {
                                Some(PathBuf::from(&hint))
                            };

                            return Command::perform(
                                interaction::settings::choose_file(
                                    initial,
                                    &[("Borderlands 4 Saves", &["sav"])],
                                ),
                                |res| {
                                    Bl3Message::Interaction(
                                        InteractionMessage::SettingsInteraction(
                                            SettingsInteractionMessage::Bl4SelectSavInputCompleted(
                                                MessageResult::handle_result(
                                                    res.map(|p| p.to_string_lossy().to_string()),
                                                ),
                                            ),
                                        ),
                                    )
                                },
                            );
                        }
                        SettingsInteractionMessage::Bl4SelectSavInputCompleted(result) => {
                            match result {
                                MessageResult::Success(path) => {
                                    self.settings_state.bl4_sav_input = path.clone();

                                    if self.settings_state.bl4_yaml_output.trim().is_empty() {
                                        let mut default = PathBuf::from(&path);
                                        default.set_extension("yaml");
                                        self.settings_state.bl4_yaml_output =
                                            default.to_string_lossy().to_string();
                                    }
                                }
                                MessageResult::Error(e) => {
                                    if e != "No file was selected." {
                                        self.notification = Some(Notification::new(
                                            format!(
                                                "Failed to select Borderlands 4 save file: {}",
                                                e
                                            ),
                                            NotificationSentiment::Negative,
                                        ));
                                    }
                                }
                            }
                        }
                        SettingsInteractionMessage::Bl4SelectYamlInput => {
                            let hint = self.settings_state.bl4_yaml_input.trim().to_string();
                            let initial = if hint.is_empty() {
                                None
                            } else {
                                Some(PathBuf::from(&hint))
                            };

                            return Command::perform(
                                interaction::settings::choose_file(initial, &[("YAML", &["yaml"])]),
                                |res| {
                                    Bl3Message::Interaction(
                                        InteractionMessage::SettingsInteraction(
                                            SettingsInteractionMessage::Bl4SelectYamlInputCompleted(
                                                MessageResult::handle_result(
                                                    res.map(|p| p.to_string_lossy().to_string()),
                                                ),
                                            ),
                                        ),
                                    )
                                },
                            );
                        }
                        SettingsInteractionMessage::Bl4SelectYamlInputCompleted(result) => {
                            match result {
                                MessageResult::Success(path) => {
                                    self.settings_state.bl4_yaml_input = path.clone();

                                    if self.settings_state.bl4_sav_output.trim().is_empty() {
                                        let mut default = PathBuf::from(&path);
                                        default.set_extension("sav");
                                        self.settings_state.bl4_sav_output =
                                            default.to_string_lossy().to_string();
                                    }
                                }
                                MessageResult::Error(e) => {
                                    if e != "No file was selected." {
                                        self.notification = Some(Notification::new(
                                            format!(
                                                "Failed to select Borderlands 4 YAML file: {}",
                                                e
                                            ),
                                            NotificationSentiment::Negative,
                                        ));
                                    }
                                }
                            }
                        }
                        SettingsInteractionMessage::Bl4DecodeSerialsToggled(checked) => {
                            self.settings_state.bl4_decode_serials = checked;
                        }
                        SettingsInteractionMessage::Bl4EncodeSerialsToggled(checked) => {
                            self.settings_state.bl4_encode_serials = checked;
                        }
                        SettingsInteractionMessage::Bl4Decrypt => {
                            if self.settings_state.bl4_decrypt_in_progress {
                                return Command::none();
                            }

                            let steamid = self.settings_state.bl4_steamid.trim().to_string();
                            if steamid.is_empty() {
                                let msg = "Borderlands 4 decrypt requires a Steam ID.".to_string();
                                error!("{}", msg);
                                self.notification =
                                    Some(Notification::new(msg, NotificationSentiment::Negative));
                                return Command::none();
                            }

                            let sav_input = self.settings_state.bl4_sav_input.trim().to_string();
                            if sav_input.is_empty() {
                                let msg = "Select a Borderlands 4 save file before decrypting."
                                    .to_string();
                                error!("{}", msg);
                                self.notification =
                                    Some(Notification::new(msg, NotificationSentiment::Negative));
                                return Command::none();
                            }

                            if !Path::new(&sav_input).exists() {
                                let msg =
                                    "Borderlands 4 save file does not exist at the provided path."
                                        .to_string();
                                error!("{}", msg);
                                self.notification =
                                    Some(Notification::new(msg, NotificationSentiment::Negative));
                                return Command::none();
                            }

                            let mut yaml_output =
                                self.settings_state.bl4_yaml_output.trim().to_string();
                            if yaml_output.is_empty() {
                                let mut default = PathBuf::from(&sav_input);
                                default.set_extension("yaml");
                                yaml_output = default.to_string_lossy().to_string();
                                self.settings_state.bl4_yaml_output = yaml_output.clone();
                            }

                            let decode_serials = self.settings_state.bl4_decode_serials;
                            self.settings_state.bl4_steamid = steamid.clone();
                            self.settings_state.bl4_sav_input = sav_input.clone();
                            self.settings_state.bl4_decrypt_in_progress = true;

                            let input_path = PathBuf::from(&sav_input);
                            let output_path = PathBuf::from(&yaml_output);
                            let output_path_clone = output_path.to_string_lossy().to_string();

                            self.config.set_bl4_user_id(Some(steamid.clone()));

                            let save_command = Command::perform(self.config.clone().save(), |r| {
                                Bl3Message::Config(ConfigMessage::SaveCompleted(
                                    MessageResult::handle_result(r),
                                ))
                            });

                            let decrypt_command = Command::perform(
                                interaction::settings::bl4_decrypt(
                                    input_path,
                                    output_path.clone(),
                                    steamid,
                                    decode_serials,
                                ),
                                move |res| {
                                    Bl3Message::Interaction(
                                        InteractionMessage::SettingsInteraction(
                                            SettingsInteractionMessage::Bl4DecryptCompleted(
                                                output_path_clone.clone(),
                                                decode_serials,
                                                MessageResult::handle_result(res),
                                            ),
                                        ),
                                    )
                                },
                            );

                            return Command::batch(vec![save_command, decrypt_command]);
                        }
                        SettingsInteractionMessage::Bl4DecryptCompleted(
                            output_path,
                            decode_serials,
                            res,
                        ) => {
                            self.settings_state.bl4_decrypt_in_progress = false;

                            match res {
                                MessageResult::Success(outcome) => {
                                    let message = match outcome {
                                        DecryptOutcome::WithDecoded { decoded_count } => format!(
                                            "BL4 decrypt wrote {} with {} decoded item serials.",
                                            output_path, decoded_count
                                        ),
                                        DecryptOutcome::Plain => {
                                            if decode_serials {
                                                format!(
                                                    "BL4 decrypt wrote {} (no item serials found to decode).",
                                                    output_path
                                                )
                                            } else {
                                                format!("BL4 decrypt wrote {}.", output_path)
                                            }
                                        }
                                    };

                                    self.notification = Some(Notification::new(
                                        message,
                                        NotificationSentiment::Positive,
                                    ));
                                }
                                MessageResult::Error(e) => {
                                    let msg =
                                        format!("Failed to decrypt Borderlands 4 save: {}", e);
                                    error!("{}", msg);
                                    self.notification = Some(Notification::new(
                                        msg,
                                        NotificationSentiment::Negative,
                                    ));
                                }
                            }
                        }
                        SettingsInteractionMessage::Bl4Encrypt => {
                            if self.settings_state.bl4_encrypt_in_progress {
                                return Command::none();
                            }

                            let steamid = self.settings_state.bl4_steamid.trim().to_string();
                            if steamid.is_empty() {
                                let msg = "Borderlands 4 encrypt requires a Steam ID.".to_string();
                                error!("{}", msg);
                                self.notification =
                                    Some(Notification::new(msg, NotificationSentiment::Negative));
                                return Command::none();
                            }

                            let yaml_input = self.settings_state.bl4_yaml_input.trim().to_string();
                            if yaml_input.is_empty() {
                                let msg = "Select a Borderlands 4 YAML file before encrypting."
                                    .to_string();
                                error!("{}", msg);
                                self.notification =
                                    Some(Notification::new(msg, NotificationSentiment::Negative));
                                return Command::none();
                            }

                            if !Path::new(&yaml_input).exists() {
                                let msg =
                                    "Borderlands 4 YAML file does not exist at the provided path."
                                        .to_string();
                                error!("{}", msg);
                                self.notification =
                                    Some(Notification::new(msg, NotificationSentiment::Negative));
                                return Command::none();
                            }

                            let mut sav_output =
                                self.settings_state.bl4_sav_output.trim().to_string();
                            if sav_output.is_empty() {
                                let mut default = PathBuf::from(&yaml_input);
                                default.set_extension("sav");
                                sav_output = default.to_string_lossy().to_string();
                                self.settings_state.bl4_sav_output = sav_output.clone();
                            }

                            let encode_serials = self.settings_state.bl4_encode_serials;
                            self.settings_state.bl4_steamid = steamid.clone();
                            self.settings_state.bl4_yaml_input = yaml_input.clone();
                            self.settings_state.bl4_encrypt_in_progress = true;

                            let input_path = PathBuf::from(&yaml_input);
                            let output_path = PathBuf::from(&sav_output);
                            let output_path_clone = output_path.to_string_lossy().to_string();

                            self.config.set_bl4_user_id(Some(steamid.clone()));

                            let save_command = Command::perform(self.config.clone().save(), |r| {
                                Bl3Message::Config(ConfigMessage::SaveCompleted(
                                    MessageResult::handle_result(r),
                                ))
                            });

                            let encrypt_command = Command::perform(
                                interaction::settings::bl4_encrypt(
                                    input_path,
                                    output_path.clone(),
                                    steamid,
                                    encode_serials,
                                ),
                                move |res| {
                                    Bl3Message::Interaction(
                                        InteractionMessage::SettingsInteraction(
                                            SettingsInteractionMessage::Bl4EncryptCompleted(
                                                output_path_clone.clone(),
                                                encode_serials,
                                                MessageResult::handle_result(res),
                                            ),
                                        ),
                                    )
                                },
                            );

                            return Command::batch(vec![save_command, encrypt_command]);
                        }
                        SettingsInteractionMessage::Bl4EncryptCompleted(
                            output_path,
                            encode_serials,
                            res,
                        ) => {
                            self.settings_state.bl4_encrypt_in_progress = false;

                            match res {
                                MessageResult::Success(outcome) => {
                                    let message = match outcome {
                                        EncryptOutcome::Reencoded => format!(
                                            "BL4 encrypt wrote {} with re-encoded item serials.",
                                            output_path
                                        ),
                                        EncryptOutcome::NoDecodedSection => format!(
                                            "BL4 encrypt wrote {} (no decoded items section found).",
                                            output_path
                                        ),
                                        EncryptOutcome::Plain => {
                                            if encode_serials {
                                                format!(
                                                    "BL4 encrypt wrote {}.",
                                                    output_path
                                                )
                                            } else {
                                                format!("BL4 encrypt wrote {}.", output_path)
                                            }
                                        }
                                    };

                                    self.notification = Some(Notification::new(
                                        message,
                                        NotificationSentiment::Positive,
                                    ));
                                }
                                MessageResult::Error(e) => {
                                    let msg =
                                        format!("Failed to encrypt Borderlands 4 YAML: {}", e);
                                    error!("{}", msg);
                                    self.notification = Some(Notification::new(
                                        msg,
                                        NotificationSentiment::Negative,
                                    ));
                                }
                            }
                        }
                        SettingsInteractionMessage::OpenConfigDir => {
                            return Command::perform(
                                interaction::settings::open_dir(
                                    self.config.config_dir().to_path_buf(),
                                ),
                                |r| {
                                    Bl3Message::Interaction(
                                        InteractionMessage::SettingsInteraction(
                                            SettingsInteractionMessage::OpenConfigDirCompleted(
                                                MessageResult::handle_result(r),
                                            ),
                                        ),
                                    )
                                },
                            );
                        }
                        SettingsInteractionMessage::OpenConfigDirCompleted(res) => {
                            res.handle_ui_error(
                                "Failed to open config folder",
                                &mut self.notification,
                            );
                        }
                        SettingsInteractionMessage::OpenBackupDir => {
                            return Command::perform(
                                interaction::settings::open_dir(
                                    self.config.backup_dir().to_path_buf(),
                                ),
                                |r| {
                                    Bl3Message::Interaction(
                                        InteractionMessage::SettingsInteraction(
                                            SettingsInteractionMessage::OpenBackupDirCompleted(
                                                MessageResult::handle_result(r),
                                            ),
                                        ),
                                    )
                                },
                            );
                        }
                        SettingsInteractionMessage::OpenBackupDirCompleted(res) => {
                            res.handle_ui_error(
                                "Failed to open backups folder",
                                &mut self.notification,
                            );
                        }
                        SettingsInteractionMessage::ChangeBackupDir => {
                            self.settings_state.choose_backup_dir_window_open = true;

                            return Command::perform(
                                interaction::choose_dir(self.config.backup_dir().to_path_buf()),
                                |r| {
                                    Bl3Message::Interaction(
                                        InteractionMessage::SettingsInteraction(
                                            SettingsInteractionMessage::ChangeBackupDirCompleted(
                                                MessageResult::handle_result(r),
                                            ),
                                        ),
                                    )
                                },
                            );
                        }
                        SettingsInteractionMessage::ChangeBackupDirCompleted(choose_dir_res) => {
                            self.settings_state.choose_backup_dir_window_open = false;

                            match choose_dir_res {
                                MessageResult::Success(dir) => {
                                    self.config.set_backup_dir(dir);
                                    self.settings_state.backup_dir_input =
                                        self.config.backup_dir().to_string_lossy().to_string();

                                    return Command::perform(self.config.clone().save(), |r| {
                                        Bl3Message::Config(ConfigMessage::SaveCompleted(
                                            MessageResult::handle_result(r),
                                        ))
                                    });
                                }
                                MessageResult::Error(e) => {
                                    let msg = format!("Failed to choose backups folder: {}", e);

                                    error!("{}", msg);

                                    self.notification = Some(Notification::new(
                                        msg,
                                        NotificationSentiment::Negative,
                                    ));
                                }
                            }
                        }
                        SettingsInteractionMessage::OpenSavesDir => {
                            return Command::perform(
                                interaction::settings::open_dir(
                                    self.config.saves_dir().to_path_buf(),
                                ),
                                |r| {
                                    Bl3Message::Interaction(
                                        InteractionMessage::SettingsInteraction(
                                            SettingsInteractionMessage::OpenSavesDirCompleted(
                                                MessageResult::handle_result(r),
                                            ),
                                        ),
                                    )
                                },
                            );
                        }
                        SettingsInteractionMessage::OpenSavesDirCompleted(res) => {
                            res.handle_ui_error(
                                "Failed to open saves folder",
                                &mut self.notification,
                            );
                        }
                        SettingsInteractionMessage::ChangeSavesDir => {
                            self.settings_state.choose_saves_dir_window_open = true;

                            return Command::perform(
                                interaction::choose_dir(self.config.saves_dir().to_path_buf()),
                                |r| {
                                    Bl3Message::Interaction(
                                        InteractionMessage::SettingsInteraction(
                                            SettingsInteractionMessage::ChangeSavesDirCompleted(
                                                MessageResult::handle_result(r),
                                            ),
                                        ),
                                    )
                                },
                            );
                        }
                        SettingsInteractionMessage::ChangeSavesDirCompleted(choose_dir_res) => {
                            self.settings_state.choose_saves_dir_window_open = false;

                            match choose_dir_res {
                                MessageResult::Success(dir) => {
                                    self.view_state = ViewState::Loading;
                                    // Prefer the in-memory Settings steamid, falling back to persisted config
                                    let effective_bl4_id = if !self
                                        .settings_state
                                        .bl4_steamid
                                        .trim()
                                        .is_empty()
                                    {
                                        Some(self.settings_state.bl4_steamid.trim().to_string())
                                    } else {
                                        self.config.bl4_user_id().cloned()
                                    };

                                    // If the settings value is present, persist it to config
                                    let mut commands: Vec<Command<Bl3Message>> = Vec::new();
                                    if let Some(ref sid) = effective_bl4_id {
                                        self.config.set_bl4_user_id(Some(sid.clone()));
                                        commands.push(Command::perform(self.config.clone().save(), |r| {
                                            Bl3Message::Config(ConfigMessage::SaveCompleted(
                                                MessageResult::handle_result(r),
                                            ))
                                        }));
                                    }

                                    commands.push(Command::perform(
                                        interaction::choose_save_directory::load_files_in_directory(
                                            dir,
                                            effective_bl4_id,
                                        ),
                                        |r| {
                                            Bl3Message::ChooseSave(ChooseSaveMessage::FilesLoaded(
                                                MessageResult::handle_result(r),
                                            ))
                                        },
                                    ));

                                    return Command::batch(commands);
                                }
                                MessageResult::Error(e) => {
                                    let msg = format!("Failed to choose saves folder: {}", e);

                                    error!("{}", msg);

                                    self.notification = Some(Notification::new(
                                        msg,
                                        NotificationSentiment::Negative,
                                    ));
                                }
                            }
                        }
                        SettingsInteractionMessage::DecreaseUIScale => {
                            if self.settings_state.ui_scale_factor >= 0.50 {
                                self.settings_state.ui_scale_factor -= 0.05;

                                self.config
                                    .set_ui_scale_factor(self.settings_state.ui_scale_factor);

                                return Command::perform(self.config.clone().save(), |r| {
                                    Bl3Message::Config(ConfigMessage::SaveCompleted(
                                        MessageResult::handle_result(r),
                                    ))
                                });
                            }
                        }
                        SettingsInteractionMessage::IncreaseUIScale => {
                            if self.settings_state.ui_scale_factor < 2.0 {
                                self.settings_state.ui_scale_factor += 0.05;

                                self.config
                                    .set_ui_scale_factor(self.settings_state.ui_scale_factor);

                                return Command::perform(self.config.clone().save(), |r| {
                                    Bl3Message::Config(ConfigMessage::SaveCompleted(
                                        MessageResult::handle_result(r),
                                    ))
                                });
                            }
                        }
                    },
                    InteractionMessage::LoadedFileSelected(loaded_file) => {
                        self.loaded_files_selected = loaded_file;

                        state_mappers::map_loaded_file_to_state(self).handle_ui_error(
                            "Failed to map loaded file to editor",
                            &mut self.notification,
                        );
                    }
                    InteractionMessage::RefreshSavesDirectory => {
                        self.view_state = ViewState::Loading;

                        return Command::perform(
                            interaction::choose_save_directory::load_files_in_directory(
                                self.config.saves_dir().to_path_buf(),
                                self.config.bl4_user_id().cloned(),
                            ),
                            |r| {
                                Bl3Message::ChooseSave(ChooseSaveMessage::FilesLoaded(
                                    MessageResult::handle_result(r),
                                ))
                            },
                        );
                    }
                    InteractionMessage::Ignore => {}
                }
            }
            Bl3Message::ChooseSave(choose_save_msg) => match choose_save_msg {
                ChooseSaveMessage::ChooseDirCompleted(choose_dir_res) => {
                    self.choose_save_directory_state.choose_dir_window_open = false;

                    match choose_dir_res {
                        MessageResult::Success(dir) => {
                            self.view_state = ViewState::Loading;

                            return Command::perform(
                                interaction::choose_save_directory::load_files_in_directory(
                                    dir,
                                    self.config.bl4_user_id().cloned(),
                                ),
                                |r| {
                                    Bl3Message::ChooseSave(ChooseSaveMessage::FilesLoaded(
                                        MessageResult::handle_result(r),
                                    ))
                                },
                            );
                        }
                        MessageResult::Error(e) => {
                            let msg = format!("Failed to choose saves folder: {}", e);

                            error!("{}", msg);

                            self.notification =
                                Some(Notification::new(msg, NotificationSentiment::Negative));
                        }
                    }
                }
                ChooseSaveMessage::FilesLoaded(res) => match res {
                    MessageResult::Success((dir, mut files)) => {
                        files.sort();
                        self.loaded_files = files;

                        self.loaded_files_selected = Box::new(
                            self.loaded_files
                                .get(0)
                                .expect("loaded_files was empty")
                                .clone(),
                        );

                        state_mappers::map_loaded_file_to_state(self).handle_ui_error(
                            "Failed to map loaded file to editor",
                            &mut self.notification,
                        );

                        self.config.set_saves_dir(dir);
                        self.settings_state.saves_dir_input =
                            self.config.saves_dir().to_string_lossy().to_string();

                        return Command::perform(self.config.clone().save(), |r| {
                            Bl3Message::Config(ConfigMessage::SaveCompleted(
                                MessageResult::handle_result(r),
                            ))
                        });
                    }
                    MessageResult::Error(e) => {
                        let msg = format!("Failed to load save folder: {}", e);

                        error!("{}", msg);

                        self.view_state = ViewState::ChooseSaveDirectory;

                        self.notification =
                            Some(Notification::new(msg, NotificationSentiment::Negative));
                    }
                },
            },
            Bl3Message::SaveFileCompleted(res) => match res {
                MessageResult::Success(save) => {
                    self.notification = Some(Notification::new(
                        "Successfully saved file!",
                        NotificationSentiment::Positive,
                    ));

                    self.is_reloading_saves = true;

                    let bl3_file_type = match save.header_type {
                        HeaderType::PcSave => Bl3FileType::PcSave(save),
                        HeaderType::Ps4Save => Bl3FileType::Ps4Save(save),
                        _ => {
                            let msg = "Unexpected Bl3FileType when reloading save";

                            error!("{}", msg);
                            panic!("{}", msg);
                        }
                    };

                    return Command::perform(
                        interaction::file_save::load_files_after_save(
                            self.config.saves_dir().to_path_buf(),
                            self.config.bl4_user_id().cloned(),
                            bl3_file_type,
                        ),
                        |r| Bl3Message::FilesLoadedAfterSave(MessageResult::handle_result(r)),
                    );
                }
                MessageResult::Error(e) => {
                    let msg = format!("Failed to save file: {}", e);

                    error!("{}", msg);

                    self.notification =
                        Some(Notification::new(msg, NotificationSentiment::Negative));
                }
            },
            Bl3Message::Bl4SaveCompleted(res) => match res {
                MessageResult::Success(save) => {
                    self.notification = Some(Notification::new(
                        "Successfully saved file!",
                        NotificationSentiment::Positive,
                    ));

                    self.is_reloading_saves = true;
                    self.manage_save_state.bl4_file = Some(save.clone());
                    manage_save::bl4::map_summary_to_states(
                        &mut self.manage_save_state,
                        &save.summary,
                    );

                    return Command::perform(
                        interaction::file_save::load_files_after_save(
                            self.config.saves_dir().to_path_buf(),
                            self.config.bl4_user_id().cloned(),
                            Bl3FileType::Bl4Save(save.clone()),
                        ),
                        |r| Bl3Message::FilesLoadedAfterSave(MessageResult::handle_result(r)),
                    );
                }
                MessageResult::Error(e) => {
                    let msg = format!("Failed to save Borderlands 4 file: {}", e);

                    error!("{}", msg);

                    self.notification =
                        Some(Notification::new(msg, NotificationSentiment::Negative));
                }
            },
            Bl3Message::SaveProfileCompleted(res) => match res {
                MessageResult::Success(profile) => {
                    self.notification = Some(Notification::new(
                        "Successfully saved profile!",
                        NotificationSentiment::Positive,
                    ));

                    self.is_reloading_saves = true;

                    let bl3_file_type = match profile.header_type {
                        HeaderType::PcProfile => Bl3FileType::PcProfile(profile),
                        HeaderType::Ps4Profile => Bl3FileType::Ps4Profile(profile),
                        _ => {
                            let msg = "Unexpected Bl3FileType when reloading profile";

                            error!("{}", msg);
                            panic!("{}", msg);
                        }
                    };

                    return Command::perform(
                        interaction::file_save::load_files_after_save(
                            self.config.saves_dir().to_path_buf(),
                            self.config.bl4_user_id().cloned(),
                            bl3_file_type,
                        ),
                        |r| Bl3Message::FilesLoadedAfterSave(MessageResult::handle_result(r)),
                    );
                }
                MessageResult::Error(e) => {
                    let msg = format!("Failed to save profile: {}", e);

                    error!("{}", msg);

                    self.notification =
                        Some(Notification::new(msg, NotificationSentiment::Negative));
                }
            },
            Bl3Message::FilesLoadedAfterSave(res) => {
                match res {
                    MessageResult::Success((saved_file, mut files)) => {
                        files.sort();

                        self.loaded_files = files;

                        let selected_file = self.loaded_files.iter().find(|f| **f == saved_file);

                        if let Some(selected_file) = selected_file {
                            self.loaded_files_selected = Box::new(selected_file.to_owned());

                            match selected_file {
                                Bl3FileType::PcProfile(_)
                                | Bl3FileType::Ps4Profile(_)
                                | Bl3FileType::Bl4Save(_) => {
                                    state_mappers::map_loaded_file_to_state(self).handle_ui_error(
                                        "Failed to map loaded file to editor",
                                        &mut self.notification,
                                    );
                                }
                                _ => (),
                            }
                        } else {
                            self.loaded_files_selected = Box::new(
                                self.loaded_files
                                    .get(0)
                                    .expect("loaded_files was empty")
                                    .clone(),
                            );

                            state_mappers::map_loaded_file_to_state(self).handle_ui_error(
                                "Failed to map loaded file to editor",
                                &mut self.notification,
                            );
                        }
                    }
                    MessageResult::Error(e) => {
                        let msg = format!("Failed to load save folder: {}", e);

                        error!("{}", msg);

                        self.notification =
                            Some(Notification::new(msg, NotificationSentiment::Negative));
                    }
                }

                self.is_reloading_saves = false;
            }
            Bl3Message::ClearNotification => {
                self.notification = None;
            }
        };

        Command::none()
    }

    fn view(&mut self) -> Element<'_, Self::Message> {
        let banner_title = if self.is_current_bl4() {
            "Borderlands 4 Save Editor"
        } else {
            "Borderlands 3 Save Editor"
        };

        let title = Text::new(banner_title.to_uppercase())
            .font(JETBRAINS_MONO_NL_EXTRA_BOLD_ITALIC)
            .size(40)
            .color(Color::from_rgb8(242, 203, 5))
            .width(Length::Fill)
            .horizontal_alignment(Horizontal::Left);

        let refresh_icon_handle = svg::Handle::from_memory(REFRESH);

        let refresh_icon = Svg::new(refresh_icon_handle)
            .height(Length::Units(17))
            .width(Length::Units(17));

        let refresh_button = Tooltip::new(
            Button::new(&mut self.refresh_button_state, refresh_icon)
                .on_press(InteractionMessage::RefreshSavesDirectory)
                .padding(10)
                .style(Bl3UiStyle)
                .into_element(),
            "Refresh saves folder",
            tooltip::Position::Bottom,
        )
        .gap(10)
        .padding(10)
        .font(JETBRAINS_MONO)
        .size(17)
        .style(Bl3UiTooltipStyle);

        let all_saves_picklist = if !self.is_reloading_saves {
            PickList::new(
                &mut self.loaded_files_selector,
                &self.loaded_files,
                Some(*self.loaded_files_selected.clone()),
                |f| InteractionMessage::LoadedFileSelected(Box::new(f)),
            )
            .font(JETBRAINS_MONO)
            .text_size(17)
            .width(Length::Fill)
            .padding(10)
            .style(Bl3UiStyle)
            .into_element()
        } else {
            Container::new(
                Text::new("Reloading saves...")
                    .font(JETBRAINS_MONO)
                    .color(Color::from_rgb8(220, 220, 200))
                    .size(17),
            )
            .width(Length::Fill)
            .padding(10)
            .style(Bl3UiStyle)
            .into()
        };

        let view_state_discrim = mem::discriminant(&self.view_state);
        let manage_save_discrim = mem::discriminant(&ViewState::ManageSave(
            ManageSaveView::TabBar(SaveTabBarView::General),
        ));
        let manage_profile_discrim = mem::discriminant(&ViewState::ManageProfile(
            ManageProfileView::TabBar(ProfileTabBarView::General),
        ));
        let mut save_button = Button::new(
            &mut self.save_file_button_state,
            Text::new("Save").font(JETBRAINS_MONO_BOLD).size(17),
        )
        .padding(10)
        .style(Bl3UiStyle);

        if view_state_discrim == manage_save_discrim {
            save_button = save_button.on_press(InteractionMessage::ManageSaveInteraction(
                ManageSaveInteractionMessage::SaveFilePressed,
            ));
        } else if view_state_discrim == manage_profile_discrim {
            save_button = save_button.on_press(InteractionMessage::ManageProfileInteraction(
                ManageProfileInteractionMessage::SaveProfilePressed,
            ));
        }

        let mut menu_bar_editor_content = Row::new()
            .push(title)
            .spacing(15)
            .align_items(Alignment::Center);

        if view_state_discrim == manage_save_discrim || view_state_discrim == manage_profile_discrim
        {
            menu_bar_editor_content = menu_bar_editor_content.push(refresh_button);
            menu_bar_editor_content = menu_bar_editor_content.push(all_saves_picklist);
            menu_bar_editor_content = menu_bar_editor_content.push(save_button.into_element());
        }

        // Always show a Settings button (with gear icon), right-aligned
        let settings_icon = Svg::new(svg::Handle::from_memory(SETTINGS))
            .height(Length::Units(18))
            .width(Length::Units(18));
        let settings_label = Text::new("Settings").font(JETBRAINS_MONO_BOLD).size(18);
        let settings_btn_content = Row::new()
            .push(settings_icon)
            .push(settings_label)
            .spacing(8)
            .align_items(Alignment::Center);

        let settings_button_el = Button::new(&mut self.settings_top_button_state, settings_btn_content)
            .on_press(InteractionMessage::ManageSaveInteraction(
                ManageSaveInteractionMessage::TabBar(SaveTabBarInteractionMessage::Settings),
            ))
            .style(Bl3UiStyle)
            .padding(8)
            .into_element();

        // push a spacer before settings to align it to the right
        menu_bar_editor_content = menu_bar_editor_content
            .push(Space::with_width(Length::Fill))
            .push(settings_button_el);

        let mut menu_bar_content = Column::new().push(menu_bar_editor_content).spacing(10);

        if let Some(latest_release) = &self.latest_release {
            let mut update_button = Button::new(
                &mut self.update_button_state,
                Text::new(match self.is_updating {
                    true => "Updating...".to_string(),
                    false => format!(
                        "Click here to update to version: {}",
                        latest_release.tag_name
                    ),
                })
                .font(JETBRAINS_MONO_BOLD)
                .size(17),
            )
            .padding(10)
            .style(Bl3UiPositiveButtonStyle);

            if !self.is_updating {
                update_button = update_button.on_press(Bl3Message::UpdateToLatestRelease);
            }

            let update_content = Container::new(
                Row::new()
                    .push(update_button)
                    .spacing(10)
                    .align_items(Alignment::Center),
            )
            .width(Length::Fill)
            .align_x(Horizontal::Left);

            menu_bar_content = menu_bar_content.push(update_content);
        }

        let menu_bar = Container::new(menu_bar_content)
            .padding(20)
            .width(Length::Fill)
            .style(Bl3UiMenuBarStyle);

        let content = match &self.view_state {
            ViewState::Initializing => views::initialization::view(),
            ViewState::Loading => views::loading::view(),
            ViewState::ChooseSaveDirectory => {
                views::choose_save_directory::view(&mut self.choose_save_directory_state)
            }
            ViewState::ManageSave(manage_save_view) => match manage_save_view {
                ManageSaveView::TabBar(main_tab_bar_view) => views::manage_save::main::view(
                    &mut self.settings_state,
                    &mut self.manage_save_state,
                    main_tab_bar_view,
                ),
            },
            ViewState::ManageProfile(manage_profile_view) => match manage_profile_view {
                ManageProfileView::TabBar(main_tab_bar_view) => views::manage_profile::main::view(
                    &mut self.settings_state,
                    &mut self.manage_profile_state,
                    main_tab_bar_view,
                ),
            },
        };

        let mut all_content = Column::new().push(menu_bar);

        if let Some(notification) = &mut self.notification {
            all_content = all_content.push(notification.view());
        }

        all_content = all_content.push(content);

        Container::new(all_content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(Bl3UiContentStyle)
            .into()
    }

    fn background_color(&self) -> Color {
        Color::from_rgb8(23, 23, 23)
    }

    fn scale_factor(&self) -> f64 {
        self.settings_state.ui_scale_factor
    }
}
