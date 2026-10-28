//! LV2 bundles: a folder with `manifest.ttl`, which names the plugins and the files that describe them.

use super::ttl::{self, Graph, Term};
use super::{Control, CtlKind, PluginInfo, PluginKind};
use crate::fmtnum::num;
use std::path::{Path, PathBuf};

const LV2: &str = "http://lv2plug.in/ns/lv2core#";
const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const RDF_VALUE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#value";
const DOAP: &str = "http://usefulinc.com/ns/doap#";
const FOAF_NAME: &str = "http://xmlns.com/foaf/0.1/name";
const PPROPS: &str = "http://lv2plug.in/ns/ext/port-props#";
const ATOM_PORT: &str = "http://lv2plug.in/ns/ext/atom#AtomPort";
const EVENT_PORT: &str = "http://lv2plug.in/ns/ext/event#EventPort";

/// The host features ffmpeg's `lv2` filter gives a plugin.
const FFMPEG_FEATURES: &[&str] = &[
    "http://lv2plug.in/ns/ext/urid#map",
    "http://lv2plug.in/ns/ext/urid#unmap",
    "http://lv2plug.in/ns/ext/buf-size#boundedBlockLength",
    "http://lv2plug.in/ns/ext/buf-size#fixedBlockLength",
    "http://lv2plug.in/ns/ext/buf-size#powerOf2BlockLength",
    "http://lv2plug.in/ns/lv2core#isLive",
    "http://lv2plug.in/ns/lv2core#inPlaceBroken",
    "http://lv2plug.in/ns/lv2core#hardRTCapable",
];

fn lv2(l: &str) -> String {
    format!("{LV2}{l}")
}

/// `file:///a/b%20c/` → `/a/b c/`.
fn iri_to_path(iri: &str) -> Option<PathBuf> {
    let rest = iri.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let mut bytes = Vec::new();
    let b = rest.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                bytes.push(v);
                i += 3;
                continue;
            }
        }
        bytes.push(b[i]);
        i += 1;
    }
    let s = String::from_utf8_lossy(&bytes).into_owned();
    // `/C:/x` on Windows
    let s = if cfg!(windows) && s.len() > 2 && s.as_bytes()[2] == b':' { s[1..].to_string() } else { s };
    Some(PathBuf::from(s))
}

fn path_to_iri(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/").replace('%', "%25").replace(' ', "%20");
    if s.starts_with('/') {
        format!("file://{s}")
    } else {
        format!("file:///{s}")
    }
}

fn load(g: &mut Graph, file: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    ttl::parse(&text, &path_to_iri(file), g).map_err(|e| format!("{}: {e}", file.display()))
}

fn number(g: &Graph, s: &Term, p: &str) -> Option<f64> {
    g.object(s, p).and_then(Term::lit).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite())
}

fn literal(g: &Graph, s: &Term, p: &str) -> Option<String> {
    g.objects(s, p).find_map(Term::lit).map(str::to_string).filter(|v| !v.is_empty())
}

/// The plugins of the bundle at `dir`.
pub fn read_bundle(dir: &Path) -> Result<Vec<PluginInfo>, String> {
    let mut g = Graph::default();
    load(&mut g, &dir.join("manifest.ttl"))?;
    let plugin_class = Term::Iri(lv2("Plugin"));
    let uris: Vec<Term> = g.subjects(ttl::RDF_TYPE, &plugin_class).cloned().collect();
    let mut files: Vec<PathBuf> = Vec::new();
    for u in &uris {
        for f in g.objects(u, &format!("{RDFS}seeAlso")).filter_map(Term::iri).filter_map(iri_to_path) {
            if !files.contains(&f) {
                files.push(f);
            }
        }
    }
    let mut errors = Vec::new();
    for f in &files {
        if let Err(e) = load(&mut g, f) {
            errors.push(e);
        }
    }
    let out: Vec<PluginInfo> = uris.iter().filter_map(|u| plugin(&g, u, dir)).collect();
    if out.is_empty() && !errors.is_empty() {
        return Err(errors.join("; "));
    }
    Ok(out)
}

