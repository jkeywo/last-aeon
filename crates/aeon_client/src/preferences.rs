//! Presentation-only interface preferences and their platform persistence.
//!
//! These values never enter a campaign resource, snapshot, command, or state
//! hash. Native and web builds share the same versioned document codec while
//! each supplies the smallest storage adapter its platform needs.

use bevy::prelude::*;
use bevy_egui::egui;
use serde::{Deserialize, Serialize};

const DOCUMENT_VERSION: u32 = 1;
#[cfg(not(target_arch = "wasm32"))]
const PREFERENCES_FILENAME: &str = "preferences.json";
#[cfg(any(test, target_arch = "wasm32"))]
const WEB_STORAGE_KEY: &str = "last-aeon.ui.preferences";

/// The supported whole-interface scales.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UiScale {
    /// Preserve the authored token sizes.
    #[default]
    Percent100,
    /// One-and-a-quarter times the authored size.
    Percent125,
    /// One-and-a-half times the authored size.
    Percent150,
    /// Twice the authored size.
    Percent200,
}

impl UiScale {
    const ALL: [Self; 4] = [
        Self::Percent100,
        Self::Percent125,
        Self::Percent150,
        Self::Percent200,
    ];

    fn factor(self) -> f32 {
        match self {
            Self::Percent100 => 1.0,
            Self::Percent125 => 1.25,
            Self::Percent150 => 1.5,
            Self::Percent200 => 2.0,
        }
    }

    fn label_key(self) -> &'static str {
        match self {
            Self::Percent100 => "ui.preferences.scale-100",
            Self::Percent125 => "ui.preferences.scale-125",
            Self::Percent150 => "ui.preferences.scale-150",
            Self::Percent200 => "ui.preferences.scale-200",
        }
    }
}

/// How much room controls receive, independently of their visual scale.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UiDensity {
    /// The existing compact strategic workspace.
    #[default]
    Compact,
    /// Larger gaps, rows, and control targets without removing information.
    Comfortable,
}

impl UiDensity {
    const ALL: [Self; 2] = [Self::Compact, Self::Comfortable];

    fn label_key(self) -> &'static str {
        match self {
            Self::Compact => "ui.preferences.density-compact",
            Self::Comfortable => "ui.preferences.density-comfortable",
        }
    }

    fn apply(self, style: &mut egui::Style) {
        if self == Self::Compact {
            return;
        }
        // Density changes only measurements. It never changes visibility,
        // truncation policy, or which information a surface draws.
        const COMFORTABLE: f32 = 1.25;
        style.spacing.item_spacing *= COMFORTABLE;
        style.spacing.button_padding *= COMFORTABLE;
        style.spacing.interact_size.y *= COMFORTABLE;
        style.spacing.icon_width *= COMFORTABLE;
        style.spacing.icon_width_inner *= COMFORTABLE;
        style.spacing.icon_spacing *= COMFORTABLE;
    }
}

/// Client-owned preferences shared by the title and campaign surfaces.
#[derive(Resource, Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct UiPreferences {
    /// Whole-interface scale.
    pub scale: UiScale,
    /// Spacing and control density.
    pub density: UiDensity,
}

/// Whether the campaign settings window is open.
#[derive(Resource, Default)]
pub struct SettingsUi {
    /// The settings window is being shown.
    pub open: bool,
    /// Stable control that opened settings, resolved against each new frame.
    pub invoker: Option<crate::ui::keyboard::LogicalFocus>,
}

impl SettingsUi {
    /// Opens settings from a rendered logical control.
    pub fn open_from(&mut self, invoker: crate::ui::keyboard::LogicalFocus) {
        self.open = true;
        self.invoker = Some(invoker);
    }

