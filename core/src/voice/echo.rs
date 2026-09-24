//! KUE hearing its own voice — L2 of `docs/KUE_ECHO_AND_SELF_WAKE.md`.
//!
//! KUE speaks through the speakers and listens through the microphone, with no
//! echo cancellation. MEASURED with the wake harness (KUE's own voice,
//! Samantha, no room): "Computer Science notes, PDF document." and "Computer
//! activity: I can see which app is in front." both WAKE the listener, and the
//! rest of the sentence becomes a request. KUE can say sentences like these:
//! file and app names come first in some narration, and a model's answer can
//! begin with anything. (Fixed registry text never begins a sentence with the
//! name — tested below.)
//!
//! The rule here compares WORDS, not audio. KUE knows what it just said. A
//! wake whose words — the invocation and what followed — are, in order, most
//! of a sentence KUE was saying (or finished saying a moment ago) is KUE, and
//! does nothing. The owner interrupting KUE says something else ("computer,
//! stop"), which is not KUE's sentence, so interrupting still works.
//!
//! Not a timer that ignores the microphone while KUE talks: a wake during
//! speech that is NOT KUE's sentence still wakes.
//!
//! Costs, stated:
//! - The words of what KUE said are held in memory for `ECHO_WINDOW_SECONDS`
//!   after it finishes — they used to be dropped the moment it finished. Never
//!   written anywhere, never logged, never sent to a model.
//! - The owner saying only the name while KUE is saying a sentence that
//!   contains it is taken for KUE. Saying the name with a request is not.
//! - Only KUE's own speech is recognised this way. A podcast saying "Computer,
//!   …" is not KUE and still wakes it (as it always has).

/// How long after KUE finishes a sentence its words still count as KUE.
/// MEASURED: a wake arrives ~1.9 s (1.74–2.21) after the audio that caused it
/// ends; room echo lasts a further fraction of a second. Six seconds covers
/// both with margin, and no more than that is kept.
pub const ECHO_WINDOW_SECONDS: f64 = 6.0;

/// At least this share of what was heard must appear, in order, in KUE's
/// sentence. The recogniser does not return KUE's text exactly ("90%" comes
/// back as "90 percent", names are respelt).
pub const MATCH_SHARE: f64 = 0.75;

/// One sentence KUE sent to its voice: the words only.
#[derive(Debug, Clone, PartialEq)]
pub struct OwnSentence {
    words: Vec<String>,
    pub sent_at: f64,
    pub ended_at: Option<f64>,
}

impl OwnSentence {
    pub fn new(text: &str, sent_at: f64) -> Self {
        OwnSentence { words: words(text), sent_at, ended_at: None }
    }

    /// Still being said, or finished within the window.
    pub fn is_recent(&self, now: f64) -> bool {
        self.ended_at.is_none_or(|e| now - e <= ECHO_WINDOW_SECONDS)
    }
}

pub fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// Whether a wake — `phrase` then `rest`, as the wake boundary reported it —
/// is KUE hearing a sentence it is saying or has just said.
pub fn is_own_speech(phrase: &str, rest: &str, own: &[OwnSentence], now: f64) -> bool {
    let heard: Vec<String> = words(phrase).into_iter().chain(words(rest)).collect();
    if heard.is_empty() { return false; }
    own.iter()
        .filter(|s| s.sent_at <= now && s.is_recent(now))
        .any(|s| {
            // Greedy in-order match: each heard word is looked for after the
            // previous match.
            let mut from = 0;
            let mut found = 0usize;
            for h in &heard {
                if let Some(i) = s.words[from..].iter().position(|w| w == h) {
                    found += 1;
                    from += i + 1;
                }
            }
            found as f64 / heard.len() as f64 >= MATCH_SHARE
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(text: &str, sent: f64, ended: Option<f64>) -> OwnSentence {
        OwnSentence { ended_at: ended, ..OwnSentence::new(text, sent) }
    }

    #[test]
    fn kue_saying_its_own_name_first_is_recognised_as_kue() {
        // The rests are what the wake harness actually returned for these
        // sentences spoken in KUE's voice.
        let own = [said("Computer Science notes, PDF document.", 10.0, Some(12.0))];
        assert!(is_own_speech("computer", "science notes pdf document", &own, 14.0));
        let own = [said("Computer activity: I can see which app is in front.", 10.0, None)];
        assert!(is_own_speech("computer", "activity i can see which app is in front", &own, 13.0));
    }

    #[test]
    fn the_owner_interrupting_kue_is_not_kue() {
        let own = [said("Computer Science notes, PDF document. It was changed yesterday.", 10.0, None)];
        assert!(!is_own_speech("computer", "stop", &own, 11.0));
        assert!(!is_own_speech("computer", "check my storage", &own, 11.0));
        assert!(!is_own_speech("computer", "what time is it", &own, 11.0));
    }

    #[test]
    fn close_enough_is_enough_because_the_recogniser_respells() {
        let own = [said("Computer, your storage is 90% full.", 10.0, Some(11.0))];
        // "90%" heard as "90 percent": 6 of 7 words.
        assert!(is_own_speech("computer", "your storage is 90 percent full", &own, 12.0));
    }

    #[test]
    fn it_lasts_the_window_and_no_longer() {
        let own = [said("Computer activity is on.", 10.0, Some(11.0))];
        assert!(is_own_speech("computer", "activity is on", &own, 11.0 + ECHO_WINDOW_SECONDS));
        assert!(!is_own_speech("computer", "activity is on", &own, 11.1 + ECHO_WINDOW_SECONDS),
            "after the window, the same words are someone else");
        // Not before KUE said it.
        assert!(!is_own_speech("computer", "activity is on", &own, 9.0));
    }

    #[test]
    fn the_stated_cost_the_bare_name_during_a_sentence_that_contains_it() {
        let own = [said("Computer automation isn't built.", 10.0, None)];
        assert!(is_own_speech("computer", "", &own, 10.5), "documented: taken for KUE");
        assert!(!is_own_speech("computer", "", &[], 10.5), "with KUE silent, the name alone wakes");
    }

    /// L1, for the text KUE speaks from its own registry: the listener wakes
    /// only on a sentence that BEGINS with the invocation, so no such sentence
    /// may. Covers the default invocation and fixed text only — file names, app
    /// names and a model's answer are not fixed, and are left to the check
    /// above.
    #[test]
    fn nothing_kue_says_from_its_capability_list_begins_a_sentence_with_its_name() {
        let starts_with_name = |text: &str| {
            text.split(['.', '!', '?', ':', ';', '—'])
                .any(|sentence| words(sentence).first().is_some_and(|w| w.starts_with("computer")))
        };
        for s in crate::capabilities::REGISTRY {
            assert!(!starts_with_name(s.voice_description), "{} speaks a sentence that begins with the name", s.id);
        }
        let answer = crate::conversation::capability_answer(&crate::capabilities::rows());
        assert!(answer.contains("computer automation"), "the check reads the real answer");
        assert!(!starts_with_name(&answer), "the capability answer begins a sentence with the name: {answer}");
        // The check can fail.
        assert!(starts_with_name("There is one item. Computer Science notes, PDF document."));
    }

    #[test]
    fn nothing_heard_is_not_kue() {
        assert!(!is_own_speech("", "", &[said("anything", 0.0, None)], 1.0));
    }
}
