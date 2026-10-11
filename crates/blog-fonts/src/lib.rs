//! The blog's web faces, bundled.
//!
//! The type scale names three families — IBM Plex Sans (body), IBM Plex Mono (numbers and
//! code), Newsreader (display serif). They used to arrive from the Google Fonts CDN, a runtime
//! fetch to a third party on every first paint. Here the woff2 ride in the binary (`include_bytes!`),
//! so a fresh clone serves its own type with cargo and nothing else — the same self-contained law
//! that brings Binaryen in as a crate rather than a machine install.
//!
//! The faces are the OFL-1.1 latin subsets from the `@fontsource` packages (see `fonts/OFL-*.txt`);
//! only the weights the scale actually asks for are carried. [`FACES`] is the single source: the
//! bytes it embeds and the `@font-face` rules [`stylesheet`] writes are generated from the same list,
//! so a face is declared once.

use std::fmt::Write as _;

/// One bundled face: the family it answers to, its weight, the basename it serves under, and the
/// embedded woff2. `style` is always `normal` — the scale never asks for an italic face file
/// (it synthesises emphasis), so it is not a field.
pub struct Face {
    pub family: &'static str,
    pub weight: u16,
    pub file: &'static str,
    pub bytes: &'static [u8],
}

macro_rules! face {
    ($family:literal, $weight:literal, $file:literal) => {
        Face {
            family: $family,
            weight: $weight,
            file: concat!($file, ".woff2"),
            bytes: include_bytes!(concat!("../fonts/", $file, ".woff2")),
        }
    };
}

/// Every bundled face, in the order the scale introduces them. The single source of truth for
/// both the served bytes and the generated stylesheet.
pub const FACES: &[Face] = &[
    face!("IBM Plex Sans", 400, "ibm-plex-sans-400"),
    face!("IBM Plex Sans", 500, "ibm-plex-sans-500"),
    face!("IBM Plex Sans", 600, "ibm-plex-sans-600"),
    face!("IBM Plex Mono", 400, "ibm-plex-mono-400"),
    face!("IBM Plex Mono", 500, "ibm-plex-mono-500"),
    face!("IBM Plex Mono", 600, "ibm-plex-mono-600"),
    face!("Newsreader", 400, "newsreader-400"),
    face!("Newsreader", 500, "newsreader-500"),
];

/// The licences the faces ship under, served beside them.
pub const LICENCES: &[(&str, &[u8])] = &[
    (
        "OFL-IBM-Plex.txt",
        include_bytes!("../fonts/OFL-IBM-Plex.txt"),
    ),
    (
        "OFL-Newsreader.txt",
        include_bytes!("../fonts/OFL-Newsreader.txt"),
    ),
];

/// A file the type ships, found by the name it serves under.
pub struct Shipped {
    pub media_type: &'static str,
    pub bytes: &'static [u8],
}

/// Every name [`file`] answers to: each face's woff2, then each licence.
pub fn names() -> impl Iterator<Item = &'static str> {
    FACES
        .iter()
        .map(|face| face.file)
        .chain(LICENCES.iter().map(|(name, _)| *name))
}

/// The face or licence served under `name`.
pub fn file(name: &str) -> Option<Shipped> {
    let face = FACES
        .iter()
        .find(|face| face.file == name)
        .map(|face| Shipped {
            media_type: "font/woff2",
            bytes: face.bytes,
        });
    face.or_else(|| {
        LICENCES
            .iter()
            .find(|(licence, _)| *licence == name)
            .map(|(_, bytes)| Shipped {
                media_type: "text/plain; charset=utf-8",
                bytes,
            })
    })
}

/// The `@font-face` rules for every face, each pointing at the URL `url` gives its file name.
pub fn stylesheet(url: impl Fn(&str) -> String) -> String {
    let mut css = String::new();
    for face in FACES {
        let _ = writeln!(
            css,
            "@font-face{{font-family:'{}';font-style:normal;font-weight:{};\
             font-display:swap;src:url('{}') format('woff2')}}",
            face.family,
            face.weight,
            url(face.file),
        );
    }
    css
}
