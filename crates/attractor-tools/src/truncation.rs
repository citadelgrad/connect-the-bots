/// Output truncation strategies for tool results.

/// How to truncate output that exceeds the maximum character limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TruncationMode {
    /// Keep first 40% and last 60%, with a marker in the middle.
    HeadTail,
    /// Keep last `max_chars`, with a marker at the start.
    Tail,
}

/// Truncate `output` to at most `max_chars` characters using the given mode.
///
/// If the output is within the limit, it is returned unchanged.
/// Otherwise a warning marker is inserted indicating how many characters were removed.
pub fn truncate_output(output: &str, max_chars: usize, mode: TruncationMode) -> String {
    if output.len() <= max_chars {
        return output.to_string();
    }

    match mode {
        TruncationMode::HeadTail => {
            let head_size = max_chars * 40 / 100;
            let tail_size = max_chars - head_size;
            let removed = output.len() - head_size - tail_size;
            let head = &output[..head_size];
            let tail = &output[output.len() - tail_size..];
            format!(
                "{}\n[WARNING: Output truncated. {} characters removed from middle]\n{}",
                head, removed, tail
            )
        }
        TruncationMode::Tail => {
            let kept = max_chars;
            let removed = output.len() - kept;
            let tail = &output[output.len() - kept..];
            format!(
                "\n[WARNING: Output truncated. {} characters removed from start]\n{}",
                removed, tail
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_truncation_when_within_limit() {
        let input = "short";
        let result = truncate_output(input, 100, TruncationMode::HeadTail);
        assert_eq!(result, input);
    }

    #[test]
    fn head_tail_truncation() {
        // Create a string of 100 chars
        let input: String = (0..100).map(|i| char::from(b'a' + (i % 26))).collect();
        let result = truncate_output(&input, 50, TruncationMode::HeadTail);

        assert!(result.contains("[WARNING: Output truncated."));
        assert!(result.contains("characters removed from middle"));
        // Head is 40% of 50 = 20 chars, tail is 30 chars
        assert!(result.starts_with(&input[..20]));
        assert!(result.ends_with(&input[70..]));
    }

    #[test]
    fn tail_truncation() {
        let input: String = (0..100).map(|i| char::from(b'a' + (i % 26))).collect();
        let result = truncate_output(&input, 50, TruncationMode::Tail);

        assert!(result.contains("[WARNING: Output truncated."));
        assert!(result.contains("characters removed from start"));
        assert!(result.ends_with(&input[50..]));
    }
}
