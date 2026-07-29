use std::borrow::Cow;

use g_code::{
    command,
    emit::{Field, FormatOptions, Token, Value, format_gcode_fmt},
};
use roxmltree::Document;
use svg2gcode::{GCodeTurtle, Machine, config::GCodeConfig};
use svg2star::{
    lower::{ConversionOptions, svg_to_turtle},
    turtle::{
        CoordinateSystem, StrokeCollectingTurtle,
        elements::{DrawCommand, Stroke},
    },
};

use crate::state::HeatSealSettings;

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

pub fn strokes_to_program(
    strokes: Vec<Stroke>,
    config: &GCodeConfig,
    heat_seal: &HeatSealSettings,
    supported_functionality: svg2gcode::config::SupportedFunctionality,
) -> Vec<Token<'static>> {
    let machine = Machine::new(supported_functionality, None, None, None, None);
    let mut turtle = GCodeTurtle {
        machine,
        tolerance: config.tolerance,
        feedrate: config.feedrate,
        program: vec![],
    };

    for stroke in strokes.into_iter().filter(stroke_has_geometry) {
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
        turtle
            .program
            .push(value_field("S", heat_seal.dwell_seconds));
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
        turtle.program.push(value_field("S", heat_seal.temperature));
        turtle.program.push(comment("不要动，温度调控S"));

        turtle.program.extend(
            command!(LinearInterpolation {
                Z: heat_seal.working_height,
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

    turtle.program
}

pub fn svg_to_heat_seal_program(
    document: &Document,
    config: &GCodeConfig,
    options: ConversionOptions,
    heat_seal: &HeatSealSettings,
    supported_functionality: svg2gcode::config::SupportedFunctionality,
) -> Vec<Token<'static>> {
    let strokes = collect_effective_strokes(document, config, options);
    strokes_to_program(strokes, config, heat_seal, supported_functionality)
}

pub fn format_heat_seal_program(
    program: &[Token<'_>],
    options: FormatOptions,
) -> Result<String, std::fmt::Error> {
    let mut formatted = String::new();
    format_gcode_fmt(program, options, &mut formatted)?;

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
    use g_code::emit::FormatOptions;
    use svg2star::turtle::elements::{CubicBezierSegment, DrawCommand, Point, Stroke};

    use super::*;

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
        let stroke = Stroke::new(points[0], commands);
        let config = GCodeConfig {
            tolerance: 0.001,
            feedrate: 600.,
            ..Default::default()
        };
        let program = strokes_to_program(
            vec![stroke],
            &config,
            &HeatSealSettings::default(),
            Default::default(),
        );
        let output = format_heat_seal_program(&program, FormatOptions::default()).unwrap();

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
    fn skips_zero_length_strokes() {
        let point = Point::new(1., 1.);
        let stroke = Stroke::new(
            point,
            vec![DrawCommand::LineTo {
                from: point,
                to: point,
            }],
        );
        assert!(
            strokes_to_program(
                vec![stroke],
                &GCodeConfig::default(),
                &HeatSealSettings::default(),
                Default::default(),
            )
            .is_empty()
        );
    }

    #[test]
    fn repeats_complete_cycle_for_each_stroke() {
        let stroke = |start: Point<f64>, end: Point<f64>| {
            Stroke::new(
                start,
                vec![DrawCommand::LineTo {
                    from: start,
                    to: end,
                }],
            )
        };
        let program = strokes_to_program(
            vec![
                stroke(Point::new(1., 2.), Point::new(3., 4.)),
                stroke(Point::new(10., 20.), Point::new(30., 40.)),
            ],
            &GCodeConfig::default(),
            &HeatSealSettings::default(),
            Default::default(),
        );
        let output = format_heat_seal_program(&program, FormatOptions::default()).unwrap();

        assert_eq!(output.matches("G1 X125 Y250 Z150").count(), 2);
        assert!(output.contains("G0 X1 Y2 F1000 E-0.6"));
        assert!(output.contains("G0 X10 Y20 F1000 E-0.6"));
        assert_eq!(output.matches("G1 Z100").count(), 2);
    }

    #[test]
    fn circular_interpolation_setting_controls_g2_g3_output() {
        let curve = Stroke::new(
            Point::new(0., 0.),
            vec![DrawCommand::CubicBezier(CubicBezierSegment {
                from: Point::new(0., 0.),
                ctrl1: Point::new(0., 10.),
                ctrl2: Point::new(10., 10.),
                to: Point::new(10., 0.),
            })],
        );
        let config = GCodeConfig {
            tolerance: 0.01,
            ..Default::default()
        };
        let linear = strokes_to_program(
            vec![curve.clone()],
            &config,
            &HeatSealSettings::default(),
            Default::default(),
        );
        let circular = strokes_to_program(
            vec![curve],
            &config,
            &HeatSealSettings::default(),
            svg2gcode::config::SupportedFunctionality {
                circular_interpolation: true,
            },
        );
        let linear = format_heat_seal_program(&linear, FormatOptions::default()).unwrap();
        let circular = format_heat_seal_program(&circular, FormatOptions::default()).unwrap();

        assert!(
            !linear
                .lines()
                .any(|line| line.starts_with("G2") || line.starts_with("G3"))
        );
        assert!(
            circular
                .lines()
                .any(|line| line.starts_with("G2") || line.starts_with("G3"))
        );
    }

    #[test]
    fn advanced_formatting_options_remain_active() {
        let stroke = Stroke::new(
            Point::new(0., 0.),
            vec![DrawCommand::LineTo {
                from: Point::new(0., 0.),
                to: Point::new(1., 1.),
            }],
        );
        let program = strokes_to_program(
            vec![stroke],
            &GCodeConfig::default(),
            &HeatSealSettings::default(),
            Default::default(),
        );
        let output = format_heat_seal_program(
            &program,
            FormatOptions {
                checksums: true,
                line_numbers: true,
                newline_before_comment: true,
                ..Default::default()
            },
        )
        .unwrap();

        assert!(output.starts_with("N0 G1"));
        assert!(output.contains('*'));
        assert!(output.contains("\nN1 *"));
        assert!(output.contains(';'));
    }
}
