#![cfg_attr(not(test), deny(unused_crate_dependencies))]

use std::path::{Path, PathBuf};

use base64::Engine;
use getrandom as _; // activate wasm_js backend for wasm32-unknown-unknown
use log::Level;
use roxmltree::{Document, ParsingOptions};
use svg2star::{lower::ConversionOptions, turtle::elements::Stroke};
use yew::prelude::*;

mod forms;
mod heat_seal;
mod marker_replace;
mod state;
mod ui;
mod util;

use forms::*;
use heat_seal::{
    build_heat_seal_program, format_heat_seal_program, frame_fit_error, prepare_svg_strokes,
    strokes_to_preview,
};
use marker_replace::{replace_marker_line, with_original_utf8_bom};
use state::*;
use ui::*;
use util::*;
use yewdux::{YewduxRoot, prelude::use_store, use_dispatch};

fn replacement_output_path(filename: &str) -> PathBuf {
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("output");
    PathBuf::from(format!("{stem}_heatseal.gcode"))
}

fn prepared_strokes_for_svg(
    svg: &Svg,
    settings: &svg2gcode::config::Settings,
    heat_seal: &HeatSealSettings,
) -> Result<Vec<Stroke>, String> {
    let document = Document::parse_with_options(
        svg.content.as_str(),
        ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .map_err(|err| format!("Could not parse {}: {err}", svg.filename))?;
    let strokes = prepare_svg_strokes(
        &document,
        &settings.conversion,
        ConversionOptions {
            dimensions: svg.dimensions,
        },
        heat_seal.auto_center_svg,
    );
    if strokes.is_empty() {
        return Err(format!(
            "No effective toolpath was found in {}.",
            svg.filename
        ));
    }
    if let Some(error) = frame_fit_error(&strokes, heat_seal) {
        return Err(format!("{}：{error}", svg.filename));
    }
    Ok(strokes)
}

fn toolpath_preview(
    svg: &Svg,
    settings: &svg2gcode::config::Settings,
    heat_seal: &HeatSealSettings,
    include_outer_frame: bool,
) -> String {
    Document::parse_with_options(
        svg.content.as_str(),
        ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .ok()
    .map(|document| {
        prepare_svg_strokes(
            &document,
            &settings.conversion,
            ConversionOptions {
                dimensions: svg.dimensions,
            },
            heat_seal.auto_center_svg,
        )
    })
    .map(|strokes| strokes_to_preview(strokes, heat_seal, include_outer_frame))
    .unwrap_or_default()
}

#[function_component(App)]
fn app() -> Html {
    let generating = use_state_eq(|| false);
    let generating_setter = generating.setter();
    let generate_error = use_state_eq(|| Option::<String>::None);

    let form_dispatch = use_dispatch::<FormState>();
    let (app_store, app_dispatch) = use_store::<AppState>();

    // TODO: come up with a less awkward way to do this.
    // Having separate stores is somewhat of an anti-pattern in Redux,
    // but there's no easy way to do hydration after the app state is
    // restored from local storage.
    let upgraded_settings_and_hydrated_form = use_state(|| false);
    if !*upgraded_settings_and_hydrated_form {
        app_dispatch.reduce_mut(|app| {
            app.migrate();
            if app.settings.try_upgrade().is_err() {
                unreachable!("No breaking upgrades yet!")
            }
            normalize_settings_for_web_heat_seal(&mut app.settings);
            let hydrated_form_state = FormState::from_app(&app.settings, &app.heat_seal);
            form_dispatch.reduce_mut(|state| *state = hydrated_form_state);
        });
        upgraded_settings_and_hydrated_form.set(true);
    }

    let merge_validation_error = app_store.svgs.iter().find_map(|svg| {
        prepared_strokes_for_svg(svg, &app_store.settings, &app_store.heat_seal).err()
    });
    let marker_validation_error = app_store.gcode_template.as_ref().and_then(|template| {
        replace_marker_line(&template.content, "")
            .err()
            .map(|err| err.to_string())
    });
    let replacement_trajectory_error = app_store.replacement_svg.as_ref().and_then(|svg| {
        prepared_strokes_for_svg(svg, &app_store.settings, &app_store.heat_seal).err()
    });

    let merge_generate_disabled =
        *generating || app_store.svgs.is_empty() || merge_validation_error.is_some();
    let replacement_generate_disabled = *generating
        || app_store.replacement_svg.is_none()
        || app_store.gcode_template.is_none()
        || marker_validation_error.is_some()
        || replacement_trajectory_error.is_some();

    let merge_generate_onclick = {
        let app_store = app_store.clone();
        let generate_error = generate_error.clone();
        let generating_setter = generating_setter.clone();
        Callback::from(move |_| {
            generating_setter.set(true);
            generate_error.set(None);
            let mut merged_strokes = Vec::new();

            for svg in app_store.svgs.iter() {
                match prepared_strokes_for_svg(svg, &app_store.settings, &app_store.heat_seal) {
                    Ok(mut strokes) => merged_strokes.append(&mut strokes),
                    Err(error) => {
                        generate_error.set(Some(error));
                        generating_setter.set(false);
                        return;
                    }
                }
            }

            let program = build_heat_seal_program(
                merged_strokes,
                &app_store.settings.conversion,
                &app_store.heat_seal,
            );
            let output = format_heat_seal_program(&program).unwrap();
            let filepath = if app_store.svgs.len() == 1 {
                Path::new(app_store.svgs[0].filename.as_str()).with_extension("gcode")
            } else {
                PathBuf::from("svg2gcode_merged.gcode")
            };
            prompt_download(filepath, output.as_bytes());

            generating_setter.set(false);
        })
    };

    let replacement_generate_onclick = {
        let app_store = app_store.clone();
        let generate_error = generate_error.clone();
        let generating_setter = generating_setter.clone();
        Callback::from(move |_| {
            generating_setter.set(true);
            generate_error.set(None);

            let svg = app_store.replacement_svg.as_ref().unwrap();
            let template = app_store.gcode_template.as_ref().unwrap();
            let strokes =
                match prepared_strokes_for_svg(svg, &app_store.settings, &app_store.heat_seal) {
                    Ok(strokes) => strokes,
                    Err(error) => {
                        generate_error.set(Some(error));
                        generating_setter.set(false);
                        return;
                    }
                };
            let program = build_heat_seal_program(
                strokes,
                &app_store.settings.conversion,
                &app_store.heat_seal,
            );
            let replacement = format_heat_seal_program(&program).unwrap();
            match replace_marker_line(&template.content, &replacement) {
                Ok(output) => {
                    let bytes = with_original_utf8_bom(&output, template.had_utf8_bom);
                    prompt_download(replacement_output_path(&template.filename), bytes);
                }
                Err(err) => generate_error.set(Some(err.to_string())),
            }
            generating_setter.set(false);
        })
    };

    let merge_mode_onclick = {
        let generate_error = generate_error.clone();
        app_dispatch.reduce_mut_callback(move |app| {
            app.workflow_mode = WorkflowMode::MergeSvg;
            generate_error.set(None);
        })
    };
    let replacement_mode_onclick = {
        let generate_error = generate_error.clone();
        app_dispatch.reduce_mut_callback(move |app| {
            app.workflow_mode = WorkflowMode::ReplaceMarker;
            generate_error.set(None);
        })
    };

    html! {
        <div class="container">
            <div class={classes!("column")}>
                <h1>
                    { "svg2gcode" }
                </h1>
                <p>
                    { env!("CARGO_PKG_DESCRIPTION") }
                </p>
                <h3>{"Output mode"}</h3>
                <ButtonGroup>
                    <Button
                        title="Merge SVG files"
                        style={if app_store.workflow_mode == WorkflowMode::MergeSvg { ButtonStyle::Primary } else { ButtonStyle::Default }}
                        disabled={false}
                        onclick={merge_mode_onclick}
                    />
                    <Button
                        title="Replace G-code marker"
                        style={if app_store.workflow_mode == WorkflowMode::ReplaceMarker { ButtonStyle::Primary } else { ButtonStyle::Default }}
                        disabled={false}
                        onclick={replacement_mode_onclick}
                    />
                </ButtonGroup>
                <div class="divider"/>
                {
                    match app_store.workflow_mode {
                        WorkflowMode::MergeSvg => html! { <SvgForm/> },
                        WorkflowMode::ReplaceMarker => html! { <MarkerReplacementForm/> },
                    }
                }
                <ButtonGroup>
                    <Button
                        title={match app_store.workflow_mode {
                            WorkflowMode::MergeSvg => "Generate merged G-Code",
                            WorkflowMode::ReplaceMarker => "Generate replaced G-Code",
                        }}
                        style={ButtonStyle::Primary}
                        loading={*generating}
                        icon={
                            html_nested! (
                                <Icon name={IconName::Download} />
                            )
                        }
                        disabled={match app_store.workflow_mode {
                            WorkflowMode::MergeSvg => merge_generate_disabled,
                            WorkflowMode::ReplaceMarker => replacement_generate_disabled,
                        }}
                        onclick={match app_store.workflow_mode {
                            WorkflowMode::MergeSvg => merge_generate_onclick,
                            WorkflowMode::ReplaceMarker => replacement_generate_onclick,
                        }}
                    />
                    <HyperlinkButton
                        title="Settings"
                        style={ButtonStyle::Default}
                        icon={IconName::Edit}
                        href="#settings"
                    />
                </ButtonGroup>
                {
                    {
                        let mode_error = match app_store.workflow_mode {
                            WorkflowMode::MergeSvg => merge_validation_error.as_ref(),
                            WorkflowMode::ReplaceMarker => marker_validation_error.as_ref().or(replacement_trajectory_error.as_ref()),
                        };
                        if let Some(error) = mode_error.or(generate_error.as_ref()) {
                            html! { <p class="text-error">{error}</p> }
                        } else {
                            html! {}
                        }
                    }
                }
                <div class={classes!("card-container", "columns")}>
                    {
                        for app_store.svgs.iter().enumerate().filter(|_| app_store.workflow_mode == WorkflowMode::MergeSvg).map(|(i, svg)| {
                            let svg_base64 = base64::engine::general_purpose::STANDARD_NO_PAD.encode(svg.content.as_bytes());
                            let preview_svg = toolpath_preview(
                                svg,
                                &app_store.settings,
                                &app_store.heat_seal,
                                i == 0,
                            );
                            let preview_svg_base64 = base64::engine::general_purpose::STANDARD_NO_PAD.encode(preview_svg.as_bytes());
                            let open_preview = {
                                let bytes = preview_svg.into_bytes();
                                Callback::from(move |_: MouseEvent| open_svg_in_new_tab(&bytes))
                            };
                            let remove_svg_onclick = app_dispatch.reduce_mut_callback(move |app| {
                                app.svgs.remove(i);
                            });
                            let footer = html!{
                                <Button
                                    title="Remove"
                                    style={ButtonStyle::Primary}
                                    icon={
                                        html_nested!(
                                            <Icon name={IconName::Delete} />
                                        )
                                    }
                                    onclick={remove_svg_onclick}
                                />
                            };
                            html!{
                                <div class={classes!("column", "col-6", "col-xs-12")}>
                                    <Card
                                        title={svg.filename.clone()}
                                        img={html_nested!(
                                            <div class={classes!("columns", "preview-columns")}>
                                                <div class="column col-5">
                                                    <p class="text-center"><small>{"Original"}</small></p>
                                                    <img class="img-responsive" src={format!("data:image/svg+xml;base64,{}", svg_base64)} alt={svg.filename.clone()} />
                                                </div>
                                                <div class="divider-vert"></div>
                                                <div class="column col-5">
                                                    <p class="text-center"><small>{"Toolpath Preview"}</small></p>
                                                    <img class="img-responsive" style="cursor:pointer" src={format!("data:image/svg+xml;base64,{}", preview_svg_base64)} alt="toolpath preview" onclick={open_preview} />
                                                </div>
                                            </div>
                                        )}
                                        footer={footer}
                                    />
                                </div>
                            }
                        })
                    }
                    {
                        if app_store.workflow_mode == WorkflowMode::ReplaceMarker {
                            app_store.replacement_svg.as_ref().map(|svg| {
                                let svg_base64 = base64::engine::general_purpose::STANDARD_NO_PAD.encode(svg.content.as_bytes());
                                let preview_svg = toolpath_preview(
                                    svg,
                                    &app_store.settings,
                                    &app_store.heat_seal,
                                    true,
                                );
                                let preview_svg_base64 = base64::engine::general_purpose::STANDARD_NO_PAD.encode(preview_svg.as_bytes());
                                let open_preview = {
                                    let bytes = preview_svg.into_bytes();
                                    Callback::from(move |_: MouseEvent| open_svg_in_new_tab(&bytes))
                                };
                                let remove_svg_onclick = app_dispatch.reduce_mut_callback(|app| {
                                    app.replacement_svg = None;
                                });
                                html! {
                                    <div class={classes!("column", "col-6", "col-xs-12")}>
                                        <Card
                                            title={svg.filename.clone()}
                                            img={html_nested!(
                                                <div class={classes!("columns", "preview-columns")}>
                                                    <div class="column col-5">
                                                        <p class="text-center"><small>{"Original"}</small></p>
                                                        <img class="img-responsive" src={format!("data:image/svg+xml;base64,{}", svg_base64)} alt={svg.filename.clone()} />
                                                    </div>
                                                    <div class="divider-vert"></div>
                                                    <div class="column col-5">
                                                        <p class="text-center"><small>{"Toolpath Preview"}</small></p>
                                                        <img class="img-responsive" style="cursor:pointer" src={format!("data:image/svg+xml;base64,{}", preview_svg_base64)} alt="toolpath preview" onclick={open_preview} />
                                                    </div>
                                                </div>
                                            )}
                                            footer={html!(
                                                <Button
                                                    title="Remove"
                                                    style={ButtonStyle::Primary}
                                                    icon={html_nested!(<Icon name={IconName::Delete} />)}
                                                    onclick={remove_svg_onclick}
                                                />
                                            )}
                                        />
                                    </div>
                                }
                            }).unwrap_or_default()
                        } else {
                            html! {}
                        }
                    }
                </div>
                <SettingsForm/>
                <ImportExportModal/>
            </div>
            <div class={classes!("text-right", "column")}>
                <p>
                    { "See the project " }
                    <a href={env!("CARGO_PKG_REPOSITORY")}>
                        { "on GitHub" }
                    </a>
                    {" for support" }
                </p>
            </div>
        </div>
    }
}

#[function_component(AppContainer)]
fn app_container() -> Html {
    html! {
        <YewduxRoot>
            <App/>
        </YewduxRoot>
    }
}

fn main() {
    wasm_logger::init(wasm_logger::Config::new(Level::Info));
    yew::Renderer::<AppContainer>::new().render();
}
