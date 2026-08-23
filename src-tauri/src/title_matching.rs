use std::path::Path;

const VIDEO_EXTENSIONS: [&str; 12] = [
    "3g2", "3gp", "avi", "flv", "m4v", "mkv", "mov", "mp4", "mpeg", "mpg", "webm", "wmv",
];

pub(crate) const TITLE_KEYS_VERSION: i64 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TitleMatchKeys {
    pub(crate) normalized: String,
    pub(crate) canonical: String,
    pub(crate) has_copy_marker: bool,
    pub(crate) numeric: String,
}

fn title_for_matching(title: &str) -> &str {
    let trimmed = title.trim();
    let path = Path::new(trimmed);
    let is_video_filename = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            VIDEO_EXTENSIONS
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        });
    if is_video_filename {
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or(trimmed)
    } else {
        trimmed
    }
}

pub(crate) fn normalized(title: &str) -> String {
    title_for_matching(title)
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn canonical(title: &str) -> (String, bool) {
    let trimmed = title_for_matching(title);
    let Some(without_closing_parenthesis) = trimmed.strip_suffix(')') else {
        return (normalized(trimmed), false);
    };
    let Some(marker_start) = without_closing_parenthesis.rfind(" (") else {
        return (normalized(trimmed), false);
    };
    let base = &without_closing_parenthesis[..marker_start];
    let marker_number = &without_closing_parenthesis[marker_start + 2..];
    let is_duplicate_marker = !base.is_empty()
        && !marker_number.is_empty()
        && marker_number
            .chars()
            .all(|character| character.is_ascii_digit())
        && marker_number.parse::<u64>().is_ok_and(|number| number >= 2);
    if is_duplicate_marker {
        (normalized(base), true)
    } else {
        (normalized(trimmed), false)
    }
}

pub(crate) fn number_sequences(title: &str) -> Vec<String> {
    let mut sequences = Vec::new();
    let mut current = String::new();
    for character in title_for_matching(title).chars() {
        if character.is_ascii_digit() {
            current.push(character);
        } else if !current.is_empty() {
            sequences.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        sequences.push(current);
    }
    sequences
}

pub(crate) fn keys(title: &str) -> TitleMatchKeys {
    let normalized = normalized(title);
    let (canonical, has_copy_marker) = canonical(title);
    let numbers = number_sequences(title);
    let numeric = if numbers.len() >= 2 && numbers.iter().map(String::len).sum::<usize>() >= 6 {
        numbers.join("\u{1f}")
    } else {
        String::new()
    };
    TitleMatchKeys {
        normalized,
        canonical,
        has_copy_marker,
        numeric,
    }
}

pub(crate) fn evidence(left: &str, right: &str) -> Option<&'static str> {
    let left = keys(left);
    let right = keys(right);
    if left.normalized == right.normalized {
        return Some(
            "Normalized titles match exactly; case and filename separators such as underscores are ignored.",
        );
    }
    if left.canonical == right.canonical && (left.has_copy_marker || right.has_copy_marker) {
        return Some(
            "Normalized titles match after removing a trailing duplicate-copy marker of (2) or higher.",
        );
    }
    if !left.numeric.is_empty() && left.numeric == right.numeric {
        return Some(
            "Possible filename/title match: the ordered multi-part number sequence matches. Review before upload or deletion.",
        );
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_title_keys_preserve_exact_copy_and_numeric_semantics() {
        assert_eq!(normalized("My_clip.MP4"), "my clip");
        assert_eq!(canonical("My clip (2).mp4"), ("my clip".into(), true));
        assert_eq!(canonical("Episode (1).mp4"), ("episode 1".into(), false));
        assert_eq!(canonical("Episode (2026).mp4"), ("episode".into(), true));
        assert!(evidence("My clip.mp4", "My clip (2).mp4").is_some());
        assert!(evidence("My clip.mp4", "My clip (1).mp4").is_none());
        assert!(evidence(
            "VID_20251218_195343_00_005.mp4",
            "VID 20251218 195343 00 005"
        )
        .is_some());
        assert!(evidence("Episode 12", "Episode 12").is_some());
        assert!(evidence("Episode 12 cut 3", "Episode 12 cut 4").is_none());
    }
}
