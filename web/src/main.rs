#![cfg_attr(not(test), deny(unused_crate_dependencies))]

use std::path::{Path, PathBuf};

use base64::Engine;
use g_code::emit::FormatOptions;
use getrandom as _; // activate wasm_js backend for wasm32-unknown-unknown
use log::Level;
use roxmltree::{Document, ParsingOptions};
use svg2star::{
    lower::{ConversionOptions, svg_to_turtle},
    turtle::{CoordinateSystem, SvgPreviewTurtle},
};
use yew::prelude::*;

mod forms;
mod heat_seal;
mod marker_replace;
mod state;
mod ui;
mod util;

use forms::*;
use heat_seal::{collect_effective_strokes, format_heat_seal_program, svg_to_heat_seal_program};
use marker_replace::{replace_marker_line, with_original_utf8_bom};
use state::*;
use ui::*;
use util::*;
use yewdux::{YewduxRoot, prelude::use_store, use_dispatch};

fn format_options(settings: &svg2gcode::config::Settings) -> FormatOptions {
    FormatOptions {
        checksums: settings.postprocess.checksums,
        line_numbers: settings.postprocess.line_numbers,
        newline_before_comment: settings.postprocess.newline_before_comment,
        ..Default::default()
    }
}

fn replacement_output_path(filename: &str) -> PathBuf {
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("output");
    PathBuf::from(format!("{stem}_heatseal.gcode"))
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
            let hydrated_form_state = FormState::from_app(&app.settings, &app.heat_seal);
            form_dispatch.reduce_mut(|state| *state = hydrated_form_state);
        });
        upgraded_settings_and_hydrated_form.set(true);
    }

    let invalid_svg_names = app_store
        .svgs
        .iter()
        .filter_map(|svg| {
            let document = Document::parse_with_options(
                svg.content.as_str(),
                ParsingOptions {
                    allow_dtd: true,
                    ..Default::default()
                },
            )
            .ok()?;
            let strokes = collect_effective_strokes(
                &document,
                &app_store.settings.conversion,
                ConversionOptions {
                    dimensions: svg.dimensions,
                },
            );
            strokes.is_empty().then(|| svg.filename.clone())
        })
        .collect::<Vec<_>>();
    let no_valid_trajectory_error = (!invalid_svg_names.is_empty()).then(|| {
        format!(
            "No effective toolpath found in: {}. Remove or replace the file before downloading.",
            invalid_svg_names.join(", ")
        )
    });
    let marker_validation_error = app_store.gcode_template.as_ref().and_then(|template| {
        replace_marker_line(&template.content, "")
            .err()
            .map(|err| err.to_string())
    });
    let replacement_trajectory_error = app_store.replacement_svg.as_ref().and_then(|svg| {
        let document = Document::parse_with_options(
            svg.content.as_str(),
            ParsingOptions {
                allow_dtd: true,
                ..Default::default()
            },
        )
        .ok()?;
        collect_effective_strokes(
            &document,
            &app_store.settings.conversion,
            ConversionOptions {
                dimensions: svg.dimensions,
            },
        )
        .is_empty()
        .then(|| format!("No effective toolpath found in {}.", svg.filename))
    });

    let merge_generate_disabled =
        *generating || app_store.svgs.is_empty() || no_valid_trajectory_error.is_some();
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
            let mut merged_program = Vec::new();

            for svg in app_store.svgs.iter() {
                let options = ConversionOptions {
                    dimensions: svg.dimensions,
                };

                let document = Document::parse_with_options(
                    svg.content.as_str(),
                    ParsingOptions {
                        allow_dtd: true,
                        ..Default::default()
                    },
                )
                .unwrap();

                let mut program = svg_to_heat_seal_program(
                    &document,
                    &app_store.settings.conversion,
                    options,
                    &app_store.heat_seal,
                    app_store.settings.machine.supported_functionality.clone(),
                );

                if program.is_empty() {
                    generate_error.set(Some(format!(
                        "No effective toolpath found in {}.",
                        svg.filename
                    )));
                    generating_setter.set(false);
                    return;
                }
                merged_program.append(&mut program);
            }

            let output =
                format_heat_seal_program(&merged_program, format_options(&app_store.settings))
                    .unwrap();
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
            let document = Document::parse_with_options(
                svg.content.as_str(),
                ParsingOptions {
                    allow_dtd: true,
                    ..Default::default()
                },
            )
            .unwrap();
            let program = svg_to_heat_seal_program(
                &document,
                &app_store.settings.conversion,
                ConversionOptions {
                    dimensions: svg.dimensions,
                },
                &app_store.heat_seal,
                app_store.settings.machine.supported_functionality.clone(),
            );

            if program.is_empty() {
                generate_error.set(Some(format!(
                    "No effective toolpath found in {}.",
                    svg.filename
                )));
                generating_setter.set(false);
                return;
            }

            let replacement =
                format_heat_seal_program(&program, format_options(&app_store.settings)).unwrap();
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
                            WorkflowMode::MergeSvg => no_valid_trajectory_error.as_ref(),
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
                            let preview_svg = Document::parse_with_options(
                                svg.content.as_str(),
                                ParsingOptions { allow_dtd: true, ..Default::default() },
                            )
                            .ok()
                            .map(|doc| {
                                let options = ConversionOptions { dimensions: svg.dimensions };
                                svg_to_turtle(&doc, &app_store.settings.conversion.inner, options, SvgPreviewTurtle::default(), CoordinateSystem::YUp).into_preview()
                            })
                            .unwrap_or_default();
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
                                let preview_svg = Document::parse_with_options(
                                    svg.content.as_str(),
                                    ParsingOptions { allow_dtd: true, ..Default::default() },
                                )
                                .ok()
                                .map(|doc| {
                                    let options = ConversionOptions { dimensions: svg.dimensions };
                                    svg_to_turtle(&doc, &app_store.settings.conversion.inner, options, SvgPreviewTurtle::default(), CoordinateSystem::YUp).into_preview()
                                })
                                .unwrap_or_default();
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
