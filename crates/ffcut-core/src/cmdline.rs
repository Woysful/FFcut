//! argv <-> editable text.
//!
//! Rules (deliberately small, and Windows-path friendly):
//! * whitespace separates arguments;
//! * `"..."` groups; inside it `\"` is a quote, every other backslash is literal;
//! * `'...'` groups with no escapes at all (handy for pasted Linux commands);
//! * a backslash outside quotes is literal, so `C:\videos\a.mp4` just works.
//!
//! Known limit: an argument that *ends* with a backslash inside double quotes
//! (`"C:\dir\"`) is ambiguous; real file paths never end like that.

#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    UnterminatedQuote(char),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::UnterminatedQuote(q) => write!(f, "unclosed quote {q}"),
        }
    }
}

pub fn parse(text: &str) -> Result<Vec<String>, ParseError> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_arg = false;
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {
                if in_arg {
                    out.push(std::mem::take(&mut cur));
                    in_arg = false;
                }
            }
            '"' => {
                in_arg = true;
                loop {
                    match chars.next() {
                        None => return Err(ParseError::UnterminatedQuote('"')),
                        Some('"') => break,
                        Some('\\') if chars.peek() == Some(&'"') => {
                            cur.push('"');
                            chars.next();
                        }
                        Some(ch) => cur.push(ch),
                    }
                }
            }
            '\'' => {
                in_arg = true;
                loop {
                    match chars.next() {
                        None => return Err(ParseError::UnterminatedQuote('\'')),
                        Some('\'') => break,
                        Some(ch) => cur.push(ch),
                    }
                }
            }
            c => {
                in_arg = true;
                cur.push(c);
            }
        }
    }
    if in_arg {
        out.push(cur);
    }
    Ok(out)
}

fn needs_quotes(arg: &str) -> bool {
    arg.is_empty()
        || arg.chars().any(|c| {
            c.is_whitespace() || matches!(c, '"' | '\'' | '&' | '|' | ';' | '(' | ')' | '<' | '>' | '$' | '`' | '*' | '?' | '!')
        })
}

pub fn quote(arg: &str) -> String {
    if needs_quotes(arg) {
        format!("\"{}\"", arg.replace('"', "\\\""))
    } else {
        arg.to_string()
    }
}

pub fn to_text(args: &[String]) -> String {
    args.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_and_quotes() {
        assert_eq!(parse("-i  a.mp4   out.mkv").unwrap(), ["-i", "a.mp4", "out.mkv"]);
        assert_eq!(parse(r#"-vf "crop=1:2:3:4,hflip" "my file.mp4""#).unwrap(), ["-vf", "crop=1:2:3:4,hflip", "my file.mp4"]);
        assert_eq!(parse("-metadata 'title=it is'").unwrap(), ["-metadata", "title=it is"]);
        assert_eq!(parse(r#"x "" y"#).unwrap(), ["x", "", "y"]);
    }

    #[test]
    fn windows_paths_stay_intact() {
        assert_eq!(parse(r#"-i "C:\Users\me\a b.mp4""#).unwrap(), ["-i", r"C:\Users\me\a b.mp4"]);
        assert_eq!(parse(r"-i C:\v\a.mp4").unwrap(), ["-i", r"C:\v\a.mp4"]);
    }

    #[test]
    fn errors_on_unclosed_quote() {
        assert_eq!(parse(r#"-i "a.mp4"#), Err(ParseError::UnterminatedQuote('"')));
    }

    #[test]
    fn roundtrip() {
        let args: Vec<String> = ["ffmpeg", "-i", r"C:\my videos\a.mp4", "-metadata", r#"title=say "hi""#, "-vf", "scale=-1:720,setsar=1", "-filter_complex", "[0:a:0][0:a:1]amix=inputs=2[aout]", ""]
            .iter().map(|s| s.to_string()).collect();
        assert_eq!(parse(&to_text(&args)).unwrap(), args);
    }
}
