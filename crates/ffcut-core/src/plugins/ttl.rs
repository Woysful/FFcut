//! Just enough Turtle to read LV2 bundles: prefixes, IRIs, prefixed names, literals (numbers, booleans,
//! strings, long strings), `a`, `;` `,` `[ ]` and `( )`. Everything is kept as triples of text.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Term {
    Iri(String),
    Blank(usize),
    /// The lexical form (`"1.5"`, `"true"`, a string without its quotes); a language or a datatype is dropped.
    Lit(String),
}

impl Term {
    pub fn lit(&self) -> Option<&str> {
        match self {
            Term::Lit(s) => Some(s),
            _ => None,
        }
    }
    pub fn iri(&self) -> Option<&str> {
        match self {
            Term::Iri(s) => Some(s),
            _ => None,
        }
    }
}

pub const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";

/// Triples of one or more files.
#[derive(Debug, Default)]
pub struct Graph {
    triples: Vec<(Term, String, Term)>,
    by_subject: HashMap<Term, Vec<usize>>,
    blanks: usize,
}

impl Graph {
    fn add(&mut self, s: Term, p: String, o: Term) {
        self.by_subject.entry(s.clone()).or_default().push(self.triples.len());
        self.triples.push((s, p, o));
    }
    fn blank(&mut self) -> Term {
        self.blanks += 1;
        Term::Blank(self.blanks)
    }
    /// Every object of `s p ?`.
    pub fn objects(&self, s: &Term, p: &str) -> std::vec::IntoIter<&Term> {
        let v: Vec<&Term> = self.by_subject.get(s).into_iter().flatten().map(|&i| &self.triples[i]).filter(|t| t.1 == p).map(|t| &t.2).collect();
        v.into_iter()
    }
    pub fn object(&self, s: &Term, p: &str) -> Option<&Term> {
        self.objects(s, p).next()
    }
    /// Every subject of `? p o`.
    pub fn subjects(&self, p: &str, o: &Term) -> std::vec::IntoIter<&Term> {
        let v: Vec<&Term> = self.triples.iter().filter(|t| t.1 == p && &t.2 == o).map(|t| &t.0).collect();
        v.into_iter()
    }
    pub fn is_a(&self, s: &Term, class: &str) -> bool {
        self.objects(s, RDF_TYPE).any(|t| t.iri() == Some(class))
    }
    pub fn len(&self) -> usize {
        self.triples.len()
    }
    pub fn is_empty(&self) -> bool {
        self.triples.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Iri(String),
    /// `prefix:local` (also `_:label`), not yet expanded.
    Name(String, String),
    Lit(String),
    A,
    Prefix,
    Base,
    Punct(char),
}

struct Lexer<'a> {
    s: &'a [u8],
    i: usize,
}

