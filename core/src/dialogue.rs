//! A conversation, rather than a series of unrelated requests.
//!
//! Until now every utterance was treated as new: "Do it" meant nothing unless
//! it arrived through one particular window command, and "No, don't touch the
//! installers" would have been parsed as a fresh request about installers. This
//! module holds what makes a conversation a conversation — **what is open**:
//! the action waiting for a confirm, the goal waiting for the owner to choose,
//! the question KUE asked — and decides, for each new utterance, whether it
//! answers that or starts something new.
//!
//! It is transport-independent on purpose. A turn is the same turn whether it
//! was spoken after the wake word, spoken after a button press, typed, or — one
//! day — raised by KUE itself from a scheduled or observed event.
//!
//! WHAT THIS DECIDES AND WHAT IT NEVER DOES. It decides what an utterance
//! *refers to*. It never authorizes anything: "Do it" becomes a Confirmation of
//! the open action, and the confirmation then goes through the same
//! re-authorization, the same Touch ID prompt and the same verification as a
//! click on Confirm. Understanding a word is not permission.
//!
//! When an utterance looks like a correction and KUE cannot tell what it
//! corrects, the answer is to ASK — never to guess which files to leave out.

use crate::storage::Category;
use serde::Serialize;

/// Every kind of turn in a conversation. Some are said by the owner, some by
/// KUE, and some are the runtime reporting on work in progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TurnKind {
    UserSpeech,
    KueResponse,
    KueQuestion,
    UserConfirmation,
    UserCorrection,
    UserCancellation,
    KueSuggestion,
    ToolProgress,
    ToolResult,
    VerificationResult,
    GoalProgress,
}

impl TurnKind {
    pub fn by_owner(self) -> bool {
        matches!(self, TurnKind::UserSpeech | TurnKind::UserConfirmation
                     | TurnKind::UserCorrection | TurnKind::UserCancellation)
    }
}

/// One turn. `said` is held in memory for the conversation and is never written
/// to local memory: what the owner says belongs in the conversation, not in an
/// archive (the same rule the existing conversation box follows).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Turn {
    pub id: String,
    pub kind: TurnKind,
    pub said: String,
    pub at: f64,
    pub request_id: Option<String>,
    pub goal_id: Option<String>,
    pub tool_execution_id: Option<String>,
    pub verification_id: Option<String>,
}

/// What the conversation is waiting on. At most one thing is open at a time,
/// because "Do it" can only ever mean one thing.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", tag = "kind")]
pub enum Open {
    /// An action is proposed and waits for the owner to confirm it.
    Confirmation { request_id: String, tool_execution_id: String },
    /// A goal has laid out options and waits for the owner to choose.
    /// `proposed` is exactly what KUE last announced it would move — "do it"
    /// confirms that set and nothing else. Paths stay in memory: they are
    /// never serialized into any projection of the conversation.
    Selection { request_id: String, goal_id: String, facets: Vec<Facet>,
                #[serde(skip)] proposed: Vec<String> },
    /// KUE asked the owner something and waits for the answer.
    Question { request_id: String },
    /// A plan has been proposed, checked against the declared tools, and waits
    /// for the owner's yes. "Do it" approves THIS plan and nothing else: the
    /// id is what the approval binds to, and the steps are fixed when it is
    /// approved. `plan_id` names it; nothing about the files is kept here.
    Plan { request_id: String, plan_id: String },
}

impl Open {
    pub fn request_id(&self) -> &str {
        match self {
            Open::Confirmation { request_id, .. } | Open::Selection { request_id, .. }
            | Open::Question { request_id } | Open::Plan { request_id, .. } => request_id,
        }
    }
}

/// Something an open selection is made of, which the owner can refer to by
/// name: "the installers", "the duplicates". Only what the open thing actually
/// contains is listed, so a correction can only name something real.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", tag = "kind", content = "value")]
pub enum Facet {
    Storage(Category),
    /// A kind of file, by extension: "leave the PDF".
    FileType(FileType),
}

/// File types the owner can name. Only types a storage check can actually
/// offer are here; a type not in an open selection cannot be named.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FileType { Pdf, Zip, Dmg, Pkg, Keynote, Movie, Image }

impl FileType {
    pub fn of(name: &str) -> Option<FileType> {
        let ext = name.rsplit('.').next()?.to_lowercase();
        Some(match ext.as_str() {
            "pdf" => FileType::Pdf, "zip" => FileType::Zip, "dmg" => FileType::Dmg, "pkg" => FileType::Pkg,
            "key" => FileType::Keynote, "mov" | "mp4" | "m4v" => FileType::Movie,
            "jpg" | "jpeg" | "png" | "heic" => FileType::Image,
            _ => return None,
        })
    }
    pub fn said(self, n: usize) -> String {
        let (one, many) = match self {
            FileType::Pdf => ("PDF", "PDFs"), FileType::Zip => ("zip file", "zip files"),
            FileType::Dmg => ("disk image", "disk images"), FileType::Pkg => ("package", "packages"),
            FileType::Keynote => ("Keynote file", "Keynote files"), FileType::Movie => ("video", "videos"),
            FileType::Image => ("picture", "pictures"),
        };
        if n == 1 { one.to_string() } else { format!("{n} {many}") }
    }
}

/// Every facet the owner can name, for reading a standing preference. Storage
/// kinds only: a preference about a FILE TYPE is not yet something a clean-up
/// plan is filtered by, and pretending otherwise would change plans for a
/// reason KUE could not explain.
pub const STORAGE_FACETS: [Facet; 4] = [
    Facet::Storage(Category::Installer), Facet::Storage(Category::DuplicateCopy),
    Facet::Storage(Category::OldDownload), Facet::Storage(Category::LargeFile),
];

/// What a standing preference asks KUE to do with a kind of file when it
/// cleans up. Read by rule from the owner's own sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct StorageWish {
    pub facet: Facet,
    /// True: include this kind. False: leave it out.
    pub include: bool,
}

