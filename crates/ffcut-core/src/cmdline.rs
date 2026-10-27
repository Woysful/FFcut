//! argv <-> editable text.
//!
//! Rules (deliberately small, and Windows-path friendly):
//! * whitespace separates arguments;
//! * `"..."` groups; inside it a backslash is literal unless backslashes run up to a `"`: then
//!   2n of them + `"` = n backslashes and the end of the group, 2n+1 = n backslashes and a literal `"`
//!   (so `\"` is a quote and `"C:\dir\\"` is `C:\dir\`);
//! * `'...'` groups with no escapes at all (handy for pasted Linux commands);
//! * a backslash outside quotes is literal, so `C:\videos\a.mp4` just works.
//!
//! `quote` writes exactly what `parse` reads back, including arguments that end with a backslash.

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
                        Some('\\') => {
                            let mut run = 1;
                            while chars.peek() == Some(&'\\') {
                                chars.next();
                                run += 1;
                            }
                            if chars.peek() == Some(&'"') {
                                cur.extend(std::iter::repeat_n('\\', run / 2));
                                if run % 2 == 1 {
                                    cur.push('"');
                                    chars.next();
                                } // else the quote closes the group, on the next turn
                            } else {
                                cur.extend(std::iter::repeat_n('\\', run));
                            }
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
    arg.is_empty() || arg.chars().any(|c| c.is_whitespace() || matches!(c, '"' | '\'' | '&' | '|' | ';' | '(' | ')' | '<' | '>' | '$' | '`' | '*' | '?' | '!'))
}

pub fn quote(arg: &str) -> String {
    if !needs_quotes(arg) {
        return arg.to_string();
    }
    let mut out = String::with_capacity(arg.len() + 2);
    out.push('"');
    // backslashes are written once they are known not to precede a quote (or the closing quote)
    let mut run = 0;
    for c in arg.chars() {
        match c {
            '\\' => run += 1,
            '"' => {
                out.extend(std::iter::repeat_n('\\', run * 2 + 1));
                out.push('"');
                run = 0;
            }
            c => {
                out.extend(std::iter::repeat_n('\\', run));
                out.push(c);
                run = 0;
            }
        }
    }
    out.extend(std::iter::repeat_n('\\', run * 2));
    out.push('"');
    out
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
        let args: Vec<String> = [
            "ffmpeg",
            "-i",
            r"C:\my videos\a.mp4",
            "-metadata",
            r#"title=say "hi""#,
            "-vf",
            "scale=-1:720,setsar=1",
            "-filter_complex",
            "[0:a:0][0:a:1]amix=inputs=2[aout]",
            "",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(parse(&to_text(&args)).unwrap(), args);
    }

    #[test]
    fn backslashes_before_quotes_roundtrip() {
        let args: Vec<String> = [r"C:\my dir\", r"C:\my dir\\", r#"a\"b c"#, r#"say \\"x" "#, r"\\server\share\a b.mp4", r#"text='it'\\\''s "q"'"#]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let text = to_text(&args);
        assert_eq!(parse(&text).unwrap(), args, "{text}");
        assert_eq!(quote(r"C:\my dir\"), r#""C:\my dir\\""#);
        // what people type keeps working
        assert_eq!(parse(r#""C:\my dir\a.mp4" "say \"hi\"""#).unwrap(), [r"C:\my dir\a.mp4", r#"say "hi""#]);
    }
}
