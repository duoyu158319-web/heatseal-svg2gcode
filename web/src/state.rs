use std::{convert::TryInto, num::ParseFloatError};

use serde::{Deserialize, Serialize};
use svg2gcode::config::{
    ConversionConfig, MachineConfig, PostprocessConfig, Settings, SupportedFunctionality, Version,
};
use svgtypes::Length;
use thiserror::Error;
use yewdux::store::Store;

#[derive(Debug, Clone, PartialEq, Store)]
#[store]
pub struct FormState {
    pub tolerance: Result<f64, ParseFloatError>,
    pub feedrate: Result<f64, ParseFloatError>,
    pub origin: [Option<Result<f64, ParseFloatError>>; 2],
    pub circular_interpolation: bool,
    pub optimize_path_order: bool,
    pub dpi: Result<f64, ParseFloatError>,
    pub tool_on_sequence: Option<Result<String, String>>,
    pub tool_off_sequence: Option<Result<String, String>>,
    pub begin_sequence: Option<Result<String, String>>,
    pub end_sequence: Option<Result<String, String>>,
    pub checksums: bool,
    pub line_numbers: bool,
    pub newline_before_comment: bool,
    pub starting_point: [Option<Result<f64, ParseFloatError>>; 2],
    pub dwell_seconds: Result<f64, String>,
    pub temperature: Result<f64, String>,
    pub working_height: Result<f64, String>,
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
    #[error("could not parse gcode: {0}")]
    GCode(String),
}

impl TryInto<Settings> for &FormState {
    type Error = FormStateConversionError;

    fn try_into(self) -> Result<Settings, Self::Error> {
        Ok(Settings {
            conversion: svg2gcode::config::GCodeConfig {
                inner: ConversionConfig {
                    dpi: self.dpi.clone()?,
                    origin: [
                        self.origin[0].clone().transpose()?,
                        self.origin[1].clone().transpose()?,
                    ],
                    extra_attribute_name: None,
                    optimize_path_order: self.optimize_path_order,
                    selector_filter: None,
                    starting_point: [
                        self.starting_point[0].clone().transpose()?,
                        self.starting_point[1].clone().transpose()?,
                    ],
                },
                tolerance: self.tolerance.clone()?,
                feedrate: self.feedrate.clone()?,
            },
            machine: MachineConfig {
                supported_functionality: SupportedFunctionality {
                    circular_interpolation: self.circular_interpolation,
                },
                tool_on_sequence: self
                    .tool_on_sequence
                    .clone()
                    .transpose()
                    .map_err(FormStateConversionError::GCode)?,
                tool_off_sequence: self
                    .tool_off_sequence
                    .clone()
                    .transpose()
                    .map_err(FormStateConversionError::GCode)?,
                begin_sequence: self
                    .begin_sequence
                    .clone()
                    .transpose()
                    .map_err(FormStateConversionError::GCode)?,
                end_sequence: self
                    .end_sequence
                    .clone()
                    .transpose()
                    .map_err(FormStateConversionError::GCode)?,
            },
            postprocess: PostprocessConfig {
                checksums: self.checksums,
                line_numbers: self.line_numbers,
                newline_before_comment: self.newline_before_comment,
            },
            version: Version::latest(),
        })
    }
}

impl From<&Settings> for FormState {
    fn from(settings: &Settings) -> Self {
        Self {
            tolerance: Ok(settings.conversion.tolerance),
            feedrate: Ok(settings.conversion.feedrate),
            circular_interpolation: settings
                .machine
                .supported_functionality
                .circular_interpolation,
            optimize_path_order: settings.conversion.inner.optimize_path_order,
            origin: [
                settings.conversion.inner.origin[0].map(Ok),
                settings.conversion.inner.origin[1].map(Ok),
            ],
            dpi: Ok(settings.conversion.inner.dpi),
            tool_on_sequence: settings.machine.tool_on_sequence.clone().map(Ok),
            tool_off_sequence: settings.machine.tool_off_sequence.clone().map(Ok),
            begin_sequence: settings.machine.begin_sequence.clone().map(Ok),
            end_sequence: settings.machine.end_sequence.clone().map(Ok),
            checksums: settings.postprocess.checksums,
            line_numbers: settings.postprocess.line_numbers,
            newline_before_comment: settings.postprocess.newline_before_comment,
            starting_point: [
                settings.conversion.inner.starting_point[0].map(Ok),
                settings.conversion.inner.starting_point[1].map(Ok),
            ],
            dwell_seconds: Ok(HeatSealSettings::default().dwell_seconds),
            temperature: Ok(HeatSealSettings::default().temperature),
            working_height: Ok(HeatSealSettings::default().working_height),
        }
    }
}

impl FormState {
    pub fn from_app(settings: &Settings, heat_seal: &HeatSealSettings) -> Self {
        Self {
            dwell_seconds: HeatSealSettings::validate_dwell(heat_seal.dwell_seconds),
            temperature: HeatSealSettings::validate_temperature(heat_seal.temperature),
            working_height: HeatSealSettings::validate_height(heat_seal.working_height),
            ..Self::from(settings)
        }
    }

    pub fn heat_seal_settings(&self) -> HeatSealSettings {
        HeatSealSettings {
            dwell_seconds: *self.dwell_seconds.as_ref().unwrap(),
            temperature: *self.temperature.as_ref().unwrap(),
            working_height: *self.working_height.as_ref().unwrap(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HeatSealSettings {
    pub dwell_seconds: f64,
    pub temperature: f64,
    pub working_height: f64,
}

impl Default for HeatSealSettings {
    fn default() -> Self {
        Self {
            dwell_seconds: 120.,
            temperature: 230.,
            working_height: 1.2,
        }
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
        requirement: &'static str,
    ) -> Result<f64, String> {
        if value.is_finite() && valid(value) {
            Ok(value)
        } else {
            Err(requirement.to_owned())
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        Self::validate_dwell(self.dwell_seconds)?;
        Self::validate_temperature(self.temperature)?;
        Self::validate_height(self.working_height)?;
        Ok(())
    }
}

pub const WEB_SETTINGS_VERSION: u32 = 1;

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
            CompatibleSettings::Web(settings) => settings,
            CompatibleSettings::Legacy(settings) => {
                WebSettings::new(settings, HeatSealSettings::default())
            }
        })
    }
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
            },
        );
        let json = serde_json::to_vec(&wrapper).unwrap();
        assert_eq!(WebSettings::from_json_slice(&json).unwrap(), wrapper);
    }

    #[test]
    fn legacy_settings_import_uses_default_heat_seal_values() {
        let legacy = Settings::default();
        let json = serde_json::to_vec(&legacy).unwrap();
        let imported = WebSettings::from_json_slice(&json).unwrap();
        assert_eq!(imported.settings, legacy);
        assert_eq!(imported.heat_seal, HeatSealSettings::default());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Store)]
#[store(storage = "local", storage_tab_sync)]
pub struct AppState {
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