/// Words that say "leave it out" and words that say "put it in". A sentence
/// carrying neither is not a wish about cleaning up, and is left alone.
const LEAVE_OUT: [&str; 12] = ["don't want", "dont want", "do not want", "don't include", "dont include",
    "not include", "never include", "exclude", "leave out", "leave the", "skip", "without"];
const PUT_IN: [&str; 6] = ["include", "do include", "always include", "put in", "keep the", "do clean"];
/// What makes a sentence about how things are done from now on, rather than
/// about the task in hand.
const STANDING: [&str; 6] = ["from now on", "always", "never", "in future", "in the future", "instead"];

/// A standing wish about cleaning up, read from one sentence — or None, which
/// is what most sentences are. Narrow on purpose: it must name a kind of file
/// KUE's clean-up actually knows, and say plainly whether to leave it out.
///
/// This is the one place a remembered sentence turns into something that
/// changes a plan, so it is a rule, it is small, and it is tested.
pub fn storage_wish(statement: &str) -> Option<StorageWish> {
    let t = format!(" {} ", statement.to_lowercase());
    let facet = STORAGE_FACETS.into_iter().find(|f| f.words().iter().any(|w| contains_word(&t, w)))?;
    // "cleaning", "clean-up", "storage", "tidy": what the wish is ABOUT. A
    // sentence that never mentions it is not about cleaning up.
    let about_cleanup = ["clean", "cleaning", "cleanup", "clean-up", "clean up", "storage", "tidy", "tidying", "trash"]
        .iter().any(|w| t.contains(w));
    if !about_cleanup { return None; }
    let out = LEAVE_OUT.iter().any(|w| t.contains(w));
    let inc = PUT_IN.iter().any(|w| t.contains(w));
    // Both or neither is not a wish KUE can read, and it does not guess.
    match (out, inc) {
        (true, false) => Some(StorageWish { facet, include: false }),
        (false, true) => Some(StorageWish { facet, include: true }),
        // "don't include" contains "include": a leave-out phrase wins only
        // when it is the one that matched the longer, more specific words.
        (true, true) => Some(StorageWish { facet, include: false }),
        (false, false) => None,
    }
}

/// Whether a sentence is about how things are done from now on, rather than
/// about the task in hand. "Include installers from now on" is a preference;
/// "leave the installers" during a clean-up is a correction to that clean-up.
pub fn is_standing(statement: &str) -> bool {
    let t = format!(" {} ", statement.to_lowercase());
    STANDING.iter().any(|w| t.contains(w))
}

impl Facet {
    /// The words the owner might use for this facet.
    pub fn said_as(self) -> &'static [&'static str] { self.words() }

    fn words(self) -> &'static [&'static str] {
        match self {
            // An installer is named as an installer; "the disk image" names the
            // file type, so one file is never named twice by one word.
            Facet::Storage(Category::Installer) => &["installer", "installers"],
            Facet::Storage(Category::DuplicateCopy) => &["duplicate", "duplicates", "copies", "copy"],
            Facet::Storage(Category::OldDownload) => &["old download", "old downloads", "downloads"],
            Facet::Storage(Category::LargeFile) => &["large file", "large files", "big file", "big files"],
            Facet::FileType(FileType::Pdf) => &["pdf", "pdfs"],
            Facet::FileType(FileType::Zip) => &["zip", "zips", "zip file", "zip files"],
            Facet::FileType(FileType::Dmg) => &["disk image", "disk images", "dmg", "dmgs"],
            Facet::FileType(FileType::Pkg) => &["package", "packages", "pkg", "pkgs"],
            Facet::FileType(FileType::Keynote) => &["keynote", "keynotes", "presentation", "presentations", "deck", "decks"],
            Facet::FileType(FileType::Movie) => &["video", "videos", "movie", "movies"],
            Facet::FileType(FileType::Image) => &["picture", "pictures", "photo", "photos", "image", "images"],
        }
    }
}

/// A change to a plan in progress, from the owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", tag = "kind", content = "facet")]
pub enum Revision {
    /// Leave these out — every one named. Always safe to apply without asking:
    /// it can only leave MORE of the owner's files alone.
    Exclude(Vec<Facet>),
    /// Only these.
    OnlyInclude(Facet),
    /// "Actually do all of them": undo every exclusion.
    IncludeAll,
    /// "Go back": undo the last correction.
    Undo,
    /// "Nothing" — in answer to "which should I leave out?": leave nothing
    /// (else) out. Changes nothing; KUE says what the plan is.
    LeaveNothingOut,
}

/// A question about the work itself, answered from the runtime — never by a
/// model reconstructing what KUE is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkQuestion {
    WhatAreYouDoing,
    Why,
    HowMany,
    WhatsLeft,
    WhatDidYouDo,
    CanYouUndo,
}

