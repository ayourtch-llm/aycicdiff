/// A token produced by the lexer — one meaningful line of config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub indent: usize,
    pub text: String,
    pub line_no: usize,
}

/// Lines to skip in show-run preamble/postamble.
fn is_preamble_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("Building configuration")
        || trimmed.starts_with("Current configuration")
        || trimmed.starts_with("Last configuration change")
        || trimmed.starts_with("NVRAM config last updated")
        || trimmed.starts_with("! Last configuration")
        || trimmed.starts_with("! NVRAM config")
}

/// Tokenize IOS config text into a sequence of Tokens.
pub fn tokenize(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut in_preamble = true;

    for (line_idx, line) in input.lines().enumerate() {
        let line_no = line_idx + 1;

        // Skip completely empty lines
        if line.trim().is_empty() {
            continue;
        }

        // Skip comment/separator lines (just "!")
        if line.trim() == "!" {
            // A bang at indent 0 ends a section context, but we don't
            // need to emit it as a token — the indent change handles it.
            continue;
        }

        // Skip preamble lines
        if in_preamble && is_preamble_line(line) {
            continue;
        }
        in_preamble = false;

        // Skip "end" at indent 0 (end of config marker)
        if line.trim() == "end" {
            let indent = line.len() - line.trim_start().len();
            if indent == 0 {
                break;
            }
        }

        // Calculate indent (number of leading spaces)
        let indent = line.len() - line.trim_start().len();
        let text = line.trim().to_string();

        // Skip inline comments that are standalone (just "!" with possible indent)
        // We already handled bare "!" above, but handle "! some comment" too
        if text.starts_with("! ") {
            continue;
        }

        tokens.push(Token {
            indent,
            text,
            line_no,
        });
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_simple() {
        let input = "\
Building configuration...

Current configuration : 1234 bytes
!
hostname Router1
!
interface GigabitEthernet0/0
 ip address 10.0.0.1 255.255.255.0
 no shutdown
!
end
";
        let tokens = tokenize(input);
        assert_eq!(tokens.len(), 4);
        assert_eq!(tokens[0].text, "hostname Router1");
        assert_eq!(tokens[0].indent, 0);
        assert_eq!(tokens[1].text, "interface GigabitEthernet0/0");
        assert_eq!(tokens[1].indent, 0);
        assert_eq!(tokens[2].text, "ip address 10.0.0.1 255.255.255.0");
        assert_eq!(tokens[2].indent, 1);
        assert_eq!(tokens[3].text, "no shutdown");
        assert_eq!(tokens[3].indent, 1);
    }

    #[test]
    fn test_tokenize_skips_preamble() {
        let input = "Building configuration...\n\nCurrent configuration : 100 bytes\nhostname R1\nend\n";
        let tokens = tokenize(input);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text, "hostname R1");
    }
}
