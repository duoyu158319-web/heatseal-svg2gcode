use std::borrow::Cow;

use g_code::{
    command,
    emit::{Field, FormatOptions, Token, Value, format_gcode_fmt},
};
use lyon_geom::Box2D;
use roxmltree::Document;
use svg2gcode::{GCodeTurtle, Machine, config::GCodeConfig};
use svg2star::{
    lower::{ConversionOptions, svg_to_turtle},
    turtle::{
        CoordinateSystem, StrokeCollectingTurtle, SvgPreviewTurtle, Turtle,
        elements::{DrawCommand, Point, Stroke, Vector},
    },
};

use crate::state::{HeatSealSettings, OuterFrameSettings};

pub const MACHINE_CENTER_X: f64 = 127.970;
pub const MACHINE_CENTER_Y: f64 = 127.970;

fn command_field(letter: &'static str, value: usize) -> Token<'static> {
    Token::Field(Field {
        letters: Cow::Borrowed(letter),
        value: Value::Integer(value),
    })
}

fn value_field(letter: &'static str, value: f64) -> Token<'static> {
    Token::Field(Field {
        letters: Cow::Borrowed(letter),
        value: Value::Float(value),
    })
}

fn comment(inner: &'static str) -> Token<'static> {
    Token::Comment {
        is_inline: false,
        inner: Cow::Borrowed(inner),
    }
}

fn stroke_has_geometry(stroke: &Stroke) -> bool {
    stroke.commands().any(|command| match command {
        DrawCommand::Comment(_) => false,
        command => command.bounding_box().is_some_and(|bounds| {
            (bounds.max.x - bounds.min.x).abs() > f64::EPSILON
                || (bounds.max.y - bounds.min.y).abs() > f64::EPSILON
        }),
    })
}

pub fn collect_effective_strokes(
    document: &Document,
    config: &GCodeConfig,
    options: ConversionOptions,
) -> Vec<Stroke> {
    svg_to_turtle(
        document,
        &config.inner,
        options,
        StrokeCollectingTurtle::default(),
        CoordinateSystem::YUp,
    )
    .into_strokes()
    .into_iter()
    .filter(stroke_has_geometry)
    .collect()
}

pub fn prepare_svg_strokes(
    document: &Document,
    config: &GCodeConfig,
    options: ConversionOptions,
    auto_center: bool,
) -> Vec<Stroke> {
    let strokes = collect_effective_strokes(document, config, options);
    if auto_center {
        center_strokes(strokes, Point::new(MACHINE_CENTER_X, MACHINE_CENTER_Y))
    } else {
        strokes
    }
}

pub fn strokes_bounding_box(strokes: &[Stroke]) -> Option<Box2D<f64>> {
    strokes
        .iter()
        .map(Stroke::bounding_box)
        .reduce(|a, b| Box2D::from_points([a.min, a.max, b.min, b.max]))
}

pub fn center_strokes(strokes: Vec<Stroke>, target: Point<f64>) -> Vec<Stroke> {
    let Some(bounds) = strokes_bounding_box(&strokes) else {
        return strokes;
    };
    let center = Point::new(
        (bounds.min.x + bounds.max.x) / 2.,
        (bounds.min.y + bounds.max.y) / 2.,
    );
    translate_strokes(strokes, target - center)
}

fn translate_strokes(strokes: Vec<Stroke>, offset: Vector<f64>) -> Vec<Stroke> {
    strokes
        .into_iter()
        .map(|stroke| {
            let start = stroke.start_point() + offset;
            let commands = stroke
                .into_commands()
                .map(|command| match command {
                    DrawCommand::LineTo { from, to } => DrawCommand::LineTo {
                        from: from + offset,
                        to: to + offset,
                    },
                    DrawCommand::Arc(mut arc) => {
                        arc.from += offset;
                        arc.to += offset;
                        DrawCommand::Arc(arc)
                    }
                    DrawCommand::CubicBezier(mut curve) => {
                        curve.from += offset;
                        curve.ctrl1 += offset;
                        curve.ctrl2 += offset;
                        curve.to += offset;
                        DrawCommand::CubicBezier(curve)
                    }
                    DrawCommand::QuadraticBezier(mut curve) => {
                        curve.from += offset;
                        curve.ctrl += offset;
                        curve.to += offset;
                        DrawCommand::QuadraticBezier(curve)
                    }
                    DrawCommand::Comment(comment) => DrawCommand::Comment(comment),
                })
                .collect();
            Stroke::new(start, commands)
        })
        .collect()
}

pub fn outer_frame_stroke(settings: &OuterFrameSettings) -> Stroke {
    let min = Point::new(
        MACHINE_CENTER_X - settings.width_mm / 2.,
        MACHINE_CENTER_Y - settings.height_mm / 2.,
    );
    let lower_right = Point::new(
        MACHINE_CENTER_X + settings.width_mm / 2.,
        MACHINE_CENTER_Y - settings.height_mm / 2.,
    );
    let upper_right = Point::new(
        MACHINE_CENTER_X + settings.width_mm / 2.,
        MACHINE_CENTER_Y + settings.height_mm / 2.,
    );
    let upper_left = Point::new(
        MACHINE_CENTER_X - settings.width_mm / 2.,
        MACHINE_CENTER_Y + settings.height_mm / 2.,
    );
    let points = [min, lower_right, upper_right, upper_left, min];
    Stroke::new(
        min,
        points
            .windows(2)
            .map(|pair| DrawCommand::LineTo {
                from: pair[0],
                to: pair[1],
            })
            .collect(),
    )
}

pub fn frame_fit_error(strokes: &[Stroke], heat_seal: &HeatSealSettings) -> Option<String> {
    if !heat_seal.auto_center_svg || !heat_seal.outer_frame.enabled {
        return None;
    }
    let bounds = strokes_bounding_box(strokes)?;
    let width = bounds.max.x - bounds.min.x;
    let height = bounds.max.y - bounds.min.y;
    if width > heat_seal.outer_frame.width_mm + f64::EPSILON
        || height > heat_seal.outer_frame.height_mm + f64::EPSILON
    {
        Some(format!(
            "The SVG toolpath size ({:.3} × {:.3} mm) exceeds the outer frame ({:.3} × {:.3} mm). Increase the frame size or turn off automatic centering.",
            width, height, heat_seal.outer_frame.width_mm, heat_seal.outer_frame.height_mm,
        ))
    } else {
        None
    }
}

#[derive(Clone, Copy)]
struct HeatSealProfile {
    temperature: f64,
    working_height: f64,
}

fn append_heat_seal_cycle(
    turtle: &mut GCodeTurtle,
    stroke: Stroke,
    dwell_seconds: f64,
    profile: HeatSealProfile,
) {
    if !stroke_has_geometry(&stroke) {
        return;
    }
    let start = stroke.start_point();
    turtle.program.extend(
        command!(LinearInterpolation {
            X: 125.,
            Y: 250.,
            Z: 150.,
            F: 1000.,
            E: -0.6,
        })
        .into_token_vec(),
    );
    turtle.program.push(comment("不要动"));

    turtle.program.push(command_field("G", 4));
    turtle.program.push(value_field("S", dwell_seconds));
    turtle.program.push(comment("不要动，悬停时间S"));

    turtle.program.extend(
        command!(RapidPositioning {
            X: turtle.round_coordinate(start.x),
            Y: turtle.round_coordinate(start.y),
            F: 1000.,
            E: -0.6,
        })
        .into_token_vec(),
    );
    turtle.program.push(comment("插入Gcode起始点XY 坐标"));

    turtle.program.push(command_field("M", 104));
    turtle.program.push(value_field("S", profile.temperature));
    turtle.program.push(comment("不要动，温度调控S"));

    turtle.program.extend(
        command!(LinearInterpolation {
            Z: profile.working_height,
            F: 1000.,
            E: -0.6,
        })
        .into_token_vec(),
    );
    turtle.program.push(comment("不要动,高度调节Z"));

    turtle.draw_stroke_commands(stroke);

    turtle
        .program
        .extend(command!(LinearInterpolation { Z: 100. }).into_token_vec());
    turtle.program.push(comment("不要动"));
}

pub fn build_heat_seal_program(
    svg_strokes: Vec<Stroke>,
    config: &GCodeConfig,
    heat_seal: &HeatSealSettings,
) -> Vec<Token<'static>> {
    let machine = Machine::new(Default::default(), None, None, None, None);
    let mut turtle = GCodeTurtle {
        machine,
        tolerance: config.tolerance,
        feedrate: config.feedrate,
        program: vec![],
    };

    if heat_seal.outer_frame.enabled {
        append_heat_seal_cycle(
            &mut turtle,
            outer_frame_stroke(&heat_seal.outer_frame),
            heat_seal.dwell_seconds,
            HeatSealProfile {
                temperature: heat_seal.outer_frame.temperature,
                working_height: heat_seal.outer_frame.working_height,
            },
        );
    }

    let svg_profile = HeatSealProfile {
        temperature: heat_seal.temperature,
        working_height: heat_seal.working_height,
    };
    for stroke in svg_strokes {
        append_heat_seal_cycle(&mut turtle, stroke, heat_seal.dwell_seconds, svg_profile);
    }
    turtle.program
}

pub fn strokes_to_preview(
    mut svg_strokes: Vec<Stroke>,
    heat_seal: &HeatSealSettings,
    include_outer_frame: bool,
) -> String {
    if include_outer_frame && heat_seal.outer_frame.enabled {
        svg_strokes.insert(0, outer_frame_stroke(&heat_seal.outer_frame));
    }
    let mut turtle = SvgPreviewTurtle::default();
    turtle.begin();
    for stroke in svg_strokes {
        turtle.stroke(stroke);
    }
    turtle.end();
    turtle.into_preview()
}

pub fn format_heat_seal_program(program: &[Token<'_>]) -> Result<String, std::fmt::Error> {
    let mut formatted = String::new();
    format_gcode_fmt(program, FormatOptions::default(), &mut formatted)?;

    let mut output = String::with_capacity(formatted.len());
    for line in formatted.split_inclusive('\n') {
        if let Some(comment_index) = line.find(';') {
            let needs_separator = comment_index > 0
                && !line[..comment_index]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_whitespace);
            if needs_separator {
                output.push_str(&line[..comment_index]);
                output.push(' ');
                output.push_str(&line[comment_index..]);
                continue;
            }
        }
        output.push_str(line);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use svg2star::turtle::elements::{CubicBezierSegment, DrawCommand, Point, Stroke};

    use super::*;

    fn line_stroke(start: Point<f64>, end: Point<f64>) -> Stroke {
        Stroke::new(
            start,
            vec![DrawCommand::LineTo {
                from: start,
                to: end,
            }],
        )
    }

    #[test]
    fn emits_exact_single_stroke_heat_seal_cycle() {
        let points = [
            Point::new(90.071, 90.071),
            Point::new(165.869, 90.071),
            Point::new(165.869, 165.869),
            Point::new(90.071, 165.869),
            Point::new(90.071, 90.071),
        ];
        let commands = points
            .windows(2)
            .map(|pair| DrawCommand::LineTo {
                from: pair[0],
                to: pair[1],
            })
            .collect();
        let config = GCodeConfig {
            tolerance: 0.001,
            feedrate: 600.,
            ..Default::default()
        };
        let program = build_heat_seal_program(
            vec![Stroke::new(points[0], commands)],
            &config,
            &HeatSealSettings::default(),
        );
        let output = format_heat_seal_program(&program).unwrap();

        assert_eq!(
            output,
            concat!(
                "G1 X125 Y250 Z150 F1000 E-0.6 ;不要动\n",
                "G4 S120 ;不要动，悬停时间S\n",
                "G0 X90.071 Y90.071 F1000 E-0.6 ;插入Gcode起始点XY 坐标\n",
                "M104 S230 ;不要动，温度调控S\n",
                "G1 Z0.12 F1000 E-0.6 ;不要动,高度调节Z\n",
                "G1 X165.869 Y90.071 F600\n",
                "G1 X165.869 Y165.869 F600\n",
                "G1 X90.071 Y165.869 F600\n",
                "G1 X90.071 Y90.071 F600\n",
                "G1 Z100 ;不要动\n",
            )
        );
    }

    #[test]
    fn centers_all_strokes_as_one_group_without_scaling() {
        let strokes = vec![
            line_stroke(Point::new(10., 20.), Point::new(30., 40.)),
            line_stroke(Point::new(50., 10.), Point::new(70., 30.)),
        ];
        let centered = center_strokes(strokes, Point::new(MACHINE_CENTER_X, MACHINE_CENTER_Y));
        let bounds = strokes_bounding_box(&centered).unwrap();
        assert!(((bounds.min.x + bounds.max.x) / 2. - MACHINE_CENTER_X).abs() < 1e-9);
        assert!(((bounds.min.y + bounds.max.y) / 2. - MACHINE_CENTER_Y).abs() < 1e-9);
        assert!((bounds.max.x - bounds.min.x - 60.).abs() < 1e-9);
        assert!((bounds.max.y - bounds.min.y - 30.).abs() < 1e-9);
        assert_eq!(centered[0].start_point(), Point::new(97.970, 122.970));
    }

    #[test]
    fn builds_exact_centered_rectangles_for_non_square_dimensions() {
        let frame = OuterFrameSettings {
            enabled: true,
            width_mm: 150.,
            height_mm: 140.,
            ..Default::default()
        };
        let stroke = outer_frame_stroke(&frame);
        assert_eq!(stroke.start_point(), Point::new(52.970, 57.970));
        assert_eq!(stroke.end_point(), stroke.start_point());
        let ends = stroke
            .commands()
            .filter_map(DrawCommand::end_point)
            .collect::<Vec<_>>();
        assert_eq!(
            ends,
            vec![
                Point::new(202.970, 57.970),
                Point::new(202.970, 197.970),
                Point::new(52.970, 197.970),
                Point::new(52.970, 57.970),
            ]
        );
    }

    #[test]
    fn outer_frame_is_emitted_once_before_svg_with_its_own_profile() {
        let mut heat = HeatSealSettings::default();
        heat.dwell_seconds = 33.;
        heat.temperature = 220.;
        heat.working_height = 0.2;
        heat.outer_frame = OuterFrameSettings {
            enabled: true,
            width_mm: 150.,
            height_mm: 140.,
            temperature: 240.,
            working_height: 0.3,
        };
        let program = build_heat_seal_program(
            vec![
                line_stroke(Point::new(1., 2.), Point::new(3., 4.)),
                line_stroke(Point::new(10., 20.), Point::new(30., 40.)),
            ],
            &GCodeConfig::default(),
            &heat,
        );
        let output = format_heat_seal_program(&program).unwrap();
        assert_eq!(output.matches("G1 X125 Y250 Z150").count(), 3);
        assert_eq!(output.matches("G4 S33").count(), 3);
        assert_eq!(output.matches("M104 S240").count(), 1);
        assert_eq!(output.matches("G1 Z0.3 F1000").count(), 1);
        assert_eq!(output.matches("M104 S220").count(), 2);
        assert!(output.find("G0 X52.97 Y57.97").unwrap() < output.find("G0 X1 Y2").unwrap());
    }

    #[test]
    fn machine_model_does_not_change_fixed_gcode_center() {
        let mut a1 = HeatSealSettings::default();
        a1.outer_frame.enabled = true;
        let mut a2l = a1.clone();
        a2l.machine_model = crate::state::MachineModel::A2L;

        let a1_output = format_heat_seal_program(&build_heat_seal_program(
            vec![],
            &GCodeConfig::default(),
            &a1,
        ))
        .unwrap();
        let a2l_output = format_heat_seal_program(&build_heat_seal_program(
            vec![],
            &GCodeConfig::default(),
            &a2l,
        ))
        .unwrap();

        assert_eq!(a1_output, a2l_output);
        assert!(a1_output.contains("G0 X52.97 Y52.97"));
        assert!(a1_output.contains("G1 X202.97 Y52.97"));
    }

    #[test]
    fn skips_zero_length_strokes() {
        let point = Point::new(1., 1.);
        let stroke = line_stroke(point, point);
        assert!(
            build_heat_seal_program(
                vec![stroke],
                &GCodeConfig::default(),
                &HeatSealSettings::default(),
            )
            .is_empty()
        );
    }

    #[test]
    fn web_heat_seal_output_always_uses_linearized_curves() {
        let curve = Stroke::new(
            Point::new(0., 0.),
            vec![DrawCommand::CubicBezier(CubicBezierSegment {
                from: Point::new(0., 0.),
                ctrl1: Point::new(0., 10.),
                ctrl2: Point::new(10., 10.),
                to: Point::new(10., 0.),
            })],
        );
        let program = build_heat_seal_program(
            vec![curve],
            &GCodeConfig {
                tolerance: 0.01,
                ..Default::default()
            },
            &HeatSealSettings::default(),
        );
        let output = format_heat_seal_program(&program).unwrap();
        assert!(output.lines().any(|line| line.starts_with("G1 X")));
        assert!(
            !output
                .lines()
                .any(|line| line.starts_with("G2") || line.starts_with("G3"))
        );
    }

    #[test]
    fn reports_centered_svg_that_is_larger_than_enabled_frame() {
        let strokes = vec![line_stroke(Point::new(0., 0.), Point::new(200., 10.))];
        let mut heat = HeatSealSettings::default();
        heat.auto_center_svg = true;
        heat.outer_frame.enabled = true;
        heat.outer_frame.width_mm = 150.;
        assert!(frame_fit_error(&strokes, &heat).is_some());
        heat.auto_center_svg = false;
        assert!(frame_fit_error(&strokes, &heat).is_none());
    }
}
