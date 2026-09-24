//! The conversation with Lantern, held in memory for this session only.
//!
//! Nothing here is written to local memory. It is cleared whenever the session
//! stops being the owner's — locked, a stranger, more than one person — and
//! whenever KUE is killed, so nobody who sits down later can scroll back.

use crate::context::{Capability, CapabilityStatus};
use crate::privacy::Turn;
use crate::voice::InputSource;
use serde::{Deserialize, Serialize};

/// Shown as the `model` of an answer Lantern wrote itself, without any model.
pub const CAPABILITY_LIST_SOURCE: &str = "KUE's capability list · no model";
pub const COMMAND_PARSER_SOURCE: &str = "KUE's command parser · no model";

/// A command verb with nothing to act on — "Can you open?" — answered by
/// Lantern with what it needs, rather than sent to the model. Measured on this
/// Mac: the model answered "Can you open?" with four paragraphs and an invented
/// keyboard shortcut.
pub fn incomplete_command(question: &str) -> Option<String> {
    let q: String = question.to_lowercase().chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' { c } else { ' ' }).collect();
    let mut words: Vec<&str> = q.split_whitespace().collect();
    const POLITE: [&str; 12] = ["can", "could", "would", "will", "you", "please", "lantern", "kue", "hey", "ok", "for", "me"];
    while words.first().is_some_and(|w| POLITE.contains(w)) { words.remove(0); }
    while words.last().is_some_and(|w| POLITE.contains(w) || *w == "it" || *w == "something" || *w == "now") { words.pop(); }
    let rest = words.join(" ");
    let example = match rest.as_str() {
        "open" | "launch" | "start" | "open up" => "“open Safari”, “open my resume” or “go to apple.com”",
        "quit" | "close" | "exit" => "“quit Notes”",
        "switch" | "switch to" | "focus" | "focus on" => "“switch to Mail”",
        "go to" | "visit" | "browse to" => "“go to apple.com”",
        "find" | "find my" | "open my" => "“open my resume”",
        "create" | "make" | "create a file" | "create a folder" | "make a folder" => "“create a folder called Projects”",
        "read" | "read the file" => "“read the file notes.txt”",
        "move" => "“move notes.txt to Projects”",
        "notify" | "notify me" | "remind" | "remind me" => "“notify me that the build finished”",
        _ => return None,
    };
    Some(format!("{} what? Say the whole command in one go, for example {example}.",
        rest.split_whitespace().next().map(|w| { let mut c = w.chars(); c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default() }).unwrap_or_default()))
}

/// A request KUE recognises as a command it cannot carry out as a whole ("open
/// Calculator and add 2 and 2"): answered by Lantern, without a model and without
/// starting any part of it. A model would answer "4" as if it had used Calculator.
pub fn unsupported_command(question: &str) -> Option<String> {
    match crate::task::plan(question)? {
        crate::task::Plan::Unsupported { reason } => Some(reason.to_string()),
        _ => None,
    }
}

/// Whether a question asks, in general, what Lantern can do.
///
/// Those are answered from the capability table, never by the model. Measured
/// on this Mac: asked "What can you do?" with "Internet research: NotImplemented"
/// in its context and "you cannot use the internet" in its instructions,
/// Apple's on-device model answered that it could "navigate the web" and
/// "search for information".
pub fn is_capability_question(question: &str) -> bool {
    let q: String = question.to_lowercase().chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '\'' { c } else { ' ' }).collect();
    let q = q.split_whitespace().collect::<Vec<_>>().join(" ");
    const ASKS: [&str; 18] = [
        "what can you do", "what can lantern do", "what are you able to do", "what are you capable of",
        "what are your capabilities", "what are lantern's capabilities", "what do you do",
        "what can't you do", "what can you not do", "what features do you have", "what are your features",
        "what are your limits", "what will you do", "how can you help", "how can you help me",
        "what can you help with", "what can you help me with", "what are you for",
    ];
    ASKS.iter().any(|a| q == *a || q.starts_with(&format!("{a} ")) || q.ends_with(&format!(" {a}")) || q.contains(&format!(" {a} ")))
}