    /// Closes settings and restores the current rendering of its invoker.
    pub fn close(&mut self, ctx: &egui::Context) {
        self.open = false;
        if let Some(invoker) = self.invoker.take() {
            crate::ui::keyboard::request_logical(ctx, invoker);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PreferenceDocument {
    version: u32,
    ui_scale: UiScale,
    density: UiDensity,
}

impl From<UiPreferences> for PreferenceDocument {
    fn from(value: UiPreferences) -> Self {
        Self {
            version: DOCUMENT_VERSION,
            ui_scale: value.scale,
            density: value.density,
        }
    }
}

fn encode(preferences: UiPreferences) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&PreferenceDocument::from(preferences))
}

fn decode(document: &str) -> Result<UiPreferences, String> {
    let document: PreferenceDocument =
        serde_json::from_str(document).map_err(|error| error.to_string())?;
    if document.version != DOCUMENT_VERSION {
        return Err(format!(
            "unsupported preferences version {}",
            document.version
        ));
    }
    Ok(UiPreferences {
        scale: document.ui_scale,
        density: document.density,
    })
}

trait PreferenceStore {
    fn load(&self) -> Result<Option<String>, String>;
    fn save(&self, document: &str) -> Result<(), String>;
}

/// A browser-shaped key/value backend. Keeping the Web Storage mechanics
/// behind this seam lets the same key, errors, and codec be exercised without
/// requiring a JavaScript runtime in Rust's ordinary test runner.
#[cfg(any(test, target_arch = "wasm32"))]
trait KeyValueBackend {
    fn get(&self, key: &str) -> Result<Option<String>, String>;
    fn set(&self, key: &str, value: &str) -> Result<(), String>;
}

#[cfg(any(test, target_arch = "wasm32"))]
struct KeyValueStore<B> {
    backend: B,
    key: &'static str,
}

#[cfg(any(test, target_arch = "wasm32"))]
impl<B: KeyValueBackend> PreferenceStore for KeyValueStore<B> {
    fn load(&self) -> Result<Option<String>, String> {
        self.backend.get(self.key)
    }

    fn save(&self, document: &str) -> Result<(), String> {
        self.backend.set(self.key, document)
    }
}

fn load_from(store: &impl PreferenceStore) -> UiPreferences {
    store
        .load()
        .ok()
        .flatten()
        .and_then(|document| decode(&document).ok())
        .unwrap_or_default()
}

fn save_to(store: &impl PreferenceStore, preferences: UiPreferences) -> Result<(), String> {
    let document = encode(preferences).map_err(|error| error.to_string())?;
    store.save(&document)
}

#[cfg(not(target_arch = "wasm32"))]
struct NativeStore {
    path: std::path::PathBuf,
}

#[cfg(not(target_arch = "wasm32"))]
impl NativeStore {
    fn at(path: impl Into<std::path::PathBuf>) -> Self {
        Self { path: path.into() }
    }

    fn for_current_user() -> Result<Self, String> {
        Ok(Self::at(native_preferences_path()?))
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl PreferenceStore for NativeStore {
    fn load(&self) -> Result<Option<String>, String> {
        match std::fs::read_to_string(&self.path) {
            Ok(document) => Ok(Some(document)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    fn save(&self, document: &str) -> Result<(), String> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| "preferences path has no parent directory".to_owned())?;
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        std::fs::write(&self.path, document).map_err(|error| error.to_string())
    }
}

/// Resolves a stable per-user application-data location without consulting
/// the process working directory.
#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
fn native_preferences_path() -> Result<std::path::PathBuf, String> {
    std::env::var_os("APPDATA")
        .map(std::path::PathBuf::from)
        .map(|root| root.join("Last Aeon").join(PREFERENCES_FILENAME))
        .ok_or_else(|| "APPDATA is unavailable".to_owned())
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "macos"))]
fn native_preferences_path() -> Result<std::path::PathBuf, String> {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .map(|root| {
            root.join("Library")
                .join("Application Support")
                .join("Last Aeon")
                .join(PREFERENCES_FILENAME)
        })
        .ok_or_else(|| "HOME is unavailable".to_owned())
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "windows"),
    not(target_os = "macos")
))]
fn native_preferences_path() -> Result<std::path::PathBuf, String> {
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .map(|home| home.join(".config"))
        })
        .ok_or_else(|| "neither XDG_CONFIG_HOME nor HOME is available".to_owned())?;
    Ok(root.join("last-aeon").join(PREFERENCES_FILENAME))
}

#[cfg(target_arch = "wasm32")]
struct BrowserBackend;

#[cfg(target_arch = "wasm32")]
impl BrowserBackend {
    fn storage() -> Result<web_sys::Storage, String> {
        web_sys::window()
            .ok_or_else(|| "browser window unavailable".to_owned())?
            .local_storage()
            .map_err(|_| "browser local storage could not be queried".to_owned())?
            .ok_or_else(|| "browser local storage unavailable".to_owned())
    }
}

#[cfg(target_arch = "wasm32")]
impl KeyValueBackend for BrowserBackend {
    fn get(&self, key: &str) -> Result<Option<String>, String> {
        Self::storage()?
            .get_item(key)
            .map_err(|_| "browser preferences could not be read".to_owned())
    }

