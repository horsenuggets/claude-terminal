//! Word wrapping utilities for text rendering

/// Word-wrap text to fit within a given width, breaking at word boundaries.
/// Returns a vector of lines that fit within the specified max_width.
pub fn word_wrap(text: &str, max_width: usize) -> Vec<String> {
    let mut result = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0;

    for word in text.split_whitespace() {
        let word_width = word.chars().count();

        if current_width == 0 {
            // First word on line
            if word_width > max_width {
                // Word is longer than max width, need to break it
                let mut remaining = word;
                while !remaining.is_empty() {
                    let chars: Vec<char> = remaining.chars().collect();
                    let chunk_len = chars.len().min(max_width);
                    let chunk: String = chars[..chunk_len].iter().collect();
                    result.push(chunk);
                    remaining = &remaining[chars[..chunk_len].iter().map(|c| c.len_utf8()).sum::<usize>()..];
                }
            } else {
                current_line = word.to_string();
                current_width = word_width;
            }
        } else if current_width + 1 + word_width <= max_width {
            // Word fits with space
            current_line.push(' ');
            current_line.push_str(word);
            current_width += 1 + word_width;
        } else {
            // Word doesn't fit, start new line
            result.push(current_line);
            if word_width > max_width {
                // Word is longer than max width, need to break it
                let mut remaining = word;
                while !remaining.is_empty() {
                    let chars: Vec<char> = remaining.chars().collect();
                    let chunk_len = chars.len().min(max_width);
                    let chunk: String = chars[..chunk_len].iter().collect();
                    result.push(chunk);
                    remaining = &remaining[chars[..chunk_len].iter().map(|c| c.len_utf8()).sum::<usize>()..];
                }
                current_line = String::new();
                current_width = 0;
            } else {
                current_line = word.to_string();
                current_width = word_width;
            }
        }
    }

    // Don't forget the last line
    if !current_line.is_empty() {
        result.push(current_line);
    }

    // Handle empty input
    if result.is_empty() {
        result.push(String::new());
    }

    result
}

/// Calculate the number of visual lines needed to display text with word wrapping.
/// This is used to calculate the height of the input area.
pub fn calculate_wrapped_line_count(text: &str, width: usize) -> usize {
    if width == 0 {
        return 1;
    }

    let mut total_lines = 0;
    for line in text.split('\n') {
        let wrapped = word_wrap(line, width);
        total_lines += wrapped.len();
    }

    total_lines.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_word_wrap_simple() {
        let result = word_wrap("hello world", 20);
        assert_eq!(result, vec!["hello world"]);
    }

    #[test]
    fn test_word_wrap_breaks_at_word_boundary() {
        let result = word_wrap("hello world foo bar", 11);
        assert_eq!(result, vec!["hello world", "foo bar"]);
    }

    #[test]
    fn test_word_wrap_empty() {
        let result = word_wrap("", 20);
        assert_eq!(result, vec![""]);
    }

    #[test]
    fn test_word_wrap_long_word() {
        let result = word_wrap("superlongword", 5);
        assert_eq!(result, vec!["super", "longw", "ord"]);
    }

    #[test]
    fn test_calculate_wrapped_line_count() {
        assert_eq!(calculate_wrapped_line_count("hello world", 20), 1);
        assert_eq!(calculate_wrapped_line_count("hello world foo bar", 11), 2);
        assert_eq!(calculate_wrapped_line_count("line1\nline2", 20), 2);
    }
}