/// An answer to "what can you do?", written from the capability table.
pub fn capability_answer(caps: &[Capability]) -> String {
    let names = |s: CapabilityStatus| -> Vec<String> {
        caps.iter().filter(|c| c.status == s).map(|c| c.name.to_lowercase()).collect()
    };
    let list = |v: Vec<String>| match v.len() {
        0 => String::new(),
        1 => v[0].clone(),
        n => format!("{} and {}", v[..n - 1].join(", "), v[n - 1]),
    };
    let real = names(CapabilityStatus::Real);
    let partial = names(CapabilityStatus::Partial);
    // Whether something is excluded on purpose is a fact about the capability,
    // read from the registry. It used to be read from the first word of the
    // note, so rewording a sentence silently moved a capability between "not
    // implemented" and "deliberately excluded".
    let deliberate = |c: &Capability| crate::capabilities::by_name(&c.name).is_some_and(|s| s.deliberate);
    let missing: Vec<String> = caps.iter()
        .filter(|c| c.status == CapabilityStatus::NotImplemented && !deliberate(c))
        .map(|c| c.name.to_lowercase()).collect();
    let excluded: Vec<String> = caps.iter()
        .filter(|c| c.status == CapabilityStatus::NotImplemented && deliberate(c))
        .map(|c| c.name.to_lowercase()).collect();
    let mut out = String::from("From KUE's own capability list, not a model: ");
    if !real.is_empty() { out.push_str(&format!("working now — {}. ", list(real))); }
    if !partial.is_empty() { out.push_str(&format!("Working with stated limits — {}. ", list(partial))); }
    if !missing.is_empty() { out.push_str(&format!("Not implemented — {}. ", list(missing))); }
    if !excluded.is_empty() { out.push_str(&format!("Deliberately excluded — {}. ", list(excluded))); }
    out.push_str("Each limit is written out in the capability panel.");
    out
}

/// Phrases that claim a capability. Matched per sentence, lower-cased.
const NEGATIONS: [&str; 12] = ["cannot", "can't", "can not", "unable", "not able", "don't", "do not",
    "never", "isn't", "is not", "doesn't", "not implemented"];

/// Whether a clause is about KUE at all. A capability phrase only amounts to a
/// claim when KUE is the one doing it: "I can search the web" is a claim,
/// "You can look at your screen to check" is advice to the owner, and "Your
/// habits are your own business" is neither.
///
/// A correction printed under a correct answer is as damaging as a missing one,
/// and it is the failure mode bare phrases drift into. "You can ask me to
/// search the web" is still a claim, and still passes this: it refers to KUE.
///
/// Takes the clause already normalised to single-spaced words with a space at
/// each end, so these match whole words.
/// Whether a clause denies what it says. Whole words only: "whenever" is not
/// "never", and matching it as one made "I can check your storage whenever you
/// ask" read as a denial — and, in `overclaims`, would have excused a claim.
///
/// Takes the clause already normalised to single-spaced words with a space at
/// each end.
fn negated(words: &str) -> bool {
    NEGATIONS.iter().any(|n| words.contains(&format!(" {n} ")))
}

fn about_kue(words: &str) -> bool {
    [" i ", " i'll ", " i'm ", " i've ", " i'd ", " my ", " me ", " lantern ", " kue "]
        .iter().any(|p| words.contains(p))
}