/// What an utterance is, relative to what is open.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", tag = "kind")]
pub enum Interpretation {
    /// Something new. Nothing open is affected.
    NewRequest,
    /// "Yes", "do it": confirm the open thing. Authorization is still asked for.
    Confirmation { request_id: String },
    /// "Stop", "no", "never mind": cancel the open thing.
    Cancellation { request_id: String },
    /// "No, don't touch the installers": revise the open plan.
    Correction { request_id: String, revision: Revision },
    /// A change to the plan that is waiting. Which step it means depends on
    /// the plan, which dialogue does not hold: the runtime resolves it with
    /// `plan_change` and the words come along unread.
    PlanChange { request_id: String, said: String },
    /// It looks like a correction, and KUE cannot tell what it corrects. Ask.
    Clarify { request_id: String, question: String },
    /// A question about KUE's own work. Answered from runtime state.
    AboutWork { question: WorkQuestion },
    /// "Undo that", "put them back": reverse the last thing KUE did.
    UndoLast,
    /// Something about what KUE keeps: remember this, what do you remember,
    /// forget that, why do you remember it. Answered from KUE's own memory,
    /// never by a model.
    Memory(MemoryAsk),
}

/// What the owner is asking about memory. The words they used travel with it
/// unread: the runtime holds the memory, not the dialogue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", tag = "ask", content = "said")]
pub enum MemoryAsk {
    /// "Remember that I prefer PDF reports." `replacing` is the owner saying
    /// this takes the place of what they said before ("instead", "not any
    /// more") — a decision of theirs, never a guess of KUE's.
    Keep { statement: String, replacing: bool },
    /// "What do you remember about my report preferences?"
    Recall { about: String },
    /// "Forget that I prefer PDF reports."
    Forget { about: String },
    /// "Why do you remember that?"
    Why,
}

/// How an owner asks KUE to keep something. Narrow on purpose: everything else
/// is a request or a question, and goes where it always went.
const KEEP_LEADS: [&str; 8] = ["remember that ", "remember this ", "remember ", "note that ",
    "keep in mind that ", "keep in mind ", "don't forget that ", "dont forget that "];
const RECALL_LEADS: [&str; 12] = ["what do you remember about ", "what do you know about ",
    "do you remember ", "do you know ",
    "what did i tell you about ", "what do you remember of ", "what have you got about ",
    "what should you remember about ", "what do you remember regarding ", "tell me what you remember about ",
    "what did i say about ", "what do you remember "];
const FORGET_LEADS: [&str; 7] = ["forget that ", "forget about ", "forget what i said about ",
    "you can forget that ", "you can forget about ", "stop remembering that ", "forget "];
const WHY_REMEMBERED: [&str; 6] = ["why do you remember that", "why do you remember this",
    "why do you know that", "where did that come from", "why is that remembered", "how do you know that"];
/// The owner saying a new memory takes the place of the old one.
const REPLACING: [&str; 7] = [" instead", "not any more", "not anymore", "no longer", "from now on",
    "rather than", "not that any more"];

/// Whether an utterance is about what KUE keeps, and what it asks for.
///
/// Checked before anything else, like a question about KUE's own work: "forget
/// that" is never a correction to a plan, and "remember that I prefer PDFs" is
/// never a new request to go and do something.
pub fn memory_ask(text: &str) -> Option<MemoryAsk> {
    let n = normalize(text);
    let bare = n.trim_end_matches(|c: char| !c.is_alphanumeric());
    if WHY_REMEMBERED.contains(&bare) { return Some(MemoryAsk::Why); }
    if bare == "what do you remember" || bare == "what do you remember about me" {
        return Some(MemoryAsk::Recall { about: String::new() });
    }
    // What the owner said comes back in THEIR words: the lead is found on a
    // lower-cased copy, and the rest is cut from the original. "PDF" is not
    // "pdf", and a memory KUE keeps should read as the person wrote it.
    let after = |leads: &[&str]| -> Option<String> {
        let start = text.len() - text.trim_start().len();
        let rest = &text[start..];
        let lower = rest.to_lowercase();
        // The polite words the normaliser drops, dropped here too, so "please
        // remember that…" is the same as "remember that…".
        let (rest, lower) = LEAD.iter().find_map(|p| {
            let p = format!("{p} ");
            lower.strip_prefix(&p).map(|l| (&rest[p.len()..], l.to_string()))
        }).unwrap_or((rest, lower));
        leads.iter().find_map(|l| lower.starts_with(l).then(|| rest[l.len()..].to_string()))
            .map(|r| r.trim().trim_end_matches(['.', '!', '?']).to_string())
            .filter(|r| !r.is_empty())
    };
    // A question about what is kept comes before "remember that…": "what do
    // you remember about X" begins with neither lead but contains both.
    if let Some(about) = after(&RECALL_LEADS) { return Some(MemoryAsk::Recall { about }); }
    if let Some(about) = after(&FORGET_LEADS) { return Some(MemoryAsk::Forget { about }); }
    if let Some(statement) = after(&KEEP_LEADS) {
        let replacing = REPLACING.iter().any(|r| n.contains(r));
        let statement = statement.trim().to_string();
        return Some(MemoryAsk::Keep { statement, replacing });
    }
    None
}

/// Exact-ish phrases only. "Why is my storage full?" is a storage request, not a
/// question about KUE's work; "why?" on its own is.
fn work_question(n: &str) -> Option<WorkQuestion> {
    let n = n.trim_end_matches(|c: char| !c.is_alphanumeric());
    Some(match n {
        "what are you doing" | "what's happening" | "what is happening" | "what are you working on"
        | "status" | "where are we" | "what's going on" => WorkQuestion::WhatAreYouDoing,
        "why" | "why did you do that" | "why that" | "why are you asking" | "why do you need that"
        | "why is that" => WorkQuestion::Why,
        "how many" | "how many files" | "how many are there" | "how many is that" => WorkQuestion::HowMany,
        "what's left" | "what is left" | "what remains" | "what's remaining" => WorkQuestion::WhatsLeft,
        "what did you just do" | "what did you do" | "what just happened" | "what happened" => WorkQuestion::WhatDidYouDo,
        "can you undo that" | "can you undo it" | "can you put them back" | "can you put it back" => WorkQuestion::CanYouUndo,
        _ => return None,
    })
}

