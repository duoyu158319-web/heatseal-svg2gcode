use std::num::ParseFloatError;

use paste::paste;
use yew::prelude::*;
use yewdux::functional::{use_store, use_store_value};

use crate::{
    state::{AppState, FormState, HeatSealSettings, OuterFrameSettings},
    ui::*,
};

macro_rules! form_input {
    ($($name: ident {
        $label: literal,
        $desc: literal,
        $form_accessor: ident,
        $app_accessor: expr,
    })*) => {
        $(
            paste! {
                #[function_component([<$name Input>])]
                pub fn [<$name:snake:lower _input>]() -> Html {
                    let app_state = use_store_value::<AppState>();
                    let (form_state, form_dispatch) = use_store::<FormState>();
                    let oninput = form_dispatch.reduce_mut_callback_with(|state, event: InputEvent| {
                        let value = event.target_unchecked_into::<web_sys::HtmlInputElement>().value();
                        state.$form_accessor = value.parse::<f64>();
                    });
                    html! {
                        <FormGroup success={form_state.$form_accessor.is_ok()}>
                            <Input<f64, ParseFloatError>
                                label=$label
                                desc=$desc
                                default={app_state.$app_accessor}
                                parsed={form_state.$form_accessor.clone()}
                                oninput={oninput}
                            />
                        </FormGroup>
                    }
                }
            }
        )*
    };
}

form_input! {
    Tolerance {
        "Curve tolerance (mm)",
        "Lower values follow curves more closely but generate more G1 coordinate lines",
        tolerance,
        settings.conversion.tolerance,
    }
    Feedrate {
        "Drawing feedrate (mm/min)",
        "Sets the F value for outer-frame edges and SVG drawing commands",
        feedrate,
        settings.conversion.feedrate,
    }
    Dpi {
        "DPI",
        "Controls conversion of visual SVG units such as px, pt, and pc into millimeters",
        dpi,
        settings.conversion.inner.dpi,
    }
}

fn parse_heat_value(
    value: &str,
    validate: impl FnOnce(f64) -> Result<f64, String>,
) -> Result<f64, String> {
    value
        .parse::<f64>()
        .map_err(|err| err.to_string())
        .and_then(validate)
}

macro_rules! simple_heat_input {
    ($name:ident, $label:literal, $desc:literal, $field:ident, $default:expr, $validate:expr) => {
        paste! {
            #[function_component([<$name Input>])]
            pub fn [<$name:snake:lower _input>]() -> Html {
                let app_state = use_store_value::<AppState>();
                let (form_state, form_dispatch) = use_store::<FormState>();
                let oninput = form_dispatch.reduce_mut_callback_with(|state, event: InputEvent| {
                    let value = event
                        .target_unchecked_into::<web_sys::HtmlInputElement>()
                        .value();
                    state.$field = parse_heat_value(&value, $validate);
                });
                html! {
                    <FormGroup success={form_state.$field.is_ok()}>
                        <Input<f64, String>
                            label=$label
                            desc=$desc
                            default={app_state.$default}
                            parsed={form_state.$field.clone()}
                            oninput={oninput}
                        />
                    </FormGroup>
                }
            }
        }
    };
}

simple_heat_input!(
    DwellSeconds,
    "Dwell time (s)",
    "Sets G4 S for the outer frame and every SVG heat-seal cycle",
    dwell_seconds,
    heat_seal.dwell_seconds,
    HeatSealSettings::validate_dwell
);

#[function_component(SvgTemperatureInput)]
pub fn svg_temperature_input() -> Html {
    let app_state = use_store_value::<AppState>();
    let (form_state, form_dispatch) = use_store::<FormState>();
    let oninput = form_dispatch.reduce_mut_callback_with(|state, event: InputEvent| {
        let value = event
            .target_unchecked_into::<web_sys::HtmlInputElement>()
            .value();
        let parsed = parse_heat_value(&value, HeatSealSettings::validate_temperature);
        state.temperature = parsed.clone();
        state.outer_frame_temperature = parsed;
        state.outer_profile_sync_revision = state.outer_profile_sync_revision.wrapping_add(1);
    });
    html! {
        <FormGroup success={form_state.temperature.is_ok()}>
            <Input<f64, String>
                label="SVG temperature"
                desc="Sets M104 S for every imported SVG heat-seal cycle and synchronizes to the outer-frame temperature"
                default={app_state.heat_seal.temperature}
                parsed={form_state.temperature.clone()}
                oninput={oninput}
            />
        </FormGroup>
    }
}

