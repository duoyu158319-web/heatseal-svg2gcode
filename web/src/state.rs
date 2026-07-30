use std::{convert::TryInto, num::ParseFloatError};

use serde::{Deserialize, Serialize};
use svg2gcode::config::{ConversionConfig, MachineConfig, PostprocessConfig, Settings, Version};
use svgtypes::Length;
use thiserror::Error;
use yewdux::store::Store;

pub const DEFAULT_OUTER_FRAME_SIZE_MM: f64 = 150.;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum MachineModel {
    #[default]
    A1,
    A2L,
}

impl MachineModel {
    pub const fn label(self) -> &'static str {
        match self {
            Self::A1 => "A1",
            Self::A2L => "A2L",
        }
    }

    pub const fn width_mm(self) -> f64 {
        match self {
            Self::A1 => 256.,
            Self::A2L => 330.,
        }
    }

    pub const fn height_mm(self) -> f64 {
        match self {
            Self::A1 => 256.,
            Self::A2L => 320.,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Store)]
#[store]
pub struct FormState {
    pub tolerance: Result<f64, ParseFloatError>,
    pub feedrate: Result<f64, ParseFloatError>,
    pub dpi: Result<f64, ParseFloatError>,
    pub dwell_seconds: Result<f64, String>,
    pub temperature: Result<f64, String>,
    pub working_height: Result<f64, String>,
    pub auto_center_svg: bool,
    pub machine_model: MachineModel,
    pub outer_frame_enabled: bool,
    pub outer_frame_width: Result<f64, String>,
    pub outer_frame_height: Result<f64, String>,
    pub outer_frame_temperature: Result<f64, String>,
    pub outer_frame_working_height: Result<f64, String>,
    pub outer_profile_sync_revision: u32,
}

impl Default for FormState {
    fn default() -> Self {
        let app_state = AppState::default();
        Self::from_app(&app_state.settings, &app_state.heat_seal)
    }
}

#[derive(Debug, Error)]
pub enum FormStateConversionError {
    #[error(transparent)]
    Float(#[from] ParseFloatError),
}

impl TryInto<Settings> for &FormState {
    type Error = FormStateConversionError;

    fn try_into(self) -> Result<Settings, Self::Error> {
        Ok(Settings {
            conversion: svg2gcode::config::GCodeConfig {
                inner: ConversionConfig {
                    dpi: self.dpi.clone()?,
                    origin: [Some(0.), Some(0.)],
                    extra_attribute_name: None,
                    optimize_path_order: false,
                    selector_filter: None,
                    starting_point: [Some(0.), Some(0.)],
                },
                tolerance: self.tolerance.clone()?,
                feedrate: self.feedrate.clone()?,
            },
            machine: MachineConfig::default(),
            postprocess: PostprocessConfig::default(),
            version: Version::latest(),
        })
    }
}

impl From<&Settings> for FormState {
    fn from(settings: &Settings) -> Self {
        let heat_seal = HeatSealSettings::default();
        Self {
            tolerance: Ok(settings.conversion.tolerance),
            feedrate: Ok(settings.conversion.feedrate),
            dpi: Ok(settings.conversion.inner.dpi),
            dwell_seconds: Ok(heat_seal.dwell_seconds),
            temperature: Ok(heat_seal.temperature),
            working_height: Ok(heat_seal.working_height),
            auto_center_svg: heat_seal.auto_center_svg,
            machine_model: heat_seal.machine_model,
            outer_frame_enabled: heat_seal.outer_frame.enabled,
            outer_frame_width: Ok(heat_seal.outer_frame.width_mm),
            outer_frame_height: Ok(heat_seal.outer_frame.height_mm),
            outer_frame_temperature: Ok(heat_seal.outer_frame.temperature),
            outer_frame_working_height: Ok(heat_seal.outer_frame.working_height),
            outer_profile_sync_revision: 0,
        }
    }
}

impl FormState {
    pub fn from_app(settings: &Settings, heat_seal: &HeatSealSettings) -> Self {
        Self {
            dwell_seconds: HeatSealSettings::validate_dwell(heat_seal.dwell_seconds),
            temperature: HeatSealSettings::validate_temperature(heat_seal.temperature),
            working_height: HeatSealSettings::validate_height(heat_seal.working_height),
            auto_center_svg: heat_seal.auto_center_svg,
            machine_model: heat_seal.machine_model,
            outer_frame_enabled: heat_seal.outer_frame.enabled,
            outer_frame_width: OuterFrameSettings::validate_width(
                heat_seal.outer_frame.width_mm,
                heat_seal.machine_model,
            ),
            outer_frame_height: OuterFrameSettings::validate_height(
                heat_seal.outer_frame.height_mm,
                heat_seal.machine_model,
            ),
            outer_frame_temperature: HeatSealSettings::validate_temperature(
                heat_seal.outer_frame.temperature,
            ),
            outer_frame_working_height: HeatSealSettings::validate_height(
                heat_seal.outer_frame.working_height,
            ),
            outer_profile_sync_revision: 0,
            ..Self::from(settings)
        }
    }