const UNDO: [&str; 6] = ["undo that", "undo it", "put them back", "put it back", "undo", "restore them"];

const LEAD: [&str; 9] = ["computer", "kue", "lantern", "hey", "okay", "ok", "actually", "please", "well"];
/// How a correction usually begins: a refusal of part of what was proposed.
const CORRECTING: [&str; 12] = ["no ", "don't", "dont ", "do not", "not the", "except", "leave ", "skip ",
                                "without", "but not", "exclude", "keep the"];
const ONLY: [&str; 3] = ["only the", "just the", "only "];

fn normalize(text: &str) -> String {
    let t: String = text.to_lowercase().chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '\'' { c } else { ' ' }).collect();
    let mut words: Vec<&str> = t.split_whitespace().collect();
    while words.len() > 1 && LEAD.contains(&words[0]) { words.remove(0); }
    words.join(" ")
}

/// Decides what an utterance refers to. Pure: no clock, no state change.
pub fn interpret(text: &str, open: Option<&Open>) -> Interpretation {
    let n = normalize(text);
    // A question about the work is answered whether or not anything is open:
    // "what did you just do?" is asked after the work has finished.
    if let Some(q) = work_question(&n) { return Interpretation::AboutWork { question: q }; }
    // The same for what KUE keeps: "forget that" is about memory whatever else
    // is waiting, and it never touches the plan on screen.
    if let Some(ask) = memory_ask(text) { return Interpretation::Memory(ask); }
    // A standing wish said plainly, without "remember that": "include
    // installers from now on" is about how things are done from here, not
    // about the plan on screen — that is what "from now on" means. It is
    // offered to memory as a replacement, and the memory rules decide whether
    // anything is actually replaced.
    if is_standing(text) && storage_wish(text).is_some() {
        // "Actually, include installers…" is kept as "include installers…":
        // the word that opened the sentence is not part of what they want.
        let said = text.trim().trim_end_matches(['.', '!', '?']);
        let said = ["actually,", "actually", "ok,", "okay,", "so,", "well,", "and", "also,", "please"]
            .iter()
            .find_map(|w| said.get(..w.len()).filter(|start| start.eq_ignore_ascii_case(w)).map(|_| said[w.len()..].trim()))
            .unwrap_or(said);
        return Interpretation::Memory(MemoryAsk::Keep { statement: said.to_string(), replacing: true });
    }
    let bare = n.trim_end_matches(|c: char| !c.is_alphanumeric());
    let Some(open) = open else {
        if UNDO.contains(&bare) { return Interpretation::UndoLast; }
        return Interpretation::NewRequest;
    };
    let rid = open.request_id().to_string();

    // A correction is checked FIRST, because it usually begins with "no" — and
    // "no, don't touch the installers" is not "no".
    if let Open::Selection { facets, .. } = open {
        // Undo every exclusion, or the last one.
        if ["actually do all of them", "do all of them", "all of them", "everything", "do everything",
            "include everything", "include all of them", "all of it"].contains(&bare) {
            return Interpretation::Correction { request_id: rid, revision: Revision::IncludeAll };
        }
        if ["go back", "undo that", "undo", "never mind that", "put that back"].contains(&bare) {
            return Interpretation::Correction { request_id: rid, revision: Revision::Undo };
        }
        // Found live 2026-09-22: asked "Which should I leave out?", the owner
        // said "nothing", and it went to the model as a new request.
        if ["nothing", "none", "nothing else", "none of them", "leave nothing out", "don't leave anything out",
            "nothing thanks", "none thanks", "no nothing", "keep them all"].contains(&bare) {
            return Interpretation::Correction { request_id: rid, revision: Revision::LeaveNothingOut };
        }
        let named: Vec<Facet> = facets.iter().copied()
            .filter(|f| f.words().iter().any(|w| contains_word(&n, w))).collect();
        let correcting = CORRECTING.iter().any(|c| n.starts_with(c) || n.contains(&format!(" {}", c.trim())));
        let only = ONLY.iter().any(|o| n.starts_with(o));
        match (named.as_slice(), correcting, only) {
            ([one], _, true) => return Interpretation::Correction { request_id: rid, revision: Revision::OnlyInclude(*one) },
            ([one], true, false) => return Interpretation::Correction { request_id: rid, revision: Revision::Exclude(vec![*one]) },
            // It refuses something, and names nothing KUE offered; or it asks
            // for "only" several things at once. Guessing which of someone's
            // files to act on is the mistake not to make.
            ([], true, _) | ([_, _, ..], _, true) => {
                return Interpretation::Clarify { request_id: rid,
                    question: "Which should I leave out? You can name installers, duplicates, old downloads or large files.".into() };
            }
            // Leaving SEVERAL things out needs no question: whatever the owner
            // meant, leaving more of their files alone is the safe direction.
            (all @ [_, _, ..], true, false) => {
                return Interpretation::Correction { request_id: rid, revision: Revision::Exclude(all.to_vec()) };
            }
            _ => {}
        }
    }

    // A plan waiting for a yes can be changed before it gets one. What the
    // owner named is matched against the plan itself, one layer up.
    if let Open::Plan { .. } = open {
        if looks_like_a_plan_change(&n) {
            return Interpretation::PlanChange { request_id: rid, said: text.to_string() };
        }
    }

    match crate::voice::reference::interpret(&n) {
        Some(crate::voice::reference::Reference::Confirm) => Interpretation::Confirmation { request_id: rid },
        Some(crate::voice::reference::Reference::Cancel) => Interpretation::Cancellation { request_id: rid },
        // Choosing among document matches stays with the existing reference
        // path.
        Some(crate::voice::reference::Reference::Choose(_)) => Interpretation::NewRequest,
        // "Don't send it yet", "use the other folder": it changes what is
        // waiting, in a way KUE cannot apply. Asked about, and what is waiting
        // stays waiting — neither done nor dropped on a guess.
        None if CORRECTING.iter().chain(ELSEWHERE.iter()).any(|c| n.starts_with(c) || n.contains(&format!(" {}", c.trim()))) =>
            Interpretation::Clarify { request_id: rid,
                question: "I'm not sure what to change. It's still waiting — say “do it”, “cancel”, or tell me exactly what to do instead.".into() },
        None => Interpretation::NewRequest,
    }
}

