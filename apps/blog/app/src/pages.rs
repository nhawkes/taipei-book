//! The blog's pages — the root component and its head. Every view lives HERE, in the
//! app crate; the server resolves data. Which page renders is the seed's `route`
//! sum — the server parsed the path exactly once, the `match` binds each variant's
//! own payload, and it is a **mount-time value**: transitions (and the dev content
//! watcher's reload) re-mount the page, so nothing here tracks.

use idyll::{view, Ctx, Never, Setup, View};
use idyll_data::Store;
use idyll_route::Route as _;

use crate::atoms::icon::icon;
use crate::atoms::typography::{heading, standfirst, HeadingLevel};
use crate::{styles, ChapterFrag, PageFrag, PageFragRoute, PageSeed};

/// The route view — the root component the server mounts per request and the browser
/// re-mounts per transition. Pure content: a value match, zero live bindings.
pub async fn page(ctx: Ctx<Setup, Never>, seed: PageSeed) -> idyll::Result {
    let store = Store::of(&ctx, &seed)?;
    let live = PageFrag::read(&store.cache, store.page).await;
    let page = live.at_mount(&ctx);
    let title = page.title;
    let content = match page.route {
        PageFragRoute::Article {
            body,
            previous,
            next,
        } => article(&title, body, previous, next),
        PageFragRoute::Index { chapters } => index(chapters),
        PageFragRoute::NotFound {} => not_found(),
    };
    Ok(ctx
        .render_content(view! {
            div css=[styles::PAGE] {
                @content(shell())
                main {
                    @content(content)
                }
            }
        })
        .await?)
}

/// The document-head content. The `<title>` is a contract field on the page node —
/// the host writes it into the envelope, not the view.
pub async fn head(ctx: Ctx<Setup, Never>, _seed: PageSeed) -> idyll::Result {
    let description = "Taipei is a library that integrates with tower to enable writing servers that behave well under stress without tuning.";
    Ok(ctx
        .render_content(view! {
            meta charset=("utf-8")
            meta name=("viewport") content=("width=device-width, initial-scale=1")
            meta name=("description") content=(description)
            meta property=("og:description") content=(description)
            meta property=("og:image") content=("https://taipei-book.pages.dev/static/og.png")
            meta name=("twitter:card") content=("summary_large_image")
            link rel=("icon") href=("/static/favicon.svg")
            // The faces the styles name. Without these the whole type scale falls back and every
            // size, weight and measure chosen for them is applied to something else. Served locally
            // from `blog-fonts` (`display:swap` in each `@font-face`), not a font CDN.
            link rel=("stylesheet") href=("/static/fonts.css")
        })
        .await?)
}

/// The shared page chrome, beside the styles it wears.
fn shell() -> View {
    view! {
        header css=[styles::HEADER] {
            a css=[styles::NAV_LINK] href=("/") { "taipei" }
        }
    }
}

/// An article's body arrives as **data**: the content mapping ran with the route
/// query (markdown to View IR, sim fences resolved against this crate's live
/// table), so the read yields the view and this component only wraps it. The title
/// is the chapter's declared one, so the page is headed the same way the index names it.
fn article(
    title: &str,
    body: View,
    previous: Option<ChapterFrag>,
    next: Option<ChapterFrag>,
) -> View {
    let previous = match previous {
        Some(chapter) => chapter_link(chapter, Toward::Previous),
        None => view! {},
    };
    let next = match next {
        Some(chapter) => chapter_link(chapter, Toward::Next),
        None => view! {},
    };
    view! {
        article css=[styles::CONTENT] {
            @content(heading(HeadingLevel::Title, title))
            @content(body)
        }
        nav css=[styles::CHAPTER_NAV] {
            @content(previous)
            @content(next)
        }
    }
}

enum Toward {
    Previous,
    Next,
}

fn chapter_link(chapter: ChapterFrag, toward: Toward) -> View {
    let href = chapter_href(&chapter);
    let title = chapter.title;
    match toward {
        Toward::Previous => view! {
            a css=[styles::NAV_LINK, styles::CHAPTER_LINK] href=(href) {
                @content(icon(icondata_lu::LuArrowLeft))
                (title)
            }
        },
        Toward::Next => view! {
            a css=[styles::NAV_LINK, styles::CHAPTER_LINK, styles::NEXT_CHAPTER] href=(href) {
                (title)
                @content(icon(icondata_lu::LuArrowRight))
            }
        },
    }
}

fn chapter_href(chapter: &ChapterFrag) -> String {
    blog_core::BlogRoute::doc(&chapter.slug).url().into_string()
}

fn not_found() -> View {
    heading(HeadingLevel::Title, "This page does not exist")
}

/// The front page's body: the book's table of contents. The chapters arrive as data in
/// book order, each naming itself with the title its own page is headed by, so the
/// contents and the chapter cannot disagree. The URL is still a projection of the route.
fn index(chapters: Vec<ChapterFrag>) -> View {
    #[derive(Clone)]
    struct TocEntry {
        href: String,
        title: String,
        num: String,
    }
    let book: Vec<TocEntry> = chapters
        .into_iter()
        .enumerate()
        .map(|(i, chapter)| TocEntry {
            href: chapter_href(&chapter),
            title: chapter.title,
            num: (i + 1).to_string(),
        })
        .collect();

    view! {
        @content(heading(HeadingLevel::Title, "taipei"))
        @content(standfirst("A queue engine, and the framework built to write about it."))
        ul css=[styles::TOC_LIST] {
            @for chapter in (book) {
                li css=[styles::TOC_ITEM] {
                    span css=[styles::TOC_NUM] { (chapter.num.clone()) }
                    a css=[styles::LINK] href=(chapter.href.clone()) { (chapter.title.clone()) }
                }
            }
        }
    }
}