impl Lexer<'_> {
    fn skip_space(&mut self) {
        while self.i < self.s.len() {
            match self.s[self.i] {
                b' ' | b'\t' | b'\r' | b'\n' => self.i += 1,
                b'#' => {
                    while self.i < self.s.len() && self.s[self.i] != b'\n' {
                        self.i += 1;
                    }
                }
                _ => break,
            }
        }
    }

    fn text(&self, a: usize, b: usize) -> String {
        String::from_utf8_lossy(&self.s[a..b]).into_owned()
    }

    fn string(&mut self, q: u8) -> Result<String, String> {
        let long = self.s[self.i..].starts_with(&[q, q, q]);
        self.i += if long { 3 } else { 1 };
        let mut out = Vec::new();
        loop {
            let Some(&c) = self.s.get(self.i) else { return Err("a string is not closed".into()) };
            if c == q && (!long || self.s[self.i..].starts_with(&[q, q, q])) {
                self.i += if long { 3 } else { 1 };
                break;
            }
            if c == b'\\' {
                let e = *self.s.get(self.i + 1).ok_or("a string is not closed")?;
                self.i += 2;
                match e {
                    b'n' => out.push(b'\n'),
                    b't' => out.push(b'\t'),
                    b'r' => out.push(b'\r'),
                    b'u' | b'U' => {
                        let n = if e == b'u' { 4 } else { 8 };
                        let hex = self.s.get(self.i..self.i + n).ok_or("a bad escape")?;
                        let ch = u32::from_str_radix(&String::from_utf8_lossy(hex), 16).ok().and_then(char::from_u32).unwrap_or('\u{FFFD}');
                        out.extend_from_slice(ch.to_string().as_bytes());
                        self.i += n;
                    }
                    other => out.push(other),
                }
                continue;
            }
            if !long && c == b'\n' {
                return Err("a line break in a short string".into());
            }
            out.push(c);
            self.i += 1;
        }
        // a language tag or a datatype: dropped
        if self.s.get(self.i) == Some(&b'@') {
            self.i += 1;
            while self.s.get(self.i).is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'-') {
                self.i += 1;
            }
        } else if self.s[self.i..].starts_with(b"^^") {
            self.i += 2;
            self.skip_space();
            self.next()?;
        }
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    fn name_char(c: u8) -> bool {
        c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.' | b'%' | b'\\') || c >= 0x80
    }

    fn next(&mut self) -> Result<Option<Tok>, String> {
        self.skip_space();
        let Some(&c) = self.s.get(self.i) else { return Ok(None) };
        let start = self.i;
        let tok = match c {
            b'<' => {
                let end = self.s[self.i..].iter().position(|&c| c == b'>').ok_or("an IRI is not closed")? + self.i;
                self.i = end + 1;
                Tok::Iri(self.text(start + 1, end))
            }
            b'"' | b'\'' => Tok::Lit(self.string(c)?),
            b'.' if !self.s.get(self.i + 1).is_some_and(u8::is_ascii_digit) => {
                self.i += 1;
                Tok::Punct('.')
            }
            b';' | b',' | b'[' | b']' | b'(' | b')' => {
                self.i += 1;
                Tok::Punct(c as char)
            }
            b'@' => {
                self.i += 1;
                while self.s.get(self.i).is_some_and(u8::is_ascii_alphabetic) {
                    self.i += 1;
                }
                match &self.s[start + 1..self.i] {
                    b"prefix" => Tok::Prefix,
                    b"base" => Tok::Base,
                    _ => Tok::Lit(String::new()),
                }
            }
            b'+' | b'-' | b'0'..=b'9' | b'.' => {
                self.i += 1;
                while let Some(&d) = self.s.get(self.i) {
                    let exp_sign = (d == b'+' || d == b'-') && matches!(self.s[self.i - 1], b'e' | b'E');
                    let dot = d == b'.' && self.s.get(self.i + 1).is_some_and(u8::is_ascii_digit);
                    if d.is_ascii_digit() || d == b'e' || d == b'E' || exp_sign || dot {
                        self.i += 1;
                    } else {
                        break;
                    }
                }
                Tok::Lit(self.text(start, self.i).trim_start_matches('+').to_string())
            }
            _ => {
                while self.s.get(self.i).is_some_and(|&c| Self::name_char(c) || c == b':') {
                    self.i += 1;
                }
                // a name never ends with a dot: that is the end of the statement
                while self.i > start && self.s[self.i - 1] == b'.' {
                    self.i -= 1;
                }
                if self.i == start {
                    return Err(format!("unexpected “{}”", c as char));
                }
                let word = self.text(start, self.i);
                match word.as_str() {
                    "a" => Tok::A,
                    "true" | "false" => Tok::Lit(word),
                    w if w.eq_ignore_ascii_case("prefix") => Tok::Prefix,
                    w if w.eq_ignore_ascii_case("base") => Tok::Base,
                    _ => {
                        let (p, l) = word.split_once(':').ok_or_else(|| format!("unexpected “{word}”"))?;
                        Tok::Name(p.to_string(), l.replace('\\', ""))
                    }
                }
            }
        };
        Ok(Some(tok))
    }
}

struct Parser<'a, 'g> {
    lex: Lexer<'a>,
    peeked: Option<Option<Tok>>,
    prefixes: HashMap<String, String>,
    base: String,
    labels: HashMap<String, Term>,
    g: &'g mut Graph,
}

/// `rel` against `base` (a directory or file IRI): enough for the file names LV2 bundles use.
pub fn resolve(base: &str, rel: &str) -> String {
    let has_scheme = rel.find(':').is_some_and(|i| i > 0 && rel[..i].chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c)));
    if has_scheme {
        return rel.to_string();
    }
    if rel.is_empty() {
        return base.to_string();
    }
    let dir = &base[..base.rfind('/').map_or(0, |i| i + 1)];
    format!("{dir}{rel}")
}

