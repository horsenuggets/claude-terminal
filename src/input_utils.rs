//! Input manipulation utilities
//! Extracted for testability

/// Find the position of the previous word boundary in input
pub fn find_word_boundary_backward(input: &str, cursor_position: usize) -> usize {
    if cursor_position == 0 {
        return 0;
    }
    let bytes = input.as_bytes();
    let mut pos = cursor_position.min(bytes.len()) - 1;
    // Skip trailing whitespace
    while pos > 0 && bytes[pos].is_ascii_whitespace() {
        pos -= 1;
    }
    // Find start of word
    while pos > 0 && !bytes[pos - 1].is_ascii_whitespace() {
        pos -= 1;
    }
    pos
}

/// Find the position of the next word boundary in input
pub fn find_word_boundary_forward(input: &str, cursor_position: usize) -> usize {
    let len = input.len();
    if cursor_position >= len {
        return len;
    }
    let bytes = input.as_bytes();
    let mut pos = cursor_position;
    // Skip current word
    while pos < len && !bytes[pos].is_ascii_whitespace() {
        pos += 1;
    }
    // Skip whitespace
    while pos < len && bytes[pos].is_ascii_whitespace() {
        pos += 1;
    }
    pos
}

/// Delete the word before cursor, returning new string and cursor position
pub fn delete_word_backward(input: &str, cursor_position: usize) -> (String, usize) {
    let new_pos = find_word_boundary_backward(input, cursor_position);
    let mut new_input = input.to_string();
    new_input.drain(new_pos..cursor_position);
    (new_input, new_pos)
}

/// Delete from cursor to end of line
pub fn delete_to_end(input: &str, cursor_position: usize) -> String {
    input[..cursor_position].to_string()
}

/// Delete from beginning to cursor
pub fn delete_to_start(input: &str, cursor_position: usize) -> String {
    input[cursor_position..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_word_boundary_backward_simple() {
        let input = "hello world";
        assert_eq!(find_word_boundary_backward(input, 11), 6); // End -> start of "world"
        assert_eq!(find_word_boundary_backward(input, 6), 0);  // Start of "world" -> start
        assert_eq!(find_word_boundary_backward(input, 5), 0);  // Space -> start
    }

    #[test]
    fn test_word_boundary_backward_multiple_spaces() {
        let input = "hello   world";
        assert_eq!(find_word_boundary_backward(input, 13), 8); // End -> start of "world"
        assert_eq!(find_word_boundary_backward(input, 8), 0);  // Start of "world" -> start
    }

    #[test]
    fn test_word_boundary_backward_at_start() {
        let input = "hello";
        assert_eq!(find_word_boundary_backward(input, 0), 0);
    }

    #[test]
    fn test_word_boundary_forward_simple() {
        let input = "hello world";
        assert_eq!(find_word_boundary_forward(input, 0), 6);  // Start -> after "hello "
        assert_eq!(find_word_boundary_forward(input, 6), 11); // Start of "world" -> end
    }

    #[test]
    fn test_word_boundary_forward_at_end() {
        let input = "hello";
        assert_eq!(find_word_boundary_forward(input, 5), 5);
    }

    #[test]
    fn test_delete_word_backward() {
        let (new_input, new_pos) = delete_word_backward("hello world", 11);
        assert_eq!(new_input, "hello ");
        assert_eq!(new_pos, 6);
    }

    #[test]
    fn test_delete_word_backward_multiple() {
        let (s1, p1) = delete_word_backward("one two three", 13);
        assert_eq!(s1, "one two ");
        assert_eq!(p1, 8);

        let (s2, p2) = delete_word_backward(&s1, p1);
        assert_eq!(s2, "one ");
        assert_eq!(p2, 4);
    }

    #[test]
    fn test_delete_to_end() {
        assert_eq!(delete_to_end("hello world", 6), "hello ");
        assert_eq!(delete_to_end("hello world", 0), "");
        assert_eq!(delete_to_end("hello world", 11), "hello world");
    }

    #[test]
    fn test_delete_to_start() {
        assert_eq!(delete_to_start("hello world", 6), "world");
        assert_eq!(delete_to_start("hello world", 0), "hello world");
        assert_eq!(delete_to_start("hello world", 11), "");
    }

    #[test]
    fn test_empty_input() {
        assert_eq!(find_word_boundary_backward("", 0), 0);
        assert_eq!(find_word_boundary_forward("", 0), 0);
        let (s, p) = delete_word_backward("", 0);
        assert_eq!(s, "");
        assert_eq!(p, 0);
    }

    #[test]
    fn test_single_word() {
        let input = "hello";
        assert_eq!(find_word_boundary_backward(input, 3), 0);
        assert_eq!(find_word_boundary_forward(input, 3), 5);
    }

    #[test]
    fn test_with_special_chars() {
        let input = "hello-world test";
        // hyphen is not whitespace, so treated as part of word
        assert_eq!(find_word_boundary_backward(input, 11), 0); // "hello-world" is one word
        assert_eq!(find_word_boundary_forward(input, 0), 12);
    }
}
