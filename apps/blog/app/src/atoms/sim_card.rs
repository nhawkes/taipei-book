//! The card shell the multi-server sims share: the surface they sit on and the row of running
//! tallies across its top.

use idyll::{live_view, Ctx, Never, Setup, Signal};
use idyll_styles::styles;

/// The sim's running aggregates (label + bold value), on the card's trailing edge. A
/// presentational (`Never`) leaf — it drives as a field of its sim, mints no status, and takes
/// its tallies already formatted so the data layer stays out of the view. A tally is a signal
/// because a batch settling in the browser is what writes it: the number climbs with the
/// picture that earns it.
#[idyll::component]
pub async fn Tallies(
    ctx: Ctx<Setup, Never>,
    aggs: Vec<(&'static str, Signal<String>)>,
) -> idyll::Result {
    Ok(ctx
        .render(live_view! {
            div css=[styles::TALLIES] {
                @for (label, value) in (aggs) {
                    span css=[styles::AGG] { (label) " " span css=[styles::AGGB] { $value } }
                }
            }
        })
        .await?)
}

#[styles]
pub mod styles {
    use crate::atoms::tokens::Radius;
    use crate::styles::Palette;
    use idyll_styles::Style;

    /// The card's control strip: what the reader can do to the batch, and — pushed to the
    /// trailing edge by [`COUNT`] — what the batch comes to.
    pub const CTRLS: Style = css! {{
        display: "flex",
        gap: "10px",
        align_items: "center",
        margin: "0 2px 12px",
        flex_wrap: "wrap",
    }};

    /// The batch's size, at the end of the control strip. Tabular so a count that changes
    /// with the scale does not shuffle the controls beside it.
    pub const COUNT: Style = css! {{
        margin_left: "auto",
        font_size: "11px",
        color: Palette::ink_faint,
        font_variant_numeric: "tabular-nums",
    }};

    /// The surface a sim sits on — the same card the single-server stage uses.
    pub const CARD: Style = css! {{
        position: "relative",
        border_radius: Radius::card,
        background: Palette::ground,
        border: "1px solid transparent",
        border_color: Palette::line,
        box_shadow: "0 6px 22px rgba(60,70,40,0.07)",
        padding: "14px",
    }};
    pub const TALLIES: Style = css! {{
        display: "flex",
        align_items: "baseline",
        justify_content: "flex-end",
        gap: "14px",
        flex_wrap: "wrap",
        margin: "2px 4px 12px",
    }};
    pub const AGG: Style = css! {{
        font_size: "12.5px",
        color: Palette::ink_muted,
        font_variant_numeric: "tabular-nums",
    }};
    pub const AGGB: Style = css! {{
        display: "inline-block",
        min_width: "5ch",
        text_align: "right",
        color: Palette::control_ink,
        font_weight: 600,
    }};
}
