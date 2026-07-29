use thiserror::Error;

const MARKER: &str = "标记";

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MarkerReplaceError {
    #[error("No line containing ‘标记’ was found in the G-code file")]
    MarkerNotFound,
    #[error("Multiple marker lines were found at lines: {0}")]
    MultipleMarkers(String),
}

fn line_ending(text: &str) -> &'static str {
    if text.contains("\r\n") {
        "\r\n"
    } else if text.contains('\n') {
        "\n"
    } else if text.contains('\r') {
        "\r"
    } else {
        "\n"
    }
}

fn normalize_line_endings(text: &str, newline: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .split('\n')
        .collect::<Vec<_>>()
        .join(newline)
}

pub fn replace_marker_line(
    template: &str,
    replacement: &str,
) -> Result<String, MarkerReplaceError> {
    let newline = line_ending(template);
    let normalized_template = normalize_line_endings(template, "\n");
    let marker_lines = normalized_template
        .split('\n')
        .enumerate()
        .filter_map(|(index, line)| line.contains(MARKER).then_some(index + 1))
        .collect::<Vec<_>>();

    match marker_lines.as_slice() {
        [] => return Err(MarkerReplaceError::MarkerNotFound),
        [_] => {}
        lines => {
            return Err(MarkerReplaceError::MultipleMarkers(
                lines
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
            ));
        }
    }

    let marker_index = marker_lines[0] - 1;
    let template_had_trailing_newline = normalized_template.ends_with('\n');
    let replacement = normalize_line_endings(replacement, "\n")
        .trim_end_matches('\n')
        .to_owned();
    let mut lines = normalized_template
        .split('\n')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    lines.splice(
        marker_index..=marker_index,
        replacement.lines().map(str::to_owned),
    );

    let mut output = lines.join(newline);
    if template_had_trailing_newline && !output.ends_with(newline) {
        output.push_str(newline);
    }
    Ok(output)
}

pub fn with_original_utf8_bom(text: &str, had_utf8_bom: bool) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(text.len() + usize::from(had_utf8_bom) * 3);
    if had_utf8_bom {
        bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    bytes.extend_from_slice(text.as_bytes());
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_whole_marker_line_and_preserves_crlf() {
        let template = "G28\r\n;这里是标记，需要替换\r\nM106 S255\r\n";
        let replacement = "G1 X1 Y2\nG1 Z100\n";
        assert_eq!(
            replace_marker_line(template, replacement).unwrap(),
            "G28\r\nG1 X1 Y2\r\nG1 Z100\r\nM106 S255\r\n"
        );
    }

    #[test]
    fn rejects_missing_or_multiple_markers() {
        assert_eq!(
            replace_marker_line("G28\nM2\n", "G1 X1\n"),
            Err(MarkerReplaceError::MarkerNotFound)
        );
        assert_eq!(
            replace_marker_line(";标记\nG1 X1 ;标记\n", "G1 X2\n"),
            Err(MarkerReplaceError::MultipleMarkers("1, 2".to_owned()))
        );
    }

    #[test]
    fn preserves_utf8_bom_when_requested() {
        assert_eq!(with_original_utf8_bom("G28\n", false), b"G28\n");
        assert_eq!(
            with_original_utf8_bom("G28\n", true),
            [0xEF, 0xBB, 0xBF, b'G', b'2', b'8', b'\n']
        );
    }
}