impl Parser<'_, '_> {
    fn peek(&mut self) -> Result<Option<&Tok>, String> {
        if self.peeked.is_none() {
            self.peeked = Some(self.lex.next()?);
        }
        Ok(self.peeked.as_ref().and_then(|t| t.as_ref()))
    }
    fn take(&mut self) -> Result<Option<Tok>, String> {
        match self.peeked.take() {
            Some(t) => Ok(t),
            None => self.lex.next(),
        }
    }
    fn expect(&mut self, c: char) -> Result<(), String> {
        match self.take()? {
            Some(Tok::Punct(p)) if p == c => Ok(()),
            other => Err(format!("expected “{c}”, found {other:?}")),
        }
    }
    fn at(&mut self, c: char) -> Result<bool, String> {
        Ok(matches!(self.peek()?, Some(Tok::Punct(p)) if *p == c))
    }

    fn name(&mut self, p: &str, l: &str) -> Result<Term, String> {
        if p == "_" {
            let g = &mut *self.g;
            return Ok(self.labels.entry(l.to_string()).or_insert_with(|| g.blank()).clone());
        }
        let ns = self.prefixes.get(p).ok_or_else(|| format!("unknown prefix “{p}:”"))?;
        Ok(Term::Iri(format!("{ns}{l}")))
    }

    fn iri_tok(&mut self, t: Tok) -> Result<String, String> {
        match t {
            Tok::Iri(i) => Ok(resolve(&self.base, &i)),
            Tok::Name(p, l) => match self.name(&p, &l)? {
                Term::Iri(i) => Ok(i),
                _ => Err("a blank node cannot be a predicate".into()),
            },
            Tok::A => Ok(RDF_TYPE.to_string()),
            other => Err(format!("expected a predicate, found {other:?}")),
        }
    }

    fn statement(&mut self) -> Result<bool, String> {
        let Some(t) = self.take()? else { return Ok(false) };
        match t {
            Tok::Prefix | Tok::Base => {
                if t == Tok::Prefix {
                    let Some(Tok::Name(p, l)) = self.take()? else { return Err("a bad @prefix".into()) };
                    if !l.is_empty() {
                        return Err("a bad @prefix".into());
                    }
                    let Some(Tok::Iri(i)) = self.take()? else { return Err("a bad @prefix".into()) };
                    let i = resolve(&self.base, &i);
                    self.prefixes.insert(p, i);
                } else {
                    let Some(Tok::Iri(i)) = self.take()? else { return Err("a bad @base".into()) };
                    self.base = resolve(&self.base, &i);
                }
                // `@prefix` ends with a dot, SPARQL's `PREFIX` does not
                if self.at('.')? {
                    self.take()?;
                }
            }
            Tok::Punct('[') => {
                let s = self.g.blank();
                if !self.at(']')? {
                    self.pred_objects(&s)?;
                }
                self.expect(']')?;
                if !self.at('.')? {
                    self.pred_objects(&s)?;
                }
                self.expect('.')?;
            }
            t => {
                let s = self.subject(t)?;
                self.pred_objects(&s)?;
                self.expect('.')?;
            }
        }
        Ok(true)
    }

    fn subject(&mut self, t: Tok) -> Result<Term, String> {
        match t {
            Tok::Iri(i) => Ok(Term::Iri(resolve(&self.base, &i))),
            Tok::Name(p, l) => self.name(&p, &l),
            Tok::Punct('(') => self.collection(),
            other => Err(format!("expected a subject, found {other:?}")),
        }
    }

    fn pred_objects(&mut self, s: &Term) -> Result<(), String> {
        loop {
            let t = self.take()?.ok_or("the file ends inside a statement")?;
            let p = self.iri_tok(t)?;
            loop {
                let o = self.object()?;
                self.g.add(s.clone(), p.clone(), o);
                if !self.at(',')? {
                    break;
                }
                self.take()?;
            }
            if !self.at(';')? {
                return Ok(());
            }
            while self.at(';')? {
                self.take()?;
            }
            if self.at('.')? || self.at(']')? {
                return Ok(());
            }
        }
    }

    fn object(&mut self) -> Result<Term, String> {
        match self.take()?.ok_or("the file ends inside a statement")? {
            Tok::Iri(i) => Ok(Term::Iri(resolve(&self.base, &i))),
            Tok::Name(p, l) => self.name(&p, &l),
            Tok::Lit(s) => Ok(Term::Lit(s)),
            Tok::Punct('[') => {
                let b = self.g.blank();
                if !self.at(']')? {
                    self.pred_objects(&b)?;
                }
                self.expect(']')?;
                Ok(b)
            }
            Tok::Punct('(') => self.collection(),
            other => Err(format!("expected an object, found {other:?}")),
        }
    }