    pub fn heat_seal_settings(&self) -> HeatSealSettings {
        HeatSealSettings {
            dwell_seconds: *self.dwell_seconds.as_ref().unwrap(),
            temperature: *self.temperature.as_ref().unwrap(),
            working_height: *self.working_height.as_ref().unwrap(),
            auto_center_svg: self.auto_center_svg,
            machine_model: self.machine_model,
            outer_frame: OuterFrameSettings {
                enabled: self.outer_frame_enabled,
                width_mm: *self.outer_frame_width.as_ref().unwrap(),
                height_mm: *self.outer_frame_height.as_ref().unwrap(),
                temperature: *self.outer_frame_temperature.as_ref().unwrap(),
                working_height: *self.outer_frame_working_height.as_ref().unwrap(),
            },
        }
    }

    pub fn is_valid(&self) -> bool {
        self.tolerance.is_ok()
            && self.feedrate.is_ok()
            && self.dpi.is_ok()
            && self.dwell_seconds.is_ok()
            && self.temperature.is_ok()
            && self.working_height.is_ok()
            && self.outer_frame_width.is_ok()
            && self.outer_frame_height.is_ok()
            && self.outer_frame_temperature.is_ok()
            && self.outer_frame_working_height.is_ok()
    }

    pub fn select_machine_model(&mut self, machine_model: MachineModel) {
        if self.machine_model != machine_model {
            self.machine_model = machine_model;
            self.outer_frame_width = Ok(DEFAULT_OUTER_FRAME_SIZE_MM);
            self.outer_frame_height = Ok(DEFAULT_OUTER_FRAME_SIZE_MM);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct HeatSealSettings {
    pub dwell_seconds: f64,
    pub temperature: f64,
    pub working_height: f64,
    pub auto_center_svg: bool,
    pub machine_model: MachineModel,
    pub outer_frame: OuterFrameSettings,
}

impl Default for HeatSealSettings {
    fn default() -> Self {
        Self {
            dwell_seconds: 120.,
            temperature: 230.,
            working_height: 0.12,
            auto_center_svg: false,
            machine_model: MachineModel::A1,
            outer_frame: OuterFrameSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct OuterFrameSettings {
    pub enabled: bool,
    pub width_mm: f64,
    pub height_mm: f64,
    pub temperature: f64,
    pub working_height: f64,
}

impl Default for OuterFrameSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            width_mm: DEFAULT_OUTER_FRAME_SIZE_MM,
            height_mm: DEFAULT_OUTER_FRAME_SIZE_MM,
            temperature: 230.,
            working_height: 0.12,
        }
    }
}

impl OuterFrameSettings {
    fn validate_dimension(
        value: f64,
        maximum: f64,
        dimension: &'static str,
        machine_model: MachineModel,
    ) -> Result<f64, String> {
        HeatSealSettings::validate_value(
            value,
            |value| value > 0. && value <= maximum,
            format!(
                "Outer-frame {dimension} must be greater than 0 and no more than {maximum} mm for {}",
                machine_model.label()
            ),
        )
    }

    pub fn validate_width(value: f64, machine_model: MachineModel) -> Result<f64, String> {
        Self::validate_dimension(value, machine_model.width_mm(), "width", machine_model)
    }

    pub fn validate_height(value: f64, machine_model: MachineModel) -> Result<f64, String> {
        Self::validate_dimension(value, machine_model.height_mm(), "height", machine_model)
    }

    pub fn validate(&self, machine_model: MachineModel) -> Result<(), String> {
        Self::validate_width(self.width_mm, machine_model)?;
        Self::validate_height(self.height_mm, machine_model)?;
        HeatSealSettings::validate_temperature(self.temperature)?;
        HeatSealSettings::validate_height(self.working_height)?;
        Ok(())
    }
}

impl HeatSealSettings {
    pub fn validate_dwell(value: f64) -> Result<f64, String> {
        Self::validate_value(
            value,
            |value| value >= 0.,
            "Dwell time must be a finite number greater than or equal to 0",
        )
    }

    pub fn validate_temperature(value: f64) -> Result<f64, String> {
        Self::validate_value(
            value,
            |value| (0. ..=300.).contains(&value),
            "Temperature must be a finite number from 0 to 300",
        )
    }

    pub fn validate_height(value: f64) -> Result<f64, String> {
        Self::validate_value(
            value,
            |value| value > 0.,
            "Working height must be a finite number greater than 0",
        )
    }

    fn validate_value(
        value: f64,
        valid: impl FnOnce(f64) -> bool,
        requirement: impl Into<String>,
    ) -> Result<f64, String> {
        if value.is_finite() && valid(value) {
            Ok(value)
        } else {
            Err(requirement.into())
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        Self::validate_dwell(self.dwell_seconds)?;
        Self::validate_temperature(self.temperature)?;
        Self::validate_height(self.working_height)?;
        self.outer_frame.validate(self.machine_model)?;
        Ok(())
    }

    fn migrate_from_web_v1(&mut self) {
        self.auto_center_svg = false;
        self.outer_frame = OuterFrameSettings {
            temperature: self.temperature,
            working_height: self.working_height,
            ..OuterFrameSettings::default()
        };
    }

    fn migrate_from_web_v2(&mut self) {
        self.machine_model = MachineModel::A1;
        if self.outer_frame.validate(self.machine_model).is_err() {
            self.outer_frame.width_mm = DEFAULT_OUTER_FRAME_SIZE_MM;
            self.outer_frame.height_mm = DEFAULT_OUTER_FRAME_SIZE_MM;
        }
    }
}

pub const WEB_SETTINGS_VERSION: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebSettings {
    pub version: u32,
    pub settings: Settings,
    #[serde(default)]
    pub heat_seal: HeatSealSettings,
}

impl WebSettings {
    pub fn new(settings: Settings, heat_seal: HeatSealSettings) -> Self {
        Self {
            version: WEB_SETTINGS_VERSION,
            settings,
            heat_seal,
        }
    }

    pub fn from_json_slice(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum CompatibleSettings {
            Web(WebSettings),
            Legacy(Settings),
        }

        serde_json::from_slice(bytes).map(|settings| match settings {
            CompatibleSettings::Web(mut settings) => {
                if settings.version < 2 {
                    settings.heat_seal.migrate_from_web_v1();
                }
                if settings.version < 3 {
                    settings.heat_seal.migrate_from_web_v2();
                }
                if settings.version < WEB_SETTINGS_VERSION {
                    settings.version = WEB_SETTINGS_VERSION;
                }
                settings
            }
            CompatibleSettings::Legacy(settings) => {
                WebSettings::new(settings, HeatSealSettings::default())
            }
        })
    }
}

pub fn normalize_settings_for_web_heat_seal(settings: &mut Settings) {
    let version = settings.version.clone();
    let tolerance = settings.conversion.tolerance;
    let feedrate = settings.conversion.feedrate;
    let dpi = settings.conversion.inner.dpi;
    *settings = Settings {
        conversion: svg2gcode::config::GCodeConfig {
            inner: ConversionConfig {
                dpi,
                origin: [Some(0.), Some(0.)],
                extra_attribute_name: None,
                optimize_path_order: false,
                selector_filter: None,
                starting_point: [Some(0.), Some(0.)],
            },
            tolerance,
            feedrate,
        },
        machine: MachineConfig::default(),
        postprocess: PostprocessConfig::default(),
        version,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_settings_round_trip_preserves_heat_seal_values() {
        let wrapper = WebSettings::new(
            Settings::default(),
            HeatSealSettings {
                dwell_seconds: 45.,
                temperature: 215.,
                working_height: 0.8,
                auto_center_svg: true,
                machine_model: MachineModel::A2L,
                outer_frame: OuterFrameSettings {
                    enabled: true,
                    width_mm: 150.,
                    height_mm: 140.,
                    temperature: 205.,
                    working_height: 0.6,
                },
            },
        );
        let json = serde_json::to_vec(&wrapper).unwrap();
        assert_eq!(WebSettings::from_json_slice(&json).unwrap(), wrapper);
    }

    #[test]
    fn web_v1_import_inherits_svg_profile_for_disabled_outer_frame() {
        let json = serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "settings": Settings::default(),
            "heat_seal": {
                "dwell_seconds": 45,
                "temperature": 215,
                "working_height": 0.8
            }
        }))
        .unwrap();
        let imported = WebSettings::from_json_slice(&json).unwrap();
        assert_eq!(imported.version, WEB_SETTINGS_VERSION);
        assert!(!imported.heat_seal.auto_center_svg);
        assert_eq!(imported.heat_seal.outer_frame.width_mm, 150.);
        assert_eq!(imported.heat_seal.outer_frame.height_mm, 150.);
        assert_eq!(imported.heat_seal.outer_frame.temperature, 215.);
        assert_eq!(imported.heat_seal.outer_frame.working_height, 0.8);
        assert!(!imported.heat_seal.outer_frame.enabled);
    }

    #[test]
    fn legacy_settings_import_uses_default_heat_seal_values() {
        let legacy = Settings::default();
        let json = serde_json::to_vec(&legacy).unwrap();
        let imported = WebSettings::from_json_slice(&json).unwrap();
        assert_eq!(imported.settings, legacy);
        assert_eq!(imported.heat_seal, HeatSealSettings::default());
    }

    #[test]
    fn web_v2_import_defaults_to_a1_and_resets_oversized_frame() {
        let json = serde_json::to_vec(&serde_json::json!({
            "version": 2,
            "settings": Settings::default(),
            "heat_seal": {
                "dwell_seconds": 120,
                "temperature": 230,
                "working_height": 0.12,
                "outer_frame": {
                    "enabled": true,
                    "width_mm": 300,
                    "height_mm": 300,
                    "temperature": 220,
                    "working_height": 0.2
                }
            }
        }))
        .unwrap();
        let imported = WebSettings::from_json_slice(&json).unwrap();
        assert_eq!(imported.version, WEB_SETTINGS_VERSION);
        assert_eq!(imported.heat_seal.machine_model, MachineModel::A1);
        assert_eq!(
            imported.heat_seal.outer_frame.width_mm,
            DEFAULT_OUTER_FRAME_SIZE_MM
        );
        assert_eq!(
            imported.heat_seal.outer_frame.height_mm,
            DEFAULT_OUTER_FRAME_SIZE_MM
        );
        assert_eq!(imported.heat_seal.outer_frame.temperature, 220.);
        assert_eq!(imported.heat_seal.outer_frame.working_height, 0.2);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Store)]
#[store(storage = "local", storage_tab_sync)]
pub struct AppState {
    #[serde(default)]
    pub storage_version: u32,
    pub first_visit: bool,
    pub settings: Settings,
    #[serde(default)]
    pub heat_seal: HeatSealSettings,
    #[serde(skip)]
    pub svgs: Vec<Svg>,
    #[serde(skip)]
    pub workflow_mode: WorkflowMode,
    #[serde(skip)]
    pub replacement_svg: Option<Svg>,
    #[serde(skip)]
    pub gcode_template: Option<GCodeTemplate>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Svg {
    pub content: String,
    pub filename: String,
    pub dimensions: [Option<Length>; 2],
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WorkflowMode {
    #[default]
    MergeSvg,
    ReplaceMarker,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GCodeTemplate {
    pub content: String,
    pub filename: String,
    pub had_utf8_bom: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            storage_version: APP_STATE_STORAGE_VERSION,
            first_visit: true,
            settings: Settings::default(),
            heat_seal: HeatSealSettings::default(),
            svgs: vec![],
            workflow_mode: WorkflowMode::default(),
            replacement_svg: None,
            gcode_template: None,
        }
    }
}

const APP_STATE_STORAGE_VERSION: u32 = 3;
const LEGACY_DEFAULT_WORKING_HEIGHT: f64 = 1.2;

impl AppState {
    pub fn migrate(&mut self) {
        if self.storage_version < 1 {
            if self.heat_seal.working_height == LEGACY_DEFAULT_WORKING_HEIGHT {
                self.heat_seal.working_height = HeatSealSettings::default().working_height;
            }
            self.storage_version = 1;
        }
        if self.storage_version < 2 {
            self.heat_seal.migrate_from_web_v1();
            self.storage_version = 2;
        }
        if self.storage_version < 3 {
            self.heat_seal.migrate_from_web_v2();
            self.storage_version = 3;
        }
        normalize_settings_for_web_heat_seal(&mut self.settings);
    }
}

#[cfg(test)]
mod app_state_tests {
    use super::*;

    #[test]
    fn heat_seal_default_working_height_is_point_twelve_mm() {
        assert_eq!(HeatSealSettings::default().working_height, 0.12);
        assert_eq!(HeatSealSettings::default().outer_frame.working_height, 0.12);
    }

    #[test]
    fn migrates_legacy_height_and_new_outer_frame_defaults() {
        let mut legacy_default = AppState::default();
        legacy_default.storage_version = 0;
        legacy_default.heat_seal.working_height = LEGACY_DEFAULT_WORKING_HEIGHT;
        legacy_default.migrate();
        assert_eq!(legacy_default.heat_seal.working_height, 0.12);
        assert_eq!(legacy_default.heat_seal.outer_frame.working_height, 0.12);
        assert!(!legacy_default.heat_seal.outer_frame.enabled);

        let mut custom = AppState::default();
        custom.storage_version = 1;
        custom.heat_seal.temperature = 212.;
        custom.heat_seal.working_height = 0.8;
        custom.migrate();
        assert_eq!(custom.heat_seal.outer_frame.temperature, 212.);
        assert_eq!(custom.heat_seal.outer_frame.working_height, 0.8);
    }

    #[test]
    fn changing_machine_model_always_resets_both_frame_dimensions() {
        let mut form = FormState::default();
        form.outer_frame_temperature = Ok(242.);
        form.outer_frame_working_height = Ok(0.2);
        form.outer_frame_width = Ok(200.);
        form.outer_frame_height = Ok(180.);
        form.select_machine_model(MachineModel::A2L);
        assert_eq!(form.machine_model, MachineModel::A2L);
        assert_eq!(form.outer_frame_width, Ok(DEFAULT_OUTER_FRAME_SIZE_MM));
        assert_eq!(form.outer_frame_height, Ok(DEFAULT_OUTER_FRAME_SIZE_MM));
        assert_eq!(form.outer_frame_temperature, Ok(242.));
        assert_eq!(form.outer_frame_working_height, Ok(0.2));

        form.outer_frame_width = Ok(300.);
        form.outer_frame_height = Ok(300.);
        form.select_machine_model(MachineModel::A1);
        assert_eq!(form.outer_frame_width, Ok(DEFAULT_OUTER_FRAME_SIZE_MM));
        assert_eq!(form.outer_frame_height, Ok(DEFAULT_OUTER_FRAME_SIZE_MM));
    }

    #[test]
    fn selecting_current_machine_model_does_not_reset_dimensions() {
        let mut form = FormState::default();
        form.outer_frame_width = Ok(200.);
        form.outer_frame_height = Ok(180.);
        form.select_machine_model(MachineModel::A1);
        assert_eq!(form.outer_frame_width, Ok(200.));
        assert_eq!(form.outer_frame_height, Ok(180.));
    }

    #[test]
    fn machine_specific_dimension_validation_uses_correct_limits() {
        assert!(OuterFrameSettings::validate_width(256., MachineModel::A1).is_ok());
        assert!(OuterFrameSettings::validate_width(256.1, MachineModel::A1).is_err());
        assert!(OuterFrameSettings::validate_width(330., MachineModel::A2L).is_ok());
        assert!(OuterFrameSettings::validate_height(320., MachineModel::A2L).is_ok());
        assert!(OuterFrameSettings::validate_height(320.1, MachineModel::A2L).is_err());
    }
}