// MARK: - Changing a plan that is waiting

/// What each step of the waiting plan can be called, so the owner's words can
/// be matched to one of them. The runtime builds these from the plan it holds
/// — dialogue never reads a plan, and these words go nowhere else.
#[derive(Debug, Clone)]
pub struct StepWords {
    pub index: usize,
    /// Lower-case words that mean this step: what its tool is called, and what
    /// it is about to touch.
    pub words: Vec<String>,
}

/// A change the owner asked for to a plan they have not approved.
///
/// Every one of these either narrows the plan or moves where it writes.
/// There is no variant that adds anything: a plan cannot grow by correction,
/// because a step nobody proposed has never been checked.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", tag = "change", content = "detail")]
pub enum PlanChange {
    /// Take these steps out, by the positions the plan holds them in.
    Drop(Vec<usize>),
    /// Put what the plan writes under this name instead.
    Destination(String),
    /// "Go back": the version before the last change.
    GoBack,
    /// KUE cannot tell what was meant. The plan stays exactly as it is, and
    /// this is the question to ask.
    Ask(String),
}

/// Adding to a plan is not a correction: a step nobody proposed has never
/// been checked against the declarations, and KUE will not invent one.
pub const CANNOT_ADD: &str = "I can only take steps out of a plan or change where it puts things — I can't add to one. \
Tell me the whole thing you want and I'll work out a fresh plan.";
pub const WHICH_STEP: &str = "Which step do you mean? You can say “don't do step two”, name what it does, \
or say “change the destination to …”.";
pub const WHICH_PLACE: &str = "I'm not sure where you mean. Say “change the destination to …” with the name of one folder.";

const GO_BACK: [&str; 5] = ["go back", "undo that", "undo the change", "never mind that", "put that back"];
const ADDING: [&str; 6] = ["add ", "also ", "as well", "include the", "and the ", "plus the"];
/// Ways of naming somewhere else to put things. Each is followed by the name.
const DESTINATION: [&[&str]; 10] = [
    &["change", "the", "destination", "to"], &["change", "the", "destination"],
    &["put", "it", "in"], &["put", "them", "in"], &["put", "it", "into"], &["put", "them", "into"],
    &["move", "it", "to"], &["move", "them", "to"], &["save", "it", "in"], &["save", "it", "to"],
];
const NUMBERS: [&str; 12] = ["one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
                             "eleven", "twelve"];
const ORDINALS: [&str; 12] = ["first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth",
                              "ninth", "tenth", "eleventh", "twelfth"];

/// Whether an utterance is trying to change the plan rather than answer it.
/// Deliberately wide: everything it lets through is then either resolved
/// exactly or asked about, and nothing here changes anything by itself.
fn looks_like_a_plan_change(n: &str) -> bool {
    let bare = n.trim_end_matches(|c: char| !c.is_alphanumeric());
    GO_BACK.contains(&bare)
        || contains_word(n, "step")
        || ADDING.iter().chain(CORRECTING.iter()).chain(ONLY.iter()).chain(ELSEWHERE.iter())
            .any(|c| n.starts_with(c) || n.contains(&format!(" {}", c.trim())))
        || DESTINATION.iter().any(|lead| n.contains(&lead.join(" ")))
        || n.contains("instead")
}

