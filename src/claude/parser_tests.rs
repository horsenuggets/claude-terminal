//! Tests for the stream parser

#[cfg(test)]
mod tests {
    use super::super::parser::StreamParser;
    use super::super::types::StreamEvent;

    #[test]
    fn test_parse_assistant_message() {
        let mut parser = StreamParser::new();
        let line = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Hello!"}],"usage":{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}"#;

        let events = parser.parse_line(line).unwrap();

        assert!(!events.is_empty());
        let has_text = events.iter().any(|e| matches!(e, StreamEvent::Text(t) if t == "Hello!"));
        assert!(has_text, "Should contain text event with 'Hello!'");
    }

    #[test]
    fn test_parse_content_block_delta() {
        let mut parser = StreamParser::new();
        let line = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"World"}}"#;

        let events = parser.parse_line(line).unwrap();

        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], StreamEvent::Text(t) if t == "World"));
    }

    #[test]
    fn test_parse_tool_use() {
        let mut parser = StreamParser::new();

        // Start tool use
        let start = r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"123","name":"Read","input":{}}}"#;
        let events = parser.parse_line(start).unwrap();
        // Tool use with empty input should emit immediately
        assert!(events.iter().any(|e| matches!(e, StreamEvent::ToolUse { name, .. } if name == "Read")));
    }

    #[test]
    fn test_parse_system_event_ignored() {
        let mut parser = StreamParser::new();
        let line = r#"{"type":"system","subtype":"init","session_id":"abc"}"#;

        let events = parser.parse_line(line).unwrap();
        assert!(events.is_empty(), "System events should be ignored");
    }

    #[test]
    fn test_parse_empty_line() {
        let mut parser = StreamParser::new();
        let events = parser.parse_line("").unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn test_parse_invalid_json() {
        let mut parser = StreamParser::new();
        let events = parser.parse_line("not json").unwrap();
        assert!(events.is_empty(), "Invalid JSON should return empty events");
    }

    #[test]
    fn test_parse_usage_info() {
        let mut parser = StreamParser::new();
        let line = r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"input_tokens":100,"output_tokens":50,"cache_read_input_tokens":25,"cache_creation_input_tokens":10}}"#;

        let events = parser.parse_line(line).unwrap();

        let usage = events.iter().find_map(|e| {
            if let StreamEvent::Usage {
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cache_write_tokens,
            } = e
            {
                Some((*input_tokens, *output_tokens, *cache_read_tokens, *cache_write_tokens))
            } else {
                None
            }
        });

        assert_eq!(usage, Some((100, 50, 25, 10)));
    }

    #[test]
    fn test_parse_thinking_delta() {
        let mut parser = StreamParser::new();
        let line = r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Let me think..."}}"#;

        let events = parser.parse_line(line).unwrap();

        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], StreamEvent::Thinking(t) if t == "Let me think..."));
    }

    #[test]
    fn test_streaming_accumulation() {
        let mut parser = StreamParser::new();

        // Simulate multiple text deltas
        let events1 = parser.parse_line(r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#).unwrap();
        let events2 = parser.parse_line(r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" "}}"#).unwrap();
        let events3 = parser.parse_line(r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"World"}}"#).unwrap();

        assert!(matches!(&events1[0], StreamEvent::Text(t) if t == "Hello"));
        assert!(matches!(&events2[0], StreamEvent::Text(t) if t == " "));
        assert!(matches!(&events3[0], StreamEvent::Text(t) if t == "World"));
    }
}