    fn set(&self, key: &str, document: &str) -> Result<(), String> {
        Self::storage()?
            .set_item(key, document)
            .map_err(|_| "browser preferences could not be written".to_owned())
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn load_platform() -> UiPreferences {
    NativeStore::for_current_user()
        .map(|store| load_from(&store))
        .unwrap_or_default()
}

#[cfg(target_arch = "wasm32")]
fn load_platform() -> UiPreferences {
    load_from(&KeyValueStore {
        backend: BrowserBackend,
        key: WEB_STORAGE_KEY,
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn save_platform(preferences: UiPreferences) -> Result<(), String> {
    save_to(&NativeStore::for_current_user()?, preferences)
}

#[cfg(target_arch = "wasm32")]
fn save_platform(preferences: UiPreferences) -> Result<(), String> {
    save_to(
        &KeyValueStore {
            backend: BrowserBackend,
            key: WEB_STORAGE_KEY,
        },
        preferences,
    )
}

/// Loads preferences once. Missing, corrupt, inaccessible, or future-version
/// documents fail softly to the presentation-preserving defaults.
pub fn load_preferences(mut preferences: ResMut<UiPreferences>) {
    *preferences = load_platform();
}

/// Persists a changed preference document without touching campaign state.
pub fn persist_preferences(
    preferences: Res<UiPreferences>,
    mut last_attempt: Local<Option<UiPreferences>>,
) {
    if *last_attempt == Some(*preferences) {
        return;
    }
    if let Err(error) = save_platform(*preferences) {
        warn!("interface preferences could not be saved: {error}");
    }
    *last_attempt = Some(*preferences);
}

/// Applies scale and density after the authored theme has established its
/// baseline. Called for every egui style variant.
pub fn zoom_factor(preferences: UiPreferences) -> f32 {
    preferences.scale.factor()
}

/// Applies density to one egui style variant.
pub fn apply_to_style(preferences: UiPreferences, style: &mut egui::Style) {
    preferences.density.apply(style);
}

/// Draws the same settings controls on any presentation surface.
pub fn draw_controls(
    ui: &mut egui::Ui,
    strings: &aeon_sim::TextDb,
    preferences: &mut UiPreferences,
) {
    ui.label(strings.text("ui.preferences.scale"));
    let scale_combo = egui::ComboBox::from_id_salt("ui-preference-scale")
        .selected_text(strings.text(preferences.scale.label_key()))
        .show_ui(ui, |ui| {
            for scale in UiScale::ALL {
                let response = ui.selectable_value(
                    &mut preferences.scale,
                    scale,
                    strings.text(scale.label_key()),
                );
                crate::ui::keyboard::capture_action(
                    ui,
                    crate::ui::keyboard::LogicalFocus::new(format!("preference-scale:{scale:?}")),
                    "preference-scale-choice",
                    crate::ui::keyboard::FocusBand::Floating,
                    &response,
                )
                .register();
            }
        });
    crate::ui::keyboard::capture_action(
        ui,
        crate::ui::keyboard::LogicalFocus::new("preference-scale"),
        "preference-scale",
        crate::ui::keyboard::FocusBand::Floating,
        &scale_combo.response,
    )
    .register();
    ui.label(strings.text("ui.preferences.density"));
    let density_combo = egui::ComboBox::from_id_salt("ui-preference-density")
        .selected_text(strings.text(preferences.density.label_key()))
        .show_ui(ui, |ui| {
            for density in UiDensity::ALL {
                let response = ui.selectable_value(
                    &mut preferences.density,
                    density,
                    strings.text(density.label_key()),
                );
                crate::ui::keyboard::capture_action(
                    ui,
                    crate::ui::keyboard::LogicalFocus::new(format!(
                        "preference-density:{density:?}"
                    )),
                    "preference-density-choice",
                    crate::ui::keyboard::FocusBand::Floating,
                    &response,
                )
                .register();
            }
        });
    crate::ui::keyboard::capture_action(
        ui,
        crate::ui::keyboard::LogicalFocus::new("preference-density"),
        "preference-density",
        crate::ui::keyboard::FocusBand::Floating,
        &density_combo.response,
    )
    .register();
    ui.weak(strings.text("ui.preferences.note"));
}

/// Draws the campaign settings surface.
pub fn draw_campaign_settings(
    ctx: &egui::Context,
    strings: &aeon_sim::TextDb,
    preferences: &mut UiPreferences,
    settings: &mut SettingsUi,
) {
    if !settings.open {
        return;
    }
    egui::Window::new(strings.text("ui.preferences.title"))
        .resizable(false)
        .show(ctx, |ui| {
            draw_controls(ui, strings, preferences);
            ui.separator();
            let response = ui.button(strings.text("ui.preferences.close"));
            crate::ui::keyboard::capture_action(
                ui,
                crate::ui::keyboard::LogicalFocus::new("settings-close"),
                "settings-close",
                crate::ui::keyboard::FocusBand::Floating,
                &response,
            )
            .register();
            if response.clicked() {
                settings.close(ui.ctx());
            }
        });
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    #[cfg(not(target_arch = "wasm32"))]
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use aeon_core::calendar::CalendarDate;
    use aeon_sim::{CampaignConfig, PlayerCommand, SimHost, persistence};

    #[derive(Default)]
    struct MemoryStore(RefCell<Option<String>>);

    impl PreferenceStore for MemoryStore {
        fn load(&self) -> Result<Option<String>, String> {
            Ok(self.0.borrow().clone())
        }

        fn save(&self, document: &str) -> Result<(), String> {
            *self.0.borrow_mut() = Some(document.to_owned());
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct MemoryKeyValue(std::rc::Rc<RefCell<BTreeMap<String, String>>>);

    impl KeyValueBackend for MemoryKeyValue {
        fn get(&self, key: &str) -> Result<Option<String>, String> {
            Ok(self.0.borrow().get(key).cloned())
        }

        fn set(&self, key: &str, value: &str) -> Result<(), String> {
            self.0.borrow_mut().insert(key.to_owned(), value.to_owned());
            Ok(())
        }
    }

    struct InaccessibleKeyValue;

    impl KeyValueBackend for InaccessibleKeyValue {
        fn get(&self, _key: &str) -> Result<Option<String>, String> {
            Err("storage unavailable".to_owned())
        }

        fn set(&self, _key: &str, _value: &str) -> Result<(), String> {
            Err("storage unavailable".to_owned())
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    struct TestDirectory(std::path::PathBuf);

    #[cfg(not(target_arch = "wasm32"))]
    impl TestDirectory {
        fn new(label: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "last-aeon-preferences-{label}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).expect("isolated test directory is created");
            Self(path)
        }

        fn join(&self, path: &str) -> std::path::PathBuf {
            self.0.join(path)
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn defaults_preserve_the_existing_presentation() {
        let preferences = UiPreferences::default();
        assert_eq!(preferences.scale, UiScale::Percent100);
        assert_eq!(preferences.density, UiDensity::Compact);
    }

    #[test]
    fn every_supported_choice_round_trips_through_the_storage_contract() {
        for scale in UiScale::ALL {
            for density in UiDensity::ALL {
                let expected = UiPreferences { scale, density };
                let store = MemoryStore::default();
                save_to(&store, expected).expect("memory store accepts the document");
                assert_eq!(load_from(&store), expected);
            }
        }
    }

    #[test]
    fn corrupt_and_future_documents_fail_softly() {
        for text in [
            "not json",
            r#"{"version":2,"ui_scale":"percent200","density":"comfortable"}"#,
            r#"{"version":1,"ui_scale":"enormous","density":"comfortable"}"#,
            r#"{"version":1,"ui_scale":"percent200","density":"spacious"}"#,
        ] {
            let store = MemoryStore(RefCell::new(Some(text.to_owned())));
            assert_eq!(load_from(&store), UiPreferences::default());
        }
    }

    #[test]
    fn browser_shaped_adapter_uses_the_stable_key_and_shared_codec() {
        let backend = MemoryKeyValue::default();
        let store = KeyValueStore {
            backend: backend.clone(),
            key: WEB_STORAGE_KEY,
        };
        let expected = UiPreferences {
            scale: UiScale::Percent200,
            density: UiDensity::Comfortable,
        };
        assert_eq!(load_from(&store), UiPreferences::default());
        save_to(&store, expected).expect("key/value backend accepts preferences");
        assert_eq!(load_from(&store), expected);
        assert!(backend.0.borrow().contains_key(WEB_STORAGE_KEY));
    }

    #[test]
    fn browser_shaped_adapter_fails_softly_for_bad_or_inaccessible_storage() {
        let backend = MemoryKeyValue::default();
        let store = KeyValueStore {
            backend: backend.clone(),
            key: WEB_STORAGE_KEY,
        };
        for document in [
            "not json",
            r#"{"version":99,"ui_scale":"percent100","density":"compact"}"#,
            r#"{"version":1,"ui_scale":"unknown","density":"compact"}"#,
        ] {
            backend
                .set(WEB_STORAGE_KEY, document)
                .expect("memory backend is writable");
            assert_eq!(load_from(&store), UiPreferences::default());
        }

        let inaccessible = KeyValueStore {
            backend: InaccessibleKeyValue,
            key: WEB_STORAGE_KEY,
        };
        assert_eq!(load_from(&inaccessible), UiPreferences::default());
        assert!(save_to(&inaccessible, UiPreferences::default()).is_err());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_store_missing_document_uses_defaults() {
        let directory = TestDirectory::new("missing");
        let store = NativeStore::at(directory.join("nested/preferences.json"));
        assert_eq!(load_from(&store), UiPreferences::default());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_store_creates_parent_directories_and_reloads() {
        let directory = TestDirectory::new("round-trip");
        let path = directory.join("application/config/preferences.json");
        let expected = UiPreferences {
            scale: UiScale::Percent150,
            density: UiDensity::Comfortable,
        };
        save_to(&NativeStore::at(&path), expected).expect("native preferences are written");
        assert!(path.is_file(), "the versioned document was created");
        assert_eq!(load_from(&NativeStore::at(path)), expected);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_store_bad_documents_fail_softly() {
        let directory = TestDirectory::new("bad-documents");
        let store = NativeStore::at(directory.join("preferences.json"));
        for document in [
            "not json",
            r#"{"version":2,"ui_scale":"percent100","density":"compact"}"#,
            r#"{"version":1,"ui_scale":"unknown","density":"compact"}"#,
        ] {
            store.save(document).expect("test document is written");
            assert_eq!(load_from(&store), UiPreferences::default());
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_store_read_and_write_failures_are_non_fatal() {
        let directory = TestDirectory::new("inaccessible");

        let directory_at_file_path = directory.join("directory-not-document");
        std::fs::create_dir(&directory_at_file_path).expect("test directory is created");
        assert_eq!(
            load_from(&NativeStore::at(directory_at_file_path)),
            UiPreferences::default()
        );

        let parent_is_file = directory.join("parent-is-file");
        std::fs::write(&parent_is_file, "block parent creation").expect("blocker is written");
        let unwritable = NativeStore::at(parent_is_file.join("preferences.json"));
        assert!(save_to(&unwritable, UiPreferences::default()).is_err());
    }

    #[test]
    fn persisting_preferences_cannot_change_authoritative_artifacts() {
        let start_date = CalendarDate {
            year: 411,
            month: 1,
            day: 1,
        }
        .to_date()
        .expect("valid test date");
        let mut host = SimHost::new(CampaignConfig {
            name: "Preference Isolation".to_owned(),
            seed: 91,
            start_date,
        });
        host.submit(PlayerCommand::Noop)
            .expect("ordinary command is accepted");
        host.advance_days(1);

        let snapshot_before = persistence::snapshot_to_ron(&host.snapshot())
            .expect("authoritative snapshot serialises");
        let hash_before = host.state_hash();
        let mut log_before = Vec::new();
        persistence::write_command_log(&mut log_before, &host.applied_commands())
            .expect("command log serialises");

        let preferences = UiPreferences {
            scale: UiScale::Percent200,
            density: UiDensity::Comfortable,
        };
        let store = MemoryStore::default();
        save_to(&store, preferences).expect("presentation preference persists");
        assert_eq!(load_from(&store), preferences);

        assert_eq!(host.state_hash(), hash_before);
        assert_eq!(
            persistence::snapshot_to_ron(&host.snapshot())
                .expect("authoritative snapshot still serialises"),
            snapshot_before
        );
        let mut log_after = Vec::new();
        persistence::write_command_log(&mut log_after, &host.applied_commands())
            .expect("command log still serialises");
        assert_eq!(log_after, log_before);
    }

    #[test]
    fn comfortable_density_changes_measurements_but_not_visibility() {
        let mut compact = egui::Style::default();
        let mut comfortable = compact.clone();
        UiDensity::Compact.apply(&mut compact);
        UiDensity::Comfortable.apply(&mut comfortable);
        assert!(comfortable.spacing.item_spacing.y > compact.spacing.item_spacing.y);
        assert!(comfortable.spacing.interact_size.y > compact.spacing.interact_size.y);
        assert_eq!(comfortable.visuals, compact.visuals);
    }

    #[cfg(target_arch = "wasm32")]
    #[test]
    fn browser_adapter_implements_the_shared_contract() {
        fn accepts_backend(_: &impl KeyValueBackend) {}
        accepts_backend(&BrowserBackend);
        assert!(!WEB_STORAGE_KEY.is_empty());
    }
}