/// Claims in a model answer to a capability Lantern's table marks NOT
/// IMPLEMENTED. A clause that negates the claim ("I cannot browse") is not one.
/// Checked per clause, not per sentence: measured on this Mac, the model wrote
/// "I can help you search the internet for information, but I can't browse the
/// web" — a claim and a denial in one sentence. Returns names, each once.
///
/// The phrases come from `capabilities::REGISTRY`, where they live on the
/// capability they claim. They used to be a second table here, matched to
/// capabilities by name: renaming a capability would have silently stopped the
/// correction, and no test would have caught it, because the tests below build
/// their own rows.
pub fn overclaims(answer: &str, caps: &[Capability]) -> Vec<String> {
    let lower = answer.to_lowercase()
        .replace(", but ", ".").replace(" but ", ".").replace("however", ".")
        .replace(" although ", ".").replace(" though ", ".").replace(" except ", ".");
    let sentences: Vec<&str> = lower.split(|c| c == '.' || c == '!' || c == '?' || c == ';' || c == '\n').collect();
    let mut found: Vec<String> = Vec::new();
    for spec in crate::capabilities::REGISTRY.iter().filter(|s| !s.claim_phrases.is_empty()) {
        let (name, phrases) = (spec.name, spec.claim_phrases);
        let missing = caps.iter().any(|c| c.name == name && c.status == CapabilityStatus::NotImplemented);
        if !missing || found.iter().any(|f| f == name) { continue; }
        // Whole words only: measured, "the Safari browser" is not a claim to browse.
        let claimed = sentences.iter().any(|s| {
            let words = format!(" {} ", s.chars().map(|c| if c.is_alphanumeric() || c == '\'' { c } else { ' ' })
                .collect::<String>().split_whitespace().collect::<Vec<_>>().join(" "));
            phrases.iter().any(|p| words.contains(&format!(" {p} ")))
                && !negated(&words)
                && about_kue(&words)
        });
        if claimed { found.push(name.to_string()); }
    }
    found
}

/// Capabilities KUE HAS that an answer told the owner it does not.
///
/// The mirror of `overclaims`, and the same shape: the phrase must be about
/// KUE, in a clause that negates it. Measured live on 2026-09-22: asked for a
/// square root, the model answered "No, I can't calculate anything. My
/// capabilities don't include this" — while arithmetic is a declared tool.
/// A denial of something KUE really cannot do is not here: those rows carry no
/// denial phrases.
pub fn underclaims(answer: &str, caps: &[Capability]) -> Vec<String> {
    let lower = answer.to_lowercase()
        .replace(", but ", ".").replace(" but ", ".").replace("however", ".")
        .replace(" although ", ".").replace(" though ", ".").replace(" except ", ".");
    let sentences: Vec<&str> = lower.split(|c| c == '.' || c == '!' || c == '?' || c == ';' || c == '\n').collect();
    let mut found: Vec<String> = Vec::new();
    for spec in crate::capabilities::REGISTRY.iter().filter(|s| !s.denial_phrases.is_empty()) {
        let exists = caps.iter().any(|c| c.name == spec.name && c.status != CapabilityStatus::NotImplemented);
        if !exists || found.iter().any(|f| f == spec.name) { continue; }
        let denied = sentences.iter().any(|s| {
            let words = format!(" {} ", s.chars().map(|c| if c.is_alphanumeric() || c == '\'' { c } else { ' ' })
                .collect::<String>().split_whitespace().collect::<Vec<_>>().join(" "));
            spec.denial_phrases.iter().any(|p| words.contains(&format!(" {p} ")))
                && negated(&words)
                && about_kue(&words)
        });
        if denied { found.push(spec.name.to_string()); }
    }
    found
}

/// The sentence shown with an answer that told the owner KUE cannot do
/// something it can. It says what KUE can actually do, in KUE's own words from
/// the capability list.
pub fn correction_for_denial(names: &[String]) -> Option<String> {
    if names.is_empty() { return None; }
    let said: Vec<&str> = names.iter()
        .filter_map(|n| crate::capabilities::by_name(n).map(|s| s.voice_description)).collect();
    Some(format!("Checked against KUE's capability list: {} {} implemented, so that part of the answer is wrong. {}",
        names.join(", "), if names.len() == 1 { "is" } else { "are" }, said.join(" ")))
}

