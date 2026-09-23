//! An icon from an icon set, set in the text beside it: the text's size and colour,
//! and hidden from assistive tech, which reads the text.

use icondata_core::IconData;
use idyll::{view, View};
use idyll_styles::styles;

pub fn icon(icon: &IconData) -> View {
    view! {
        svg css=[styles::ICON] aria_hidden=("true") viewBox[icon.view_box] style[icon.style]
            fill[icon.fill] stroke[icon.stroke] stroke_width[icon.stroke_width]
            stroke_linecap[icon.stroke_linecap] stroke_linejoin[icon.stroke_linejoin]
        {
            @dangerouslyUnescapedHtml(icon.data)
        }
    }
}

#[styles]
pub mod styles {
    use idyll_styles::Style;

    pub const ICON: Style = css! {{
        width: "1em",
        height: "1em",
        flex_shrink: 0,
    }};
}