/// Resolves what the owner said against the plan that is waiting.
///
/// Nothing is guessed. A change applies only when the words point at exactly
/// one step, or name one place; anything else comes back as a question, and
/// the plan is left as it is.
pub fn plan_change(text: &str, steps: &[StepWords]) -> PlanChange {
    let n = normalize(text);
    let bare = n.trim_end_matches(|c: char| !c.is_alphanumeric());
    if GO_BACK.contains(&bare) { return PlanChange::GoBack; }

    let correcting = CORRECTING.iter().any(|c| n.starts_with(c) || n.contains(&format!(" {}", c.trim())));
    let only = ONLY.iter().any(|o| n.starts_with(o) || n.contains(&format!(" {o}")));
    if !correcting && ADDING.iter().any(|a| n.starts_with(a) || n.contains(&format!(" {}", a.trim()))) {
        return PlanChange::Ask(CANNOT_ADD.into());
    }

    // Somewhere else to put it. Not read when the owner is refusing something:
    // "don't put it in Reports" takes a step out, it does not move one.
    let words: Vec<(String, &str)> = text.split_whitespace()
        .map(|w| (w.trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != '_' && c != '-')
                   .to_lowercase(), w)).collect();
    if !correcting {
        if let Some(name) = destination_named(&words) {
            return if name.is_empty() { PlanChange::Ask(WHICH_PLACE.into()) } else { PlanChange::Destination(name) };
        }
        // "Use the other folder", "not that one": somewhere else is meant, and
        // which is not said. Asked about; nothing is moved on a guess.
        if ELSEWHERE.iter().any(|e| n.starts_with(e) || n.contains(&format!(" {e}"))) {
            return PlanChange::Ask(WHICH_PLACE.into());
        }
    }

    // A step by its number, as the owner saw it. "The last step" is resolved
    // against the plan, which is why this needs the plan at all.
    if let Some(i) = step_numbered(&words) {
        if steps.is_empty() { return PlanChange::Ask(WHICH_STEP.into()); }
        let i = if i == LAST { steps[steps.len() - 1].index } else { i };
        return if only { drop_all_but(&[i], steps) } else { PlanChange::Drop(vec![i]) };
    }

    // A step by what it does, or what it touches. The step the owner's words
    // fit BEST, not every step they touch on: "don't create the folder" is
    // about the folder, though both steps create something.
    let matched: Vec<(usize, Vec<&str>)> = steps.iter()
        .map(|s| (s.index, s.words.iter().map(String::as_str).filter(|w| contains_word(&n, w)).collect::<Vec<_>>()))
        .filter(|(_, hit): &(usize, Vec<&str>)| !hit.is_empty()).collect();
    let best = matched.iter().map(|(_, hit)| hit.len()).max().unwrap_or(0);
    let top: Vec<&(usize, Vec<&str>)> = matched.iter().filter(|(_, hit)| hit.len() == best).collect();
    // Several steps fit equally well. When each was named by a different word
    // the owner named them all; when one word fits them both, which was meant
    // is a guess, and KUE asks.
    let named: Vec<usize> = if top.len() > 1
        && top.iter().any(|(_, hit)| hit.iter().any(|w| !top.iter().all(|(_, other)| other.contains(w)))) {
        top.iter().map(|(i, _)| *i).collect()
    } else if top.len() == 1 {
        vec![top[0].0]
    } else {
        Vec::new()
    };
    match (named.as_slice(), correcting, only) {
        ([i], _, true) => drop_all_but(&[*i], steps),
        ([i], true, false) => PlanChange::Drop(vec![*i]),
        (many @ [_, _, ..], _, true) => drop_all_but(many, steps),
        // Refusing several named steps at once needs no question: taking more
        // out of a plan is the safe direction.
        (many @ [_, _, ..], true, false) => PlanChange::Drop(many.to_vec()),
        _ => PlanChange::Ask(WHICH_STEP.into()),
    }
}

fn drop_all_but(keep: &[usize], steps: &[StepWords]) -> PlanChange {
    let drop: Vec<usize> = steps.iter().map(|s| s.index).filter(|i| !keep.contains(i)).collect();
    if drop.is_empty() { PlanChange::Ask(WHICH_STEP.into()) } else { PlanChange::Drop(drop) }
}

/// The name after "put it in", "change the destination to" and the like.
/// Some(String::new()) means a lead was said with no name after it.
fn destination_named(words: &[(String, &str)]) -> Option<String> {
    let lower: Vec<&str> = words.iter().map(|(l, _)| l.as_str()).collect();
    let at = DESTINATION.iter().find_map(|lead| {
        lower.windows(lead.len()).position(|w| w == *lead).map(|i| i + lead.len())
    })?;
    // The name runs to the end, or to "instead".
    let end = lower.iter().skip(at).position(|w| *w == "instead").map_or(words.len(), |o| at + o);
    let name: Vec<&str> = words[at..end].iter()
        .filter(|(l, _)| !l.is_empty() && !["the", "a", "my", "folder", "called", "named"].contains(&l.as_str()))
        .map(|(_, original)| original.trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != '_' && c != '-'))
        .filter(|w| !w.is_empty()).collect();
    Some(name.join(" "))
}

/// "the last step", resolved against the plan by the caller.
const LAST: usize = usize::MAX;

/// "step two", "step 2", "the second step", "the last step" — the position the
/// owner saw, as an index. None when no step is numbered.
fn step_numbered(words: &[(String, &str)]) -> Option<usize> {
    let lower: Vec<&str> = words.iter().map(|(l, _)| l.as_str()).collect();
    let at = lower.iter().position(|w| *w == "step" || *w == "steps")?;
    let number = |w: &str| -> Option<usize> {
        if let Ok(n) = w.parse::<usize>() { return (n >= 1).then(|| n - 1); }
        NUMBERS.iter().position(|x| *x == w).or_else(|| ORDINALS.iter().position(|x| *x == w))
    };
    if let Some(n) = lower.get(at + 1).and_then(|w| number(w)) { return Some(n); }
    if at > 0 {
        if let Some(n) = number(lower[at - 1]) { return Some(n); }
        if lower[at - 1] == "last" { return Some(LAST); }
    }
    None
}

/// Words that point away from what was proposed without saying what instead.
const ELSEWHERE: [&str; 8] = ["use the other", "the other one", "a different", "not yet", "wait",
                              "somewhere else", "another folder", "elsewhere"];

fn contains_word(haystack: &str, phrase: &str) -> bool {
    let padded = format!(" {haystack} ");
    padded.contains(&format!(" {phrase} "))
}

/// One conversation: its turns, and what is open.
#[derive(Debug, Clone, Serialize)]
pub struct Dialogue {
    pub conversation_id: String,
    turns: Vec<Turn>,
    open: Option<Open>,
    next: u64,
    max_turns: usize,
}

impl Dialogue {
    pub fn new(conversation_id: impl Into<String>) -> Dialogue {
        Dialogue { conversation_id: conversation_id.into(), turns: Vec::new(), open: None, next: 1, max_turns: 60 }
    }

    pub fn open(&self) -> Option<&Open> { self.open.as_ref() }
    pub fn set_open(&mut self, open: Option<Open>) { self.open = open; }
    pub fn turns(&self) -> &[Turn] { &self.turns }

