//! The taipei blog/book — **data and infra**: the one `route(request)` resolver
//! parses the path natively, reads the content index, and returns a pure-data
//! [`Page`] node whose `route` sum names the page it resolved. The UI — the page
//! component, its head, the sims — lives in `blog-app`, mounted through the
//! membrane per request and claimed by the browser.
//!
//! The server scans `docs/` once into [`blog_core::Content`] (requests are map lookups,
//! never file paths — traversal-proof by construction) and hot-swaps the index when the
//! prose changes.

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use idyll_data::{node, root, value, AppRoot, Mutations, Queries, Request, Root, RootHandle};
use idyll_serve::Server;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(clap::Subcommand, Default)]
enum Command {
    #[default]
    Dev,
    Build,
    Prod,
    Prerender {
        #[arg(long, default_value = "dist")]
        out: PathBuf,
        #[arg(long)]
        standalone: bool,
    },
}

// ── The page node (the route contract) ───────────────────────────────────────────────

/// One tab of a code group: what the bar shows, and the code it shows.
#[value]
pub struct CodeTab {
    pub label: String,
    pub lang: String,
    pub code: String,
}

/// A run of adjacent ` ```rust tab=… ` fences, as **data**. Page-scoped: `id` numbers the
/// group within its own page, so a group has no identity to be fetched by and rides inside
/// the page record. The marker names the id, and the island finds its group in the page.
#[value]
pub struct CodeGroup {
    pub id: String,
    pub tabs: Vec<CodeTab>,
}

/// One line of the book's table of contents: where the chapter is, and what it calls
/// itself. The name is read from the chapter's own front matter, never restated here.
#[value]
pub struct Chapter {
    pub slug: String,
    pub title: String,
}

/// The parsed route, **as a sum**: an article carries its markdown, the index the
/// contents — [`blog_core::BOOK`] fixes the order, and each chapter's title comes from
/// the same scan that serves it, so the contents cannot drift from the page. Each page
/// ships exactly its own data, and the component matches the closed set exhaustively.
#[value]
pub enum Route {
    Article {
        body: idyll_data::Content,
        previous: Option<Chapter>,
        next: Option<Chapter>,
    },
    Index {
        chapters: Vec<Chapter>,
    },
    NotFound,
}

/// The route root's output — the framework contract (`id` = the request path,
/// `title`) plus the parsed route and its payload.
#[node]
pub struct Page {
    pub id: String,
    pub title: String,
    pub route: Route,
    /// The article's code groups, in document order. The content mapping renders the
    /// prose around markers that point in here.
    pub code_tabs: Vec<CodeGroup>,
}

// ── The content index ────────────────────────────────────────────────────────────────

/// The content index, shared with the route resolver and **hot-swappable in dev**: the
/// content watcher rescans on change and replaces the index under the lock, so editing
/// a markdown file shows up on the next request — no restart.
#[derive(Clone)]
struct Db(Arc<std::sync::RwLock<blog_core::Content>>);

impl Db {
    fn content(&self) -> std::sync::RwLockReadGuard<'_, blog_core::Content> {
        self.0.read().unwrap()
    }
}

// ── The route root — path parsing happens HERE, once, in native Rust ─────────────────