    fn collection(&mut self) -> Result<Term, String> {
        let mut items = Vec::new();
        while !self.at(')')? {
            items.push(self.object()?);
        }
        self.expect(')')?;
        let mut list = Term::Iri(RDF_NIL.to_string());
        for item in items.into_iter().rev() {
            let b = self.g.blank();
            self.g.add(b.clone(), RDF_FIRST.into(), item);
            self.g.add(b.clone(), RDF_REST.into(), list);
            list = b;
        }
        Ok(list)
    }
}

/// Adds the triples of `text` (read from `base`, a file IRI) to `g`. On an error the triples before it stay.
pub fn parse(text: &str, base: &str, g: &mut Graph) -> Result<(), String> {
    let mut p = Parser { lex: Lexer { s: text.as_bytes(), i: 0 }, peeked: None, prefixes: HashMap::new(), base: base.to_string(), labels: HashMap::new(), g };
    while p.statement()? {}
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LV2: &str = r#"@prefix : <http://lv2plug.in/ns/lv2core#> .
@prefix doap: <http://usefulinc.com/ns/doap#> .
PREFIX swh: <http://plugin.org.uk/swh-plugins/>
# a comment with a "quote"
swh:sc4 a :Plugin ;
   a :CompressorPlugin ;
   doap:name "SC4"@en ;
   :binary <plugin-linux.so> ;
   :port [
     a :InputPort, :ControlPort ;
     :name "Attack \"time\" (ms)" ;
     :index 1 ;
     :minimum -1.5e1 ;
     :maximum +400 ;
     :default 101.125 ;
     :scalePoint [ :label 'one' ; :value 1.0 ] ;
   ] ;
   :code """
      #include <x.h>
      "quoted" ;
    """ ;
   :list ( 1 2 ) ;
   :ok true .
[] a :Thing ; :of _:x .
_:x :name "x" .
"#;

    #[test]
    fn reads_an_lv2_description() {
        let mut g = Graph::default();
        parse(LV2, "file:///usr/lib/lv2/sc4.lv2/plugin.ttl", &mut g).unwrap();
        let lv2 = |l: &str| format!("http://lv2plug.in/ns/lv2core#{l}");
        let s = Term::Iri("http://plugin.org.uk/swh-plugins/sc4".into());
        assert!(g.is_a(&s, &lv2("Plugin")) && g.is_a(&s, &lv2("CompressorPlugin")));
        assert_eq!(g.object(&s, "http://usefulinc.com/ns/doap#name").and_then(Term::lit), Some("SC4"));
        assert_eq!(g.object(&s, &lv2("binary")).and_then(Term::iri), Some("file:///usr/lib/lv2/sc4.lv2/plugin-linux.so"));
        let port = g.object(&s, &lv2("port")).unwrap().clone();
        assert!(g.is_a(&port, &lv2("ControlPort")));
        assert_eq!(g.object(&port, &lv2("name")).and_then(Term::lit), Some("Attack \"time\" (ms)"));
        assert_eq!(g.object(&port, &lv2("minimum")).and_then(Term::lit), Some("-1.5e1"));
        assert_eq!(g.object(&port, &lv2("maximum")).and_then(Term::lit), Some("400"));
        let sp = g.object(&port, &lv2("scalePoint")).unwrap().clone();
        assert_eq!(g.object(&sp, &lv2("label")).and_then(Term::lit), Some("one"));
        assert!(g.object(&s, &lv2("code")).and_then(Term::lit).unwrap().contains("\"quoted\" ;"));
        assert_eq!(g.object(&s, &lv2("ok")).and_then(Term::lit), Some("true"));
        let thing = g.subjects(RDF_TYPE, &Term::Iri(lv2("Thing"))).next().unwrap().clone();
        let x = g.object(&thing, &lv2("of")).unwrap().clone();
        assert_eq!(g.object(&x, &lv2("name")).and_then(Term::lit), Some("x"));
    }

    #[test]
    fn errors_keep_what_came_before() {
        let mut g = Graph::default();
        assert!(parse("@prefix : <x#> . :a :b 1 . :c :d \"open", "file:///t", &mut g).is_err());
        assert_eq!(g.len(), 1);
        assert!(parse(":a :b 1 .", "file:///t", &mut Graph::default()).is_err());
        assert_eq!(resolve("file:///a/b/manifest.ttl", "c.ttl"), "file:///a/b/c.ttl");
        assert_eq!(resolve("file:///a/b/", "urn:x"), "urn:x");
    }
}
