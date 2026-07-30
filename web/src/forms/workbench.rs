use yew::prelude::*;
use yewdux::functional::use_store;

use crate::state::{FormState, MachineModel};

const MINOR_GRID_MM: f64 = 10.;
const MAJOR_GRID_MM: f64 = 50.;

#[derive(Debug, Clone, Copy, PartialEq)]
struct PreviewFrame {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

fn preview_frame(
    machine_model: MachineModel,
    width: &Result<f64, String>,
    height: &Result<f64, String>,
) -> Option<PreviewFrame> {
    let width = *width.as_ref().ok()?;
    let height = *height.as_ref().ok()?;
    if width <= 0.
        || height <= 0.
        || width > machine_model.width_mm()
        || height > machine_model.height_mm()
    {
        return None;
    }
    Some(PreviewFrame {
        x: (machine_model.width_mm() - width) / 2.,
        y: (machine_model.height_mm() - height) / 2.,
        width,
        height,
    })
}

#[function_component(MachineWorkbench)]
pub fn machine_workbench() -> Html {
    let (form_state, form_dispatch) = use_store::<FormState>();
    let machine = form_state.machine_model;
    let bed_width = machine.width_mm();
    let bed_height = machine.height_mm();
    let frame = preview_frame(
        machine,
        &form_state.outer_frame_width,
        &form_state.outer_frame_height,
    );

    let select_a1 = form_dispatch.reduce_mut_callback(|form| {
        form.select_machine_model(MachineModel::A1);
    });
    let select_a2l = form_dispatch.reduce_mut_callback(|form| {
        form.select_machine_model(MachineModel::A2L);
    });

    let machine_button = |model: MachineModel, onclick: Callback<MouseEvent>| {
        html! {
            <button
                type="button"
                class={classes!("machine-model-button", (machine == model).then_some("active"))}
                aria-pressed={(machine == model).to_string()}
                {onclick}
            >
                <strong>{model.label()}</strong>
                <span>{format!("{} × {} mm", model.width_mm(), model.height_mm())}</span>
            </button>
        }
    };

    html! {
        <div class="machine-workbench-section">
            <div class="machine-model-heading">
                <div>
                    <h4>{"Machine model"}</h4>
                    <p>{"Changing the machine model resets the outer-frame size to 150 × 150 mm."}</p>
                </div>
                <span class="machine-model-badge">{machine.label()}</span>
            </div>
            <div class="machine-model-selector" role="group" aria-label="Machine model">
                {machine_button(MachineModel::A1, select_a1)}
                {machine_button(MachineModel::A2L, select_a2l)}
            </div>

            <div class="machine-workbench">
                <div class="machine-workbench-toolbar">
                    <span>{format!("{} workbench", machine.label())}</span>
                    <span>{format!("{} × {} mm", bed_width, bed_height)}</span>
                </div>
                <svg
                    class="machine-workbench-canvas"
                    viewBox={format!("0 0 {bed_width} {bed_height}")}
                    role="img"
                    aria-label={format!("{} workbench with centered outer sealing frame", machine.label())}
                >
                    <defs>
                        <pattern id="minor-workbench-grid" width={MINOR_GRID_MM.to_string()} height={MINOR_GRID_MM.to_string()} patternUnits="userSpaceOnUse">
                            <path d={format!("M {MINOR_GRID_MM} 0 L 0 0 0 {MINOR_GRID_MM}")} class="machine-grid-minor" />
                        </pattern>
                        <pattern id="major-workbench-grid" width={MAJOR_GRID_MM.to_string()} height={MAJOR_GRID_MM.to_string()} patternUnits="userSpaceOnUse">
                            <rect width={MAJOR_GRID_MM.to_string()} height={MAJOR_GRID_MM.to_string()} fill="url(#minor-workbench-grid)" />
                            <path d={format!("M {MAJOR_GRID_MM} 0 L 0 0 0 {MAJOR_GRID_MM}")} class="machine-grid-major" />
                        </pattern>
                    </defs>
                    <rect x="0" y="0" width={bed_width.to_string()} height={bed_height.to_string()} class="machine-bed-background" />
                    <rect x="0" y="0" width={bed_width.to_string()} height={bed_height.to_string()} fill="url(#major-workbench-grid)" />
                    <line x1={(bed_width / 2.).to_string()} y1="0" x2={(bed_width / 2.).to_string()} y2={bed_height.to_string()} class="machine-center-axis machine-center-axis-x" />
                    <line x1="0" y1={(bed_height / 2.).to_string()} x2={bed_width.to_string()} y2={(bed_height / 2.).to_string()} class="machine-center-axis machine-center-axis-y" />
                    {
                        frame.map(|frame| html! {
                            <rect
                                x={frame.x.to_string()}
                                y={frame.y.to_string()}
                                width={frame.width.to_string()}
                                height={frame.height.to_string()}
                                class={classes!("machine-frame-preview", (!form_state.outer_frame_enabled).then_some("disabled"))}
                            />
                        }).unwrap_or_else(|| html! {
                            <text x={(bed_width / 2.).to_string()} y={(bed_height / 2.).to_string()} class="machine-frame-invalid" text-anchor="middle">
                                {"Enter a valid outer-frame size"}
                            </text>
                        })
                    }
                    <rect x="0" y="0" width={bed_width.to_string()} height={bed_height.to_string()} class="machine-bed-border" />
                </svg>
                <div class="machine-workbench-footer">
                    <span><i class="workbench-swatch frame"></i>{if form_state.outer_frame_enabled { "Outer frame enabled" } else { "Outer frame preview (disabled)" }}</span>
                    <span>{"Fixed G-code center: X127.970, Y127.970"}</span>
                </div>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_frame_is_centered_for_both_machine_models() {
        let a1 = preview_frame(MachineModel::A1, &Ok(150.), &Ok(150.)).unwrap();
        assert_eq!(a1.x, 53.);
        assert_eq!(a1.y, 53.);

        let a2l = preview_frame(MachineModel::A2L, &Ok(150.), &Ok(150.)).unwrap();
        assert_eq!(a2l.x, 90.);
        assert_eq!(a2l.y, 85.);
    }

    #[test]
    fn preview_frame_rejects_dimensions_outside_selected_workbench() {
        assert!(preview_frame(MachineModel::A1, &Ok(256.), &Ok(256.)).is_some());
        assert!(preview_frame(MachineModel::A1, &Ok(256.1), &Ok(200.)).is_none());
        assert!(preview_frame(MachineModel::A2L, &Ok(330.), &Ok(320.)).is_some());
        assert!(preview_frame(MachineModel::A2L, &Ok(330.), &Ok(320.1)).is_none());
    }
}
