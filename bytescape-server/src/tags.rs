//! Shared tag matching — the single rule for hashtag subscriptions on every
//! surface (main feed + events). A subscription to `#tag` matches content whose
//! text carries the tag as a hashtag token (`#tag`) or as a bare word, always
//! boundary-checked. A tag NEVER expands to its topic family: subscribing to
//! `#astrophotography` must not surface `#astronomy` content, and `#atproto`
//! must not surface `#activitypub`.
//!
//! Token boundaries mirror the `appview_post_tags` extractor (migration 064:
//! `#([A-Za-z0-9_]+)`), so `#cat` never matches `#cats` or `#cat_x` — the same
//! guarantee `match_interest_chips` asserts with its `#tech`/`#techno` test.

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// True when `text` (any case) carries `tag` as a real hashtag token: `#tag`
/// followed by a non-token character or end of text.
pub fn text_has_hashtag(text: &str, tag: &str) -> bool {
    let tag = tag.trim().trim_start_matches('#').to_lowercase();
    if tag.is_empty() {
        return false;
    }
    let text = text.to_lowercase();
    let needle = format!("#{}", tag);
    text.match_indices(&needle).any(|(i, _)| {
        text[i + needle.len()..]
            .chars()
            .next()
            .map_or(true, |c| !is_word_char(c))
    })
}

/// True when `text` (any case) carries `tag` as a whole word: the bare token
/// with non-token characters (or text edges) on both sides.
pub fn text_has_word(text: &str, tag: &str) -> bool {
    let tag = tag.trim().trim_start_matches('#').to_lowercase();
    if tag.is_empty() {
        return false;
    }
    let text = text.to_lowercase();
    let mut start = 0usize;
    while let Some(pos) = text[start..].find(&tag) {
        let at = start + pos;
        let before_ok = text[..at].chars().next_back().map_or(true, |c| !is_word_char(c));
        let after_ok = text[at + tag.len()..]
            .chars()
            .next()
            .map_or(true, |c| !is_word_char(c));
        if before_ok && after_ok {
            return true;
        }
        start = at + tag.len();
    }
    false
}

/// The subscription rule: `#tag` or the bare word `tag`, boundary-checked.
/// Feed *membership* may use this inferred matcher — a topic feed can carry
/// posts without the literal hashtag ("they may fit in the feed for that
/// topic, which is fine"). Chips may not.
pub fn text_matches_tag(text: &str, tag: &str) -> bool {
    text_has_hashtag(text, tag) || text_has_word(text, tag)
}

/// How a text relates to a tag, for chip display.
pub enum ChipClaim {
    /// The literal hashtag `#tag` is present (entire tag) — may render as `#tag`.
    Hashtag,
    /// Only the bare word `tag` is present.
    Word,
}

/// The chip truth rule — shared by the main feed and the events feed so the
/// two cannot drift. A chip naming a tag is earned ONLY by an actual string
/// literal match of the entire hashtag in the text: the `#climate` chip needs
/// `#climate` in the post — not the word "climate", not `#climatechange`, and
/// never a topic-family sibling. Word/inferred matches (`text_matches_tag`)
/// may still place content in a feed ("they may fit in the feed for that
/// topic, which is fine") but earn NO tag chip: chips never come from "some
/// inferred matching system like feeds".
pub fn chip_claim(text: &str, tag: &str) -> Option<ChipClaim> {
    if text_has_hashtag(text, tag) {
        Some(ChipClaim::Hashtag)
    } else if text_has_word(text, tag) {
        Some(ChipClaim::Word)
    } else {
        None
    }
}

/// Postgres `~*` pattern for a **literal hashtag token only** — the events
/// surface's membership rule. It never accepts the bare word: events carry
/// long-form marketing prose, where "a weekly practice space" must not claim
/// `#space` (74 event rows did, none of them carrying the hashtag). This also
/// matches the main feed's indexed rule, which only looks up literal `#tag`
/// rows in `appview_post_tags`.
pub fn sql_hashtag_pattern(tag: &str) -> String {
    let tag = tag.trim().trim_start_matches('#').to_lowercase();
    format!("(^|[^[:alnum:]_])#{}([^[:alnum:]_]|$)", regex_escape(&tag))
}

pub(crate) fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if r"\.+*?()|[]{}^$".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `#hashtag` match only for a real hashtag token: `#tech` must NOT
    /// match `#techno` or `#tech_x`.
    #[test]
    fn hashtag_is_token_exact() {
        assert!(text_has_hashtag("I love #tech today", "tech"));
        assert!(!text_has_hashtag("dancing to #techno all night", "tech"));
        assert!(!text_has_hashtag("see #tech_x posts", "tech"));
    }

    /// A bare word counts (boundary-checked), and is distinct from the
    /// hashtag form.
    #[test]
    fn word_is_token_exact() {
        assert!(text_has_word("CLIMATE report for the region", "climate"));
        assert!(!text_has_word("the climateaction march", "climate"));
        assert!(!text_matches_tag("Senate approves fiscal package", "activitypub"));
    }

    /// A tag never expands to its topic family: `#astrophotography` must not
    /// match `#astronomy` content and vice versa (the bleed bug).
    #[test]
    fn tag_never_expands_to_family() {
        assert!(!text_matches_tag("Join our #astronomy night", "astrophotography"));
        assert!(!text_matches_tag("#astrophotography workshop tonight", "astronomy"));
        assert!(!text_matches_tag("The #activitypub meetup", "atproto"));
        assert!(!text_matches_tag("#atproto builders call", "activitypub"));
    }

    /// Subscribing `#astrophotography` still finds its own tag and word.
    #[test]
    fn exact_tag_still_matches() {
        assert!(text_matches_tag("Best #astrophotography shots", "astrophotography"));
        assert!(text_matches_tag("an astrophotography workshop", "astrophotography"));
    }

    /// Chip truth: a `#tag` chip only for the literal hashtag — the entire
    /// hashtag, string-matched. A bare word is Word (no `#` chip); a longer
    /// hashtag (`#climatechange`) neither matches nor claims `#climate`.
    #[test]
    fn chip_claim_is_literal() {
        assert!(matches!(chip_claim("I love #climate talk", "climate"), Some(ChipClaim::Hashtag)));
        assert!(matches!(chip_claim("CLIMATE report for the region", "climate"), Some(ChipClaim::Word)));
        assert!(chip_claim("a #climatechange march", "climate").is_none());
        assert!(chip_claim("dancing to #techno", "tech").is_none());
        assert!(chip_claim("Senate approves fiscal package", "activitypub").is_none());
    }

    /// The events surface's SQL membership rule requires a literal hashtag:
    /// the pattern must not accept the bare word ("a weekly practice space"),
    /// and must still match `#space` / `#Space` as a token, never `#spaceship`.
    #[test]
    fn event_sql_pattern_requires_literal_hashtag() {
        let re = regex::Regex::new(&format!("(?i){}", sql_hashtag_pattern("space"))).unwrap();
        assert!(re.is_match("join us for #space tonight"));
        assert!(re.is_match("#Space"));
        assert!(re.is_match("ends with #space"));
        assert!(!re.is_match("a weekly practice space for everyone"));
        assert!(!re.is_match("finding space for practice"));
        assert!(!re.is_match("#spaceship launch"));
        assert!(!re.is_match("#space_x"));
    }
}