/// The sentence shown with an answer that claimed something Lantern cannot do.
pub fn correction_for(names: &[String]) -> Option<String> {
    (!names.is_empty()).then(|| format!(
        "Checked against KUE's capability list: {} {} not implemented. KUE cannot do {} — that part of the answer is wrong.",
        names.join(", "), if names.len() == 1 { "is" } else { "are" },
        if names.len() == 1 { "that" } else { "those" }))
}

/// Removes a speaker label the model sometimes repeats from the prompt format.
pub fn strip_speaker_label(text: &str) -> &str {
    let mut t = text.trim_start();
    loop {
        let before = t;
        for label in ["KUE:", "Kue:", "kue:", "Lantern:", "lantern:", "LANTERN:", "Answer:"] {
            if let Some(rest) = t.strip_prefix(label) { t = rest.trim_start(); }
        }
        if t == before { return t; }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Pending {
    pub id: String,
    pub question: String,
    pub partial: String,
    pub started_at: f64,
    #[serde(default)]
    pub source: InputSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TurnOutcome {
    Answered,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Exchange {
    pub question: String,
    pub answer: String,
    pub outcome: TurnOutcome,
    pub seconds: f64,
    pub model: String,
    /// Data kinds the firewall kept out of this prompt.
    pub withheld: Vec<String>,
    /// Deterministic checks Lantern made on the answer, shown with it.
    #[serde(default)]
    pub corrections: Vec<String>,
    /// Typed or spoken. Answers to spoken questions are spoken.
    #[serde(default)]
    pub source: InputSource,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Conversation {
    pub exchanges: Vec<Exchange>,
    pub pending: Option<Pending>,
    #[serde(skip)]
    pending_withheld: Vec<String>,
    #[serde(skip)]
    next: u64,
    /// Why the conversation was last cleared, shown instead of an empty box.
    pub cleared_because: Option<String>,
}

pub const MAX_EXCHANGES: usize = 20;

impl Conversation {
    pub fn new() -> Self { Conversation::default() }

    pub fn is_busy(&self) -> bool { self.pending.is_some() }

    /// Earlier exchanges as the firewall's `Turn` type, for the prompt. A
    /// corrected answer carries its correction, so the model is not handed its
    /// own false claim as settled conversation.
    /// What a model may be told was said before. A request the safety boundary
    /// refused is not in it: the boundary refuses it BEFORE any model, and
    /// carrying its words into the next question's prompt would hand them to a
    /// model one turn later.
    pub fn history(&self) -> Vec<Turn> {
        self.exchanges.iter()
            .filter(|e| e.outcome == TurnOutcome::Answered && e.model != crate::safety::SAFETY_BOUNDARY_SOURCE)
            .map(|e| {
                let mut answer = e.answer.clone();
                for c in &e.corrections { answer.push_str(&format!(" ({c})")); }
                Turn { owner: e.question.clone(), answer: Some(answer) }
            }).collect()
    }

    /// An answer Lantern wrote itself, with no model and no pending question.
    pub fn answer_directly(&mut self, question: &str, answer: &str, source: &str, now: f64) -> Result<(), String> {
        self.answer_directly_from(question, answer, source, InputSource::Text, now)
    }

    /// `answer_directly`, for a question that arrived typed or spoken.
    pub fn answer_directly_from(&mut self, question: &str, answer: &str, source: &str, input: InputSource, now: f64) -> Result<(), String> {
        if self.pending.is_some() { return Err("KUE is still answering the previous question.".into()); }
        self.cleared_because = None;
        self.exchanges.push(Exchange {
            question: question.to_string(), answer: answer.to_string(), outcome: TurnOutcome::Answered,
            seconds: 0.0, model: source.to_string(), withheld: Vec::new(), corrections: Vec::new(), source: input,
        });
        if self.exchanges.len() > MAX_EXCHANGES { self.exchanges.remove(0); }
        let _ = now;
        Ok(())
    }

    pub fn begin(&mut self, question: &str, withheld: Vec<String>, now: f64) -> Result<String, String> {
        self.begin_from(question, InputSource::Text, withheld, now)
    }

    /// `begin`, for a question that arrived typed or spoken.
    pub fn begin_from(&mut self, question: &str, input: InputSource, withheld: Vec<String>, now: f64) -> Result<String, String> {
        if self.pending.is_some() { return Err("KUE is still answering the previous question.".into()); }
        self.next += 1;
        let id = format!("q{}", self.next);
        self.pending = Some(Pending { id: id.clone(), question: question.to_string(), partial: String::new(), started_at: now, source: input });
        self.pending_withheld = withheld;
        self.cleared_because = None;
        Ok(id)
    }

    pub fn partial(&mut self, id: &str, text: &str) {
        // The same cut as the finished answer, so an invented owner turn never flashes by while it streams.
        if let Some(p) = self.pending.as_mut().filter(|p| p.id == id) {
            p.partial = crate::model::cut_invented_turn(strip_speaker_label(text)).0.to_string();
        }
    }

    /// Returns false for an answer to a question this conversation no longer
    /// holds (cleared or cancelled) — which is then dropped, not shown.
    pub fn finish(&mut self, id: &str, outcome: TurnOutcome, text: &str, model: &str, now: f64) -> bool {
        self.finish_checked(id, outcome, text, model, &[], now)
    }

    /// Finishes a model answer, checking it against the capability table.
    pub fn finish_checked(&mut self, id: &str, outcome: TurnOutcome, text: &str, model: &str,
                          caps: &[Capability], now: f64) -> bool {
        self.finish_checked_against(id, outcome, text, model, caps, &[], now)
    }

    /// The same, with what KUE verified. A model may explain a confirmed action
    /// and reason from it; if it tells the owner KUE does not know something
    /// KUE confirmed, the correction says so before the answer is shown.
    pub fn finish_checked_against(&mut self, id: &str, outcome: TurnOutcome, text: &str, model: &str,
                                  caps: &[Capability], verified: &[&crate::facts::Fact], now: f64) -> bool {
        let Some(p) = self.pending.take_if(|p| p.id == id) else { return false };
        // A model's answer is untrusted text: checked by `model::check_answer`
        // before it is kept, shown or spoken. A failure message is KUE's own.
        let (answer, corrections) = if outcome == TurnOutcome::Answered {
            let checked = crate::model::check_answer_against(text, caps, verified);
            (checked.text, checked.corrections)
        } else { (strip_speaker_label(text).trim().to_string(), Vec::new()) };
        self.exchanges.push(Exchange {
            question: p.question, answer, outcome,
            seconds: (now - p.started_at).max(0.0), model: model.to_string(),
            withheld: std::mem::take(&mut self.pending_withheld),
            corrections, source: p.source,
        });
        if self.exchanges.len() > MAX_EXCHANGES { self.exchanges.remove(0); }
        true
    }

    pub fn clear(&mut self, because: &str) {
        if self.exchanges.is_empty() && self.pending.is_none() { return; }
        self.exchanges.clear();
        self.pending = None;
        self.pending_withheld.clear();
        self.cleared_because = Some(because.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::context::{Capability, CapabilityStatus};

    fn caps() -> Vec<Capability> {
        let c = |n: &str, s: CapabilityStatus, note: &str| Capability { name: n.into(), status: s, note: note.into() };
        vec![
            c("Camera capture", CapabilityStatus::Real, ""),
            c("Natural conversation", CapabilityStatus::Partial, ""),
            c("Internet research", CapabilityStatus::NotImplemented, "KUE makes no network requests."),
            c("Screen context", CapabilityStatus::NotImplemented, ""),
            c("Emotion or mood reading", CapabilityStatus::NotImplemented, "Deliberately excluded."),
            c("Computer automation", CapabilityStatus::Real, ""),
        ]
    }

    #[test]
    fn general_capability_questions_are_recognised_and_ordinary_ones_are_not() {
        for q in ["What can you do?", "what can you do", "Hey, what can you do?", "So what are your capabilities?",
                  "WHAT CAN LANTERN DO", "What can't you do?", "what are you capable of",
                  "Hi, what will you do?", "How can you help me?"] {
            assert!(is_capability_question(q), "{q}");
        }
        for q in ["What am I doing?", "Which app is frontmost?", "Can you tell me what you can see?",
                  "What do you think I'm doing?", "Hello."] {
            assert!(!is_capability_question(q), "{q}");
        }
    }

    #[test]
    fn a_command_with_nothing_to_act_on_gets_a_short_answer_not_a_model_essay() {
        let a = incomplete_command("Can you open?").expect("measured: this went to the model");
        assert!(a.starts_with("Open what?") && a.contains("open my resume"), "{a}");
        assert!(incomplete_command("please quit").unwrap().starts_with("Quit what?"));
        assert!(incomplete_command("Lantern, switch to").is_some());
        for q in ["Can you open Safari?", "What is open right now?", "open my resume", "Hello.", "Is Safari open?"] {
            assert_eq!(incomplete_command(q), None, "{q}");
        }
    }

    #[test]
    fn the_capability_answer_comes_from_the_table() {
        let a = capability_answer(&caps());
        assert!(a.contains("working now — camera capture and computer automation"), "{a}");
        assert!(a.contains("Working with stated limits — natural conversation"), "{a}");
        assert!(a.contains("Not implemented — internet research and screen context"), "{a}");
        assert!(a.contains("Deliberately excluded — emotion or mood reading"), "{a}");
    }

    #[test]
    fn a_measured_overclaim_is_caught_and_a_denial_is_not() {
        // The on-device model's actual answer on this Mac.
        let real = "I can assist you by answering questions, providing information, and performing tasks related to the computer. \
                    I can help you navigate the web, search for information, and perform other tasks that are supported by my capabilities.";
        assert_eq!(overclaims(real, &caps()), vec!["Internet research".to_string()]);
        assert!(overclaims("I cannot browse the internet or see your screen.", &caps()).is_empty());
        // Also measured: a claim and a denial in the same sentence.
        let mixed = "I can help you search the internet for information, but I can't browse the web or access external resources.";
        assert_eq!(overclaims(mixed, &caps()), vec!["Internet research".to_string()]);
        assert!(overclaims("I don't have internet access, but Safari is frontmost.", &caps()).is_empty());
        assert!(overclaims("Safari is the frontmost application.", &caps()).is_empty());
        // Measured false positive before whole-word matching.
        assert!(overclaims("You are interacting with the computer, as evidenced by the presence of the Safari browser \
            as the frontmost application.", &caps()).is_empty());
        assert_eq!(overclaims("I can read your screen and your mood.", &caps()),
            vec!["Screen context".to_string(), "Emotion or mood reading".to_string()]);
        // A capability that IS implemented is never "corrected".
        let mut implemented = caps();
        implemented[2].status = CapabilityStatus::Real;
        assert!(overclaims(real, &implemented).is_empty());
    }

    /// The tests above build their own rows, so they would keep passing even if
    /// the checker had stopped firing against the capabilities KUE actually
    /// ships. This one runs against the real registry.
    #[test]
    fn the_checker_fires_against_the_capabilities_kue_actually_ships() {
        let shipped = crate::capabilities::rows();
        // Named through the registry, not spelled out: renaming the capability
        // must keep the correction working, which is the whole point of the
        // phrases living on the capability. Verified by mutation — renaming
        // "Internet research" leaves this passing, and under the old design
        // (a second phrase table matched by name) it returned nothing at all.
        let web = crate::capabilities::find("internet_research").unwrap().name.to_string();
        assert_eq!(overclaims("I can search the web for that.", &shipped), vec![web]);
        assert!(overclaims("I can't search the web — I have no internet access.", &shipped).is_empty());

        // Every not-implemented capability that carries phrases must be
        // reachable by at least one of them through the shipped rows. A rename
        // that broke the coupling would empty one of these.
        for spec in crate::capabilities::REGISTRY.iter().filter(|s| !s.claim_phrases.is_empty()) {
            let phrase = spec.claim_phrases[0];
            let caught = overclaims(&format!("I can use {phrase} for you."), &shipped);
            assert!(caught.contains(&spec.name.to_string()),
                "claiming {:?} no longer corrects {} — the phrases and the capability have drifted apart",
                phrase, spec.id);
        }
    }

    /// The other half of the checker's job: leaving a correct answer alone. A
    /// correction printed under good advice is as damaging as a missing one,
    /// and it is the failure mode phrases drift into — "click on" and "type
    /// into" matched instructions to the owner, not claims by KUE.
    #[test]
    fn telling_the_owner_what_to_do_is_never_corrected_as_a_claim() {
        let shipped = crate::capabilities::rows();
        for answer in [
            "You can click on Confirm to go ahead.",
            "Type into the box at the bottom and press return.",
            "Safari is the frontmost application.",
            "You can look at your screen to check.",
            "Your habits are your own business.",
            "I can open Google Chrome for you.",
            "I can tell you which app is in front.",
        ] {
            assert!(overclaims(answer, &shipped).is_empty(),
                "corrected an answer that claimed nothing: {answer:?} → {:?}", overclaims(answer, &shipped));
        }
        // A claim wearing an instruction's clothes is still a claim.
        assert_eq!(overclaims("You can ask me to search the web for you.", &shipped),
            vec![crate::capabilities::find("internet_research").unwrap().name.to_string()]);
    }

    #[test]
    fn what_is_excluded_on_purpose_is_read_from_the_capability_not_its_wording() {
        let a = capability_answer(&crate::capabilities::rows());
        // Marked deliberate in the registry.
        assert!(a.contains("Deliberately excluded — emotion or mood reading and deleting files"), "{a}");
        // Not marked, and so listed as simply missing — speaker identity among
        // them: it waits on a decision, it is not ruled out.
        assert!(a.contains("Not implemented — screen context, speaker identity, internet research"), "{a}");
    }

    #[test]
    fn a_checked_answer_carries_its_correction_and_loses_the_speaker_label() {
        let mut c = Conversation::new();
        let id = c.begin("What can you help with?", vec![], 0.0).unwrap();
        c.partial(&id, "KUE: I can");
        assert_eq!(c.pending.as_ref().unwrap().partial, "I can");
        assert!(c.finish_checked(&id, TurnOutcome::Answered, "KUE: I can browse the web for you.", "m", &caps(), 1.0));
        let x = &c.exchanges[0];
        assert_eq!(x.answer, "I can browse the web for you.");
        assert_eq!(x.corrections.len(), 1);
        assert!(x.corrections[0].contains("Internet research is not implemented"), "{:?}", x.corrections);
        assert!(c.history()[0].answer.as_ref().unwrap().contains("not implemented"),
            "the model must not be handed its false claim as settled conversation");
    }

    #[test]
    fn an_answer_arriving_after_a_clear_is_dropped() {
        let mut c = Conversation::new();
        let id = c.begin("what am I doing?", vec![], 0.0).unwrap();
        c.clear("locked");
        assert!(!c.finish(&id, TurnOutcome::Answered, "secret", "m", 1.0));
        assert!(c.exchanges.is_empty());
        assert_eq!(c.cleared_because.as_deref(), Some("locked"));
    }

    #[test]
    fn one_question_at_a_time_and_failures_are_not_fed_back() {
        let mut c = Conversation::new();
        let a = c.begin("one", vec![], 0.0).unwrap();
        assert!(c.begin("two", vec![], 0.1).is_err());
        c.finish(&a, TurnOutcome::Failed, "GUARDRAIL", "m", 1.0);
        let b = c.begin("three", vec![], 2.0).unwrap();
        c.finish(&b, TurnOutcome::Answered, "ok", "m", 3.0);
        assert_eq!(c.history(), vec![Turn { owner: "three".into(), answer: Some("ok".into()) }]);
    }
}