    /// Adds a turn, bounded: a conversation is not an archive.
    pub fn say(&mut self, kind: TurnKind, said: &str, at: f64, request_id: Option<&str>) -> &Turn {
        let id = format!("{}-t{}", self.conversation_id, self.next);
        self.next += 1;
        if self.turns.len() >= self.max_turns { self.turns.remove(0); }
        self.turns.push(Turn { id, kind, said: said.to_string(), at,
                               request_id: request_id.map(str::to_string), goal_id: None,
                               tool_execution_id: None, verification_id: None });
        self.turns.last().unwrap()
    }

    /// Tags the newest turn with the ids of the work it belongs to.
    pub fn tag_last(&mut self, goal_id: Option<&str>, tool_execution_id: Option<&str>, verification_id: Option<&str>) {
        if let Some(t) = self.turns.last_mut() {
            if goal_id.is_some() { t.goal_id = goal_id.map(str::to_string); }
            if tool_execution_id.is_some() { t.tool_execution_id = tool_execution_id.map(str::to_string); }
            if verification_id.is_some() { t.verification_id = verification_id.map(str::to_string); }
        }
    }

    /// If the newest turn said exactly `was`, it becomes `kind` saying `now`.
    /// Returns whether it did. Used when progress turns out to be the final,
    /// verified result, so KUE does not say the same sentence twice.
    pub fn promote_last(&mut self, was: &str, kind: TurnKind, now: &str) -> bool {
        match self.turns.last_mut() {
            Some(t) if t.said == was && !t.kind.by_owner() => { t.kind = kind; t.said = now.to_string(); true }
            _ => false,
        }
    }

    /// Kill and pause close whatever was open: nothing waits across either.
    pub fn close_everything(&mut self) { self.open = None; }
}

#[cfg(test)]
mod wish_tests {
    use super::*;

    #[test]
    fn a_standing_wish_about_cleaning_up_is_read_by_rule_or_not_at_all() {
        let out = |f| Some(StorageWish { facet: Facet::Storage(f), include: false });
        let inc = |f| Some(StorageWish { facet: Facet::Storage(f), include: true });
        assert_eq!(storage_wish("You don't want installers included in storage cleanup"), out(Category::Installer));
        assert_eq!(storage_wish("Never include installers when cleaning up"), out(Category::Installer));
        assert_eq!(storage_wish("Always exclude large files from storage clean-up"), out(Category::LargeFile));
        assert_eq!(storage_wish("Include installers in storage cleanup from now on"), inc(Category::Installer));

        // Not a wish about cleaning up: no kind named, or nothing about
        // cleaning, or no direction given.
        for said in ["You prefer PDF reports", "You work on the SAP project",
                     "You don't want to be disturbed in the morning", "Installers are big",
                     "You like cleaning up on Fridays"] {
            assert_eq!(storage_wish(said), None, "{said}");
        }
    }

    #[test]
    fn a_standing_wish_is_kept_in_the_owners_words_without_the_word_that_opened_it() {
        let ask = |t: &str| match interpret(t, None) {
            Interpretation::Memory(MemoryAsk::Keep { statement, replacing }) => (statement, replacing),
            other => panic!("{t:?} → {other:?}"),
        };
        assert_eq!(ask("Actually, include installers in storage cleanup from now on."),
                   ("include installers in storage cleanup from now on".to_string(), true));
        assert_eq!(ask("Always exclude large files from storage cleanup").1, true);
        // Without a standing word it is not a preference, and goes where it went before.
        assert!(matches!(interpret("include installers", None), Interpretation::NewRequest));
    }