fn plugin(g: &Graph, uri: &Term, dir: &Path) -> Option<PluginInfo> {
    let id = uri.iri()?.to_string();
    let name = literal(g, uri, &format!("{DOAP}name")).unwrap_or_else(|| id.rsplit(['/', '#']).next().unwrap_or(&id).to_string());
    let maker = g.objects(uri, &format!("{DOAP}maintainer")).chain(g.objects(uri, &format!("{DOAP}developer"))).find_map(|m| literal(g, m, FOAF_NAME));
    let desc = literal(g, uri, &format!("{RDFS}comment")).or(maker).unwrap_or_default();

    struct Port {
        index: i64,
        control: Option<Control>,
    }
    let (mut inputs, mut outputs) = (0, 0);
    let mut problem = None;
    let mut ports = Vec::new();
    for p in g.objects(uri, &lv2("port")) {
        let input = g.is_a(p, &lv2("InputPort"));
        let props: Vec<&str> = g.objects(p, &lv2("portProperty")).filter_map(Term::iri).collect();
        let has = |prop: &str| props.contains(&prop);
        let optional = has(&lv2("connectionOptional"));
        let index = number(g, p, &lv2("index")).map(|v| v as i64).unwrap_or(i64::MAX);
        if g.is_a(p, &lv2("AudioPort")) {
            if input {
                inputs += 1;
            } else {
                outputs += 1;
            }
        } else if g.is_a(p, &lv2("CVPort")) && !optional {
            problem.get_or_insert_with(|| "has CV ports, which ffmpeg cannot connect".to_string());
        } else if (g.is_a(p, ATOM_PORT) || g.is_a(p, EVENT_PORT)) && input && !optional {
            problem.get_or_insert_with(|| "takes MIDI or events (an instrument?), which ffmpeg does not send".to_string());
        }
        let control = (input && g.is_a(p, &lv2("ControlPort"))).then(|| {
            let symbol = literal(g, p, &lv2("symbol")).unwrap_or_default();
            let int = has(&lv2("integer"));
            let toggled = has(&lv2("toggled"));
            let choices: Vec<(String, String)> = if has(&lv2("enumeration")) {
                let mut v: Vec<(f64, String)> = g
                    .objects(p, &lv2("scalePoint"))
                    .filter_map(|sp| Some((number(g, sp, RDF_VALUE)?, literal(g, sp, &format!("{RDFS}label")).unwrap_or_default())))
                    .collect();
                v.sort_by(|a, b| a.0.total_cmp(&b.0));
                v.into_iter().map(|(val, l)| (num(val, 6), l)).collect()
            } else {
                Vec::new()
            };
            let kind = if toggled {
                CtlKind::Toggle
            } else if !choices.is_empty() {
                CtlKind::Choice
            } else if int {
                CtlKind::Int
            } else {
                CtlKind::Float
            };
            Control {
                name: literal(g, p, &lv2("name")).unwrap_or_else(|| symbol.clone()),
                key: symbol,
                help: literal(g, p, &format!("{RDFS}comment")).unwrap_or_default(),
                kind,
                min: number(g, p, &lv2("minimum")),
                max: number(g, p, &lv2("maximum")),
                default: number(g, p, &lv2("default")).map(|v| num(v, if int || toggled { 0 } else { 6 })),
                strict: false,
                log: has(&format!("{PPROPS}logarithmic")),
                sample_rate: has(&lv2("sampleRate")),
                choices,
            }
        });
        ports.push(Port { index, control });
    }
    ports.sort_by_key(|p| p.index);
    let controls: Vec<Control> = ports.into_iter().filter_map(|p| p.control).filter(|c| super::is_symbol(&c.key)).collect();
    if inputs == 0 {
        problem.get_or_insert_with(|| "makes sound instead of changing it: it has no audio input".to_string());
    } else if outputs == 0 {
        problem.get_or_insert_with(|| "has no audio output".to_string());
    }
    if problem.is_none() {
        let missing = g.objects(uri, &lv2("requiredFeature")).filter_map(Term::iri).find(|f| !FFMPEG_FEATURES.contains(f));
        if let Some(f) = missing {
            problem = Some(format!("needs a host feature ffmpeg does not have ({})", f.rsplit(['/', '#']).next().unwrap_or(f)));
        }
    }
    Some(PluginInfo { kind: PluginKind::Lv2, id, label: String::new(), name, desc, file: dir.to_path_buf(), inputs, outputs, controls, problem })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bundle_on_disk() {
        let dir = std::env::temp_dir().join(format!("ffcut-lv2-{}/my%20amp.lv2", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("manifest.ttl"),
            "@prefix lv2: <http://lv2plug.in/ns/lv2core#> . @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             <urn:x:amp> a lv2:Plugin ; lv2:binary <amp.so> ; rdfs:seeAlso <amp%20b.ttl> .\n\
             <urn:x:synth> a lv2:Plugin ; rdfs:seeAlso <amp%20b.ttl> .",
        )
        .unwrap();
        std::fs::write(
            dir.join("amp b.ttl"),
            r#"@prefix lv2: <http://lv2plug.in/ns/lv2core#> . @prefix doap: <http://usefulinc.com/ns/doap#> .
               @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> . @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
               <urn:x:amp> doap:name "Amp" ; lv2:requiredFeature <http://lv2plug.in/ns/ext/urid#map> ;
                 lv2:port [ a lv2:AudioPort, lv2:OutputPort ; lv2:index 3 ; lv2:symbol "out" ] ,
                   [ a lv2:InputPort, lv2:ControlPort ; lv2:index 1 ; lv2:symbol "mode" ; lv2:name "Mode" ;
                     lv2:portProperty lv2:enumeration, lv2:integer ; lv2:default 1 ;
                     lv2:scalePoint [ rdfs:label "B" ; rdf:value 1 ] , [ rdfs:label "A" ; rdf:value 0 ] ] ,
                   [ a lv2:InputPort, lv2:ControlPort ; lv2:index 0 ; lv2:symbol "gain" ; lv2:name "Gain" ;
                     lv2:minimum -24 ; lv2:maximum +24 ] ,
                   [ a lv2:InputPort, lv2:AudioPort ; lv2:index 2 ; lv2:symbol "in" ] .
               <urn:x:synth> doap:name "Synth" ;
                 lv2:port [ a lv2:AudioPort, lv2:OutputPort ; lv2:index 0 ; lv2:symbol "out" ] ."#,
        )
        .unwrap();
        let mut list = read_bundle(&dir).unwrap();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        let (amp, synth) = (&list[0], &list[1]);
        assert_eq!((amp.name.as_str(), amp.inputs, amp.outputs, amp.problem.as_ref()), ("Amp", 1, 1, None));
        let keys: Vec<&str> = amp.controls.iter().map(|c| c.key.as_str()).collect();
        assert_eq!(keys, ["gain", "mode"]);
        assert_eq!((amp.controls[0].min, amp.controls[0].max, amp.controls[0].default.as_ref()), (Some(-24.0), Some(24.0), None));
        let m = &amp.controls[1];
        assert_eq!((m.kind, m.default.as_deref()), (CtlKind::Choice, Some("1")));
        assert_eq!(m.choices, [("0".to_string(), "A".to_string()), ("1".into(), "B".into())]);
        assert!(synth.problem.as_ref().unwrap().contains("no audio input"));
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    #[test]
    fn file_iris() {
        assert_eq!(iri_to_path("file:///a/b%20c.ttl"), Some(PathBuf::from("/a/b c.ttl")));
        assert_eq!(iri_to_path("http://x/y"), None);
        assert_eq!(path_to_iri(Path::new("/a/b")), "file:///a/b");
    }
}