#[function_component(SvgWorkingHeightInput)]
pub fn svg_working_height_input() -> Html {
    let app_state = use_store_value::<AppState>();
    let (form_state, form_dispatch) = use_store::<FormState>();
    let oninput = form_dispatch.reduce_mut_callback_with(|state, event: InputEvent| {
        let value = event
            .target_unchecked_into::<web_sys::HtmlInputElement>()
            .value();
        let parsed = parse_heat_value(&value, HeatSealSettings::validate_height);
        state.working_height = parsed.clone();
        state.outer_frame_working_height = parsed;
        state.outer_profile_sync_revision = state.outer_profile_sync_revision.wrapping_add(1);
    });
    html! {
        <FormGroup success={form_state.working_height.is_ok()}>
            <Input<f64, String>
                label="SVG working height (mm)"
                desc="Sets G1 Z for every imported SVG heat-seal cycle and synchronizes to the outer-frame height"
                default={app_state.heat_seal.working_height}
                parsed={form_state.working_height.clone()}
                oninput={oninput}
            />
        </FormGroup>
    }
}

#[function_component(OuterFrameWidthInput)]
pub fn outer_frame_width_input() -> Html {
    let (form_state, form_dispatch) = use_store::<FormState>();
    let oninput = form_dispatch.reduce_mut_callback_with(|state, event: InputEvent| {
        let value = event
            .target_unchecked_into::<web_sys::HtmlInputElement>()
            .value();
        let machine_model = state.machine_model;
        state.outer_frame_width = parse_heat_value(&value, |value| {
            OuterFrameSettings::validate_width(value, machine_model)
        });
    });
    html! {
        <FormGroup success={form_state.outer_frame_width.is_ok()}>
            <Input<f64, String>
                label="Outer-frame width (mm)"
                desc="Centered at fixed G-code X127.970"
                default={form_state.outer_frame_width.as_ref().ok().copied()}
                parsed={form_state.outer_frame_width.clone()}
                r#type={InputType::Number}
                min={0.1}
                max={form_state.machine_model.width_mm()}
                step={0.1}
                oninput={oninput}
            />
        </FormGroup>
    }
}

#[function_component(OuterFrameHeightInput)]
pub fn outer_frame_height_input() -> Html {
    let (form_state, form_dispatch) = use_store::<FormState>();
    let oninput = form_dispatch.reduce_mut_callback_with(|state, event: InputEvent| {
        let value = event
            .target_unchecked_into::<web_sys::HtmlInputElement>()
            .value();
        let machine_model = state.machine_model;
        state.outer_frame_height = parse_heat_value(&value, |value| {
            OuterFrameSettings::validate_height(value, machine_model)
        });
    });
    html! {
        <FormGroup success={form_state.outer_frame_height.is_ok()}>
            <Input<f64, String>
                label="Outer-frame height (mm)"
                desc="Centered at fixed G-code Y127.970"
                default={form_state.outer_frame_height.as_ref().ok().copied()}
                parsed={form_state.outer_frame_height.clone()}
                r#type={InputType::Number}
                min={0.1}
                max={form_state.machine_model.height_mm()}
                step={0.1}
                oninput={oninput}
            />
        </FormGroup>
    }
}

#[function_component(OuterFrameTemperatureInput)]
pub fn outer_frame_temperature_input() -> Html {
    let (form_state, form_dispatch) = use_store::<FormState>();
    let oninput = form_dispatch.reduce_mut_callback_with(|state, event: InputEvent| {
        let value = event
            .target_unchecked_into::<web_sys::HtmlInputElement>()
            .value();
        state.outer_frame_temperature =
            parse_heat_value(&value, HeatSealSettings::validate_temperature);
    });
    html! {
        <FormGroup success={form_state.outer_frame_temperature.is_ok()}>
            <Input<f64, String>
                label="Outer-frame temperature"
                desc="Sets M104 S for the outer-frame cycle. Changes here do not affect the SVG temperature"
                default={form_state.outer_frame_temperature.as_ref().ok().copied()}
                parsed={form_state.outer_frame_temperature.clone()}
                oninput={oninput}
            />
        </FormGroup>
    }
}

#[function_component(OuterFrameWorkingHeightInput)]
pub fn outer_frame_working_height_input() -> Html {
    let (form_state, form_dispatch) = use_store::<FormState>();
    let oninput = form_dispatch.reduce_mut_callback_with(|state, event: InputEvent| {
        let value = event
            .target_unchecked_into::<web_sys::HtmlInputElement>()
            .value();
        state.outer_frame_working_height =
            parse_heat_value(&value, HeatSealSettings::validate_height);
    });
    html! {
        <FormGroup success={form_state.outer_frame_working_height.is_ok()}>
            <Input<f64, String>
                label="Outer-frame working height (mm)"
                desc="Sets G1 Z for the outer-frame cycle. Changes here do not affect the SVG working height"
                default={form_state.outer_frame_working_height.as_ref().ok().copied()}
                parsed={form_state.outer_frame_working_height.clone()}
                oninput={oninput}
            />
        </FormGroup>
    }
}
