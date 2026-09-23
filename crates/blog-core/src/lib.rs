#[cfg(feature = "markdown")]
use std::{collections::BTreeMap, fs, path::Path};

#[cfg(feature = "markdown")]
use gray_matter::{engine::YAML, Matter};

#[cfg(feature = "markdown")]
pub struct Doc {
    pub title: String,
    pub markdown: String,
}

// ── Book structure ──────────────────────────────────────────────────────────
// Which chapters there are, and in what order. Each path resolves to `docs/<path>.md`
// and `/<path>`; a chapter is *named* by its own front matter, so no title is repeated
// here and the contents cannot disagree with the page.

pub const BOOK: &[&str] = &[
    "intro",
    "modelling_a_server",
    "rejecting_requests",
    "load_balancing",
    "rate_limiting_and_auto_scaling",
    "other_workloads",
];

// ── Routes ──────────────────────────────────────────────────────────────────
// The blog's route identity, as a typed value. A `BlogRoute` you can hold is a page that
// exists; its URL is a projection (`route.url()`), and `parse` recovers it from a request
// path — both from the one `#[route]` spec, so links and parsing cannot drift. A chapter
// is one segment, so the route space is exactly the flat set of files under `docs/` —
// nothing nested is addressable.

/// The blog's routes. Available everywhere (no content dependency) — the enumeration of
/// *which* routes exist is [`BOOK`], in book order.
#[derive(idyll_route::Route, Debug, Clone, PartialEq, Eq)]
pub enum BlogRoute {
    #[route("/")]
    Index,
    #[route("/404")]
    NotFound,
    #[route("/{slug}")]
    Doc { slug: String },
}

impl BlogRoute {
    /// A doc route from a book path (`"modelling_a_server"`).
    pub fn doc(path: &str) -> BlogRoute {
        BlogRoute::Doc {
            slug: path.to_owned(),
        }
    }

    /// Every book chapter as a `Doc` route, in book order.
    pub fn chapters() -> impl Iterator<Item = BlogRoute> {
        BOOK.iter().map(|path| BlogRoute::doc(path))
    }
}

// ── Content index ─────────────────────────────────────────────────────────────
// `docs/` is scanned once at startup (and again on each reload) into a lookup map. A
// request's path is then a **key**, never a filesystem path: an unknown key is a plain map
// miss (→ 404), and nothing user-supplied is ever joined onto a path — so traversal is
// impossible by construction, and a request does no disk IO.

#[cfg(feature = "markdown")]
pub struct Content {
    docs: BTreeMap<String, Doc>,
}

#[cfg(feature = "markdown")]
impl Content {
    /// Scan `docs/*.md` into memory. Call at startup and on each reload; hold the result
    /// behind an `Arc` and serve requests from it.
    pub fn load(docs_dir: &Path) -> anyhow::Result<Self> {
        Ok(Self {
            docs: scan_flat(docs_dir)?,
        })
    }

    pub fn doc(&self, path: &str) -> Option<&Doc> {
        self.docs.get(path)
    }
}

/// Scan a flat directory of `<slug>.md` files into a `slug → Doc` map.
#[cfg(feature = "markdown")]
fn scan_flat(dir: &Path) -> anyhow::Result<BTreeMap<String, Doc>> {
    let mut map = BTreeMap::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) == Some("md") {
            let slug = path.file_stem().unwrap().to_string_lossy().into_owned();
            map.insert(slug.clone(), read_doc(slug, &path)?);
        }
    }
    Ok(map)
}

/// What a chapter's `---` fenced head declares. The title is *declared*, so a chapter is
/// named the same whether or not its prose opens with a heading.
#[cfg(feature = "markdown")]
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FrontMatter {
    title: String,
}

#[cfg(feature = "markdown")]
fn read_doc(slug: String, path: &Path) -> anyhow::Result<Doc> {
    use anyhow::Context as _;

    let source = fs::read_to_string(path)?;
    let parsed = Matter::<YAML>::new()
        .parse::<FrontMatter>(&source)
        .with_context(|| format!("docs/{slug}.md: front matter"))?;
    let front = parsed
        .data
        .with_context(|| format!("docs/{slug}.md: no front matter"))?;
    Ok(Doc {
        title: front.title,
        markdown: parsed.content,
    })
}

#[cfg(all(test, feature = "markdown"))]
mod tests {
    use super::*;

    #[test]
    fn missing_content_directory_is_an_error() {
        let missing =
            std::env::temp_dir().join(format!("missing-blog-content-{}", std::process::id()));
        assert!(!missing.exists());
        assert!(Content::load(&missing).is_err());
    }

    #[test]
    fn content_indexes_files_and_lookups_cannot_traverse() {
        let root = std::env::temp_dir().join(format!("blogcore-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let docs = root.join("docs");
        fs::create_dir_all(docs.join("notes")).unwrap();
        fs::write(docs.join("intro.md"), "---\ntitle: Intro\n---\ndoc").unwrap();
        fs::write(
            docs.join("notes/private.md"),
            "---\ntitle: Private\n---\nnope",
        )
        .unwrap();

        let content = Content::load(&docs).unwrap();

        // Requests are pure map lookups.
        assert_eq!(content.doc("intro").unwrap().title, "Intro");

        // A file in a subdirectory is not a chapter: the scan is one level, so nothing
        // nested is ever indexed — and `/{slug}` cannot name it either.
        assert!(content.doc("notes/private").is_none());
        assert!(content.doc("private").is_none());

        // A traversal-shaped key is just a miss — no path is ever built from it, so there
        // is nothing to escape (this is why the index design is safe by construction).
        assert!(content.doc("../secret").is_none());

        let _ = fs::remove_dir_all(&root);
    }
}

pub mod sim_key;
pub use sim_key::{GateSignal, PolicyStage, PolicyTab, ServerBehavior, SimKey, TryControl};

pub mod code_key;
pub use code_key::CodeKey;

#[cfg(feature = "markdown")]
pub mod code_tabs;
#[cfg(feature = "markdown")]
pub use code_tabs::{code_groups, CodeGroup, CodeTab};

#[cfg(feature = "markdown")]
pub mod markdown_view;
#[cfg(feature = "markdown")]
pub use markdown_view::{ContentMapping, CODE_TABS_LIVE};