    #[test]
    fn what_stands_from_now_on_is_told_apart_from_what_is_about_this_task() {
        for said in ["Include installers from now on", "Always leave the installers out",
                     "Never include duplicates in cleanup", "Use DOCX instead"] {
            assert!(is_standing(said), "{said}");
        }
        for said in ["Leave the installers", "Don't touch the duplicates", "Actually do all of them"] {
            assert!(!is_standing(said), "{said}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selection() -> Open {
        Open::Selection { request_id: "r1".into(), goal_id: "g1".into(),
            facets: vec![Facet::Storage(Category::Installer), Facet::Storage(Category::DuplicateCopy),
                         Facet::Storage(Category::OldDownload)], proposed: vec![] }
    }

    fn confirmation() -> Open { Open::Confirmation { request_id: "r2".into(), tool_execution_id: "x7".into() } }

    #[test]
    fn a_correction_revises_the_plan_it_refers_to() {
        let open = selection();
        for said in ["No, don't touch the installers.", "Computer, leave the installers alone",
                     "don't move the installers", "everything except the installers"] {
            assert_eq!(interpret(said, Some(&open)),
                Interpretation::Correction { request_id: "r1".into(),
                                             revision: Revision::Exclude(vec![Facet::Storage(Category::Installer)]) }, "{said}");
        }
        assert_eq!(interpret("only the duplicates", Some(&open)),
            Interpretation::Correction { request_id: "r1".into(),
                                         revision: Revision::OnlyInclude(Facet::Storage(Category::DuplicateCopy)) });
        // A file type is named only when the selection has one: "the dmgs" is
        // the disk images, and never quietly also "the installers".
        let with_types = Open::Selection { request_id: "r1".into(), goal_id: "g1".into(),
            facets: vec![Facet::Storage(Category::Installer), Facet::FileType(FileType::Dmg), Facet::FileType(FileType::Pdf)],
            proposed: vec![] };
        assert_eq!(interpret("skip the dmgs", Some(&with_types)),
            Interpretation::Correction { request_id: "r1".into(), revision: Revision::Exclude(vec![Facet::FileType(FileType::Dmg)]) });
        assert_eq!(interpret("Leave the PDF.", Some(&with_types)),
            Interpretation::Correction { request_id: "r1".into(), revision: Revision::Exclude(vec![Facet::FileType(FileType::Pdf)]) });
        assert!(matches!(interpret("skip the dmgs", Some(&open)), Interpretation::Clarify { .. }),
                "a type the selection does not contain is asked about");
    }

    #[test]
    fn going_back_and_including_everything_are_corrections_too() {
        let open = selection();
        assert_eq!(interpret("Actually do all of them.", Some(&open)),
            Interpretation::Correction { request_id: "r1".into(), revision: Revision::IncludeAll });
        assert_eq!(interpret("Go back.", Some(&open)),
            Interpretation::Correction { request_id: "r1".into(), revision: Revision::Undo });
        for said in ["nothing", "None.", "leave nothing out"] {
            assert_eq!(interpret(said, Some(&open)),
                Interpretation::Correction { request_id: "r1".into(), revision: Revision::LeaveNothingOut }, "{said}");
        }
        // With nothing open it is not a correction of anything.
        assert_eq!(interpret("nothing", None), Interpretation::NewRequest);
    }

    #[test]
    fn questions_about_the_work_are_recognised_whatever_is_open() {
        for open in [None, Some(selection()), Some(confirmation())] {
            for (said, q) in [("What are you doing?", WorkQuestion::WhatAreYouDoing), ("Why?", WorkQuestion::Why),
                              ("How many files?", WorkQuestion::HowMany), ("What's left?", WorkQuestion::WhatsLeft),
                              ("What did you just do?", WorkQuestion::WhatDidYouDo), ("Can you undo that?", WorkQuestion::CanYouUndo)] {
                assert_eq!(interpret(said, open.as_ref()), Interpretation::AboutWork { question: q }, "{said}");
            }
        }
        assert_eq!(interpret("Put them back.", None), Interpretation::UndoLast);
    }

    #[test]
    fn a_vague_change_to_a_waiting_action_is_asked_about_and_leaves_it_waiting() {
        let open = confirmation();
        for said in ["Don't send it yet.", "Use the other folder.", "No, not that one."] {
            assert!(matches!(interpret(said, Some(&open)), Interpretation::Clarify { .. }), "{said}");
        }
    }

    #[test]
    fn a_correction_kue_cannot_place_is_asked_about_not_guessed() {
        let open = selection();
        // Refuses something, names nothing KUE offered.
        assert!(matches!(interpret("no, not those", Some(&open)), Interpretation::Clarify { .. }));
        // Names two things at once while refusing: BOTH are left alone. No
        // question is needed, because leaving more of the owner's files alone is
        // the safe direction whatever they meant. (Before 2026-09-21 this asked;
        // asking is for when acting on a guess could touch the wrong file.)
        assert_eq!(interpret("don't touch the installers or the duplicates", Some(&open)),
            Interpretation::Correction { request_id: "r1".into(), revision: Revision::Exclude(vec![
                Facet::Storage(Category::Installer), Facet::Storage(Category::DuplicateCopy)]) });
        // But ONLY-including several things is narrowing to a guess: ask.
        assert!(matches!(interpret("only the installers and the duplicates", Some(&open)), Interpretation::Clarify { .. }));
        // Names something the selection does not contain: large files were not offered.
        assert!(matches!(interpret("don't touch the large files", Some(&open)), Interpretation::Clarify { .. }));
    }

    #[test]
    fn yes_means_the_open_thing_and_nothing_else() {
        let open = confirmation();
        for said in ["Do it.", "yes", "Computer, go ahead", "yes please"] {
            assert_eq!(interpret(said, Some(&open)), Interpretation::Confirmation { request_id: "r2".into() }, "{said}");
        }
        for said in ["stop", "no", "never mind", "Computer, cancel"] {
            assert_eq!(interpret(said, Some(&open)), Interpretation::Cancellation { request_id: "r2".into() }, "{said}");
        }
        // With nothing open, "do it" confirms nothing: there is nothing to do.
        assert_eq!(interpret("do it", None), Interpretation::NewRequest);
        assert_eq!(interpret("yes", None), Interpretation::NewRequest);
    }

    #[test]
    fn plain_no_is_a_cancellation_and_no_with_a_reason_is_a_correction() {
        let open = selection();
        assert_eq!(interpret("no", Some(&open)), Interpretation::Cancellation { request_id: "r1".into() });
        assert!(matches!(interpret("no, don't touch the installers", Some(&open)), Interpretation::Correction { .. }));
    }

    #[test]
    fn something_unrelated_is_a_new_request_even_while_something_is_open() {
        assert_eq!(interpret("open Safari", Some(&selection())), Interpretation::NewRequest);
        assert_eq!(interpret("what is 17 percent of 840", Some(&confirmation())), Interpretation::NewRequest);
    }

    #[test]
    fn a_conversation_is_bounded_and_every_turn_is_tagged() {
        let mut d = Dialogue::new("c1");
        for i in 0..100 { d.say(TurnKind::UserSpeech, "hi", i as f64, Some("r")); }
        assert_eq!(d.turns().len(), 60, "a conversation is not an archive");
        d.say(TurnKind::ToolResult, "Moved 14 files.", 200.0, Some("r9"));
        d.tag_last(Some("g1"), Some("x1"), Some("v1"));
        let last = d.turns().last().unwrap();
        assert_eq!((last.goal_id.as_deref(), last.tool_execution_id.as_deref(), last.verification_id.as_deref()),
                   (Some("g1"), Some("x1"), Some("v1")));
        assert!(TurnKind::UserCorrection.by_owner() && !TurnKind::ToolResult.by_owner());
    }
}