/// An article page: the markdown is source data; the registered content mapping
/// turns it into View IR with the operation, so the wire carries the view.
fn article(
    path: &str,
    title: &str,
    markdown: &str,
    previous: Option<Chapter>,
    next: Option<Chapter>,
) -> Page {
    Page {
        id: path.to_string(),
        title: title.to_string(),
        route: Route::Article {
            body: markdown.into(),
            previous,
            next,
        },
        // The same walk the mapping does, so a marker's key always names a group that
        // is actually here (`blog_core::code_tabs`).
        code_tabs: blog_core::code_groups(markdown)
            .into_iter()
            .map(|group| CodeGroup {
                id: group.id,
                tabs: group
                    .tabs
                    .into_iter()
                    .map(|tab| CodeTab {
                        label: tab.label,
                        lang: tab.lang,
                        code: tab.code,
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// The table of contents in book order. A chapter missing from `docs/` is simply not
/// listed — `check_book` is what refuses to publish a book with a hole in it.
fn contents(content: &blog_core::Content) -> Vec<Chapter> {
    blog_core::BOOK
        .iter()
        .filter_map(|slug| {
            let doc = content.doc(slug)?;
            Some(Chapter {
                slug: slug.to_string(),
                title: doc.title.clone(),
            })
        })
        .collect()
}

/// The chapters either side of `slug` in book order.
fn neighbours(book: &[Chapter], slug: &str) -> (Option<Chapter>, Option<Chapter>) {
    let Some(at) = book.iter().position(|chapter| chapter.slug == slug) else {
        return (None, None);
    };
    (book[..at].last().cloned(), book[at + 1..].first().cloned())
}

/// `route(request) -> Option<Page>`: the whole routing story. **Absence is a value**
/// (`Ok(None)`) — the executor's typed 404. No pattern DSL: the path is parsed with
/// ordinary Rust against the content index (map lookups, traversal-proof).
#[root]
async fn route(db: &Db, request: Request) -> Result<Option<Page>, std::convert::Infallible> {
    use idyll_route::Route as _;
    let content = db.content();
    let path = request.path.as_str();

    // The path is parsed to a typed `BlogRoute` (the derived inverse of `route.url()`), then
    // resolved to its page. An unknown shape is `None` — the typed 404.
    let resolved = match blog_core::BlogRoute::parse(path) {
        Some(blog_core::BlogRoute::Index) => Some(Page {
            id: path.to_string(),
            title: "taipei".to_string(),
            route: Route::Index {
                chapters: contents(&content),
            },
            code_tabs: Vec::new(),
        }),
        Some(blog_core::BlogRoute::Doc { slug }) => content.doc(&slug).map(|doc| {
            let (previous, next) = neighbours(&contents(&content), &slug);
            article(path, &doc.title, &doc.markdown, previous, next)
        }),
        Some(blog_core::BlogRoute::NotFound) => Some(Page {
            id: path.to_string(),
            title: "This page does not exist".to_string(),
            route: Route::NotFound,
            code_tabs: Vec::new(),
        }),
        None => None,
    };
    Ok(resolved)
}

// ── Boot ─────────────────────────────────────────────────────────────────────────────

/// Watch `docs/`; on any change, rescan and swap the shared index.
/// Failures are logged, not fatal — a half-saved file shouldn't kill the server.
fn spawn_content_watcher(db: Db) {
    use notify::{RecursiveMode, Watcher};

    std::thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = match notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        }) {
            Ok(watcher) => watcher,
            Err(err) => {
                eprintln!("content watch disabled: {err}");
                return;
            }
        };
        if let Err(err) = watcher.watch(&PathBuf::from("docs"), RecursiveMode::Recursive) {
            eprintln!("content watch disabled: {err}");
        }
        while rx.recv().is_ok() {
            // Drain the burst an editor save produces, then rescan once.
            while rx.try_recv().is_ok() {}
            match blog_core::Content::load(&PathBuf::from("docs")) {
                Ok(content) => {
                    *db.0.write().unwrap() = content;
                    println!("content rescanned");
                }
                Err(err) => eprintln!("content rescan failed: {err}"),
            }
        }
    });
}

/// The blog's entry surface: one query, no mutations. The schema is the reachability
/// closure of the root object (`Request`, `Page`, `CodeGroup` all arrive uninvited);
/// the resolver table derives from the same value. Content types never leave the
/// server; content *data* crosses as page fields.
#[derive(Queries)]
struct Query {
    route: RootHandle<Db>,
}

#[derive(Mutations)]
struct Mutation;

fn root() -> AppRoot<Db> {
    // The content mapping executes with the data layer: markdown becomes View IR
    // (sim fences included -- resolved against the app's live table) before the
    // payload is cut, and the client reads views, never source. Its styles come from
    // the app, like every other view's.
    let mapping = content_mapping();
    let app_root: AppRoot<Db> = Root {
        query: Query { route: route() },
        mutation: Mutation,
    }
    .into();
    app_root.content(move |source| mapping.content(source))
}

fn content_mapping() -> blog_core::ContentMapping {
    blog_core::ContentMapping {
        sims: blog_app::SIM_NAMES
            .iter()
            .map(|name| name.to_string())
            .collect(),
        error: blog_app::atoms::content::SIM_ERROR,
    }
}

fn check_book(content: &blog_core::Content) -> anyhow::Result<()> {
    use anyhow::Context as _;

    let mapping = content_mapping();
    for slug in blog_core::BOOK {
        let doc = content
            .doc(slug)
            .with_context(|| format!("missing required chapter: docs/{slug}.md"))?;
        if let Some(message) = mapping.map(&doc.markdown).errors.first() {
            anyhow::bail!("docs/{slug}.md: {message}");
        }
    }
    Ok(())
}

/// The blog's route set for prerendering — the index and every book chapter (from `BOOK`).
/// Typed values; the prerenderer projects each to its URL with the same `#[route]` codec the
/// resolver parses with.
impl idyll_serve::Sitemap for Db {
    type Route = blog_core::BlogRoute;
    async fn routes(&self) -> Vec<blog_core::BlogRoute> {
        std::iter::once(blog_core::BlogRoute::Index)
            .chain(blog_core::BlogRoute::chapters())
            .collect()
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let command = Cli::parse().command.unwrap_or_default();
    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3002);

    // Scan up front; requests are served from the shared index, which the content
    // watcher hot-swaps on edits.
    let content = blog_core::Content::load(&PathBuf::from("docs"))?;
    if matches!(command, Command::Prerender { .. }) {
        check_book(&content)?;
    }
    let db = Db(Arc::new(std::sync::RwLock::new(content)));
    spawn_content_watcher(db.clone());

    // Lay the bundled web faces down beside the other static assets, so `/static/fonts.css` and
    // its woff2 serve locally — the type is in the binary, not fetched from a font CDN.
    blog_fonts::install(&PathBuf::from("static"))?;

    // `prerender` runs the same SSR early, to a static `dist/` any host serves; otherwise
    // serve live. Prerendering builds the optimized bundle (as prod does).
    // `--standalone` inlines every asset into each page (data: URLs), so a page is a
    // self-contained file that works from `file://` — no server, no fetches.
    let mode = match command {
        Command::Dev => idyll_serve::Mode::Dev,
        Command::Build => idyll_serve::Mode::Build,
        Command::Prod | Command::Prerender { .. } => idyll_serve::Mode::Prod,
    };

    let server = Server::builder()
        .app_crate("blog-app") // ALL the UI: the page component, its head, the sims
        .route_query(blog_app::RouteQuery::query_file())
        .root(root())
        .data(db)
        .manifest_dir(env!("CARGO_MANIFEST_DIR"))
        .mode(mode)
        .port(port)
        .assets("static") // favicon.svg and the sim bundles; the styles are typed
        .not_found(blog_core::BlogRoute::NotFound)
        .build();

    match command {
        Command::Prerender { out, standalone } => server.prerender(&out, standalone).await,
        _ => server.serve().await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A chapter file: the declared title every doc needs, then the body under test.
    fn chapter(body: &str) -> String {
        format!("---\ntitle: Test\n---\n{body}")
    }

    #[test]
    fn publication_requires_every_chapter_and_valid_simulations() {
        let root = tempfile::tempdir().unwrap();
        for slug in blog_core::BOOK {
            std::fs::write(root.path().join(format!("{slug}.md")), chapter("")).unwrap();
        }
        let load = || blog_core::Content::load(root.path()).unwrap();
        check_book(&load()).unwrap();
        let intro = root.path().join("intro.md");
        std::fs::remove_file(&intro).unwrap();
        assert!(check_book(&load())
            .unwrap_err()
            .to_string()
            .contains("intro.md"));
        for spec in [
            "{",
            r#"{"sim":"missing","width":960,"height":520}"#,
            r#"{"sim":"queue-viz","width":960,"height":520,"stage":"missing"}"#,
        ] {
            std::fs::write(&intro, chapter(&format!("```sim\n{spec}\n```\n"))).unwrap();
            assert!(check_book(&load()).is_err(), "{spec}");
        }
        std::fs::write(&intro, chapter(
            "```sim\n{\"sim\":\"queue-viz\",\"width\":960,\"height\":520,\"stage\":\"queue\"}\n```\n"
        )).unwrap();
        check_book(&load()).unwrap();
    }
}
