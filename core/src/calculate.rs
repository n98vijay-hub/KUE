//! Arithmetic KUE does itself: "17% of 840", "what is 12 times 7".
//!
//! Deterministic, exact and local. No model, no app, nothing on the Mac is
//! touched. Numbers are held as exact fractions, so 17% of 840 is 714/5, not a
//! float that is nearly 142.8. Before the answer is said, the number KUE is
//! about to say is read back into a fraction and compared with the one it
//! computed: a formatting mistake cannot become a wrong answer said with
//! confidence. A result that fails that check is not said at all.
//!
//! Parsed by rule. Once the lead-in ("calculate", "what is") is gone, the rest
//! must be numbers and operators and nothing else — "what is the capital of
//! France" is not a calculation — and nothing here guesses.
//!
//! This is not the Calculator app. Typing into another app needs macOS
//! Accessibility control, which KUE does not have, so "open Calculator and add
//! 2 and 2" is still refused whole (`task::CANNOT_TYPE_INTO_APPS`).

/// Why a calculation has no answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalcError {
    DivideByZero,
    /// A number or an intermediate result does not fit in exact arithmetic.
    TooLarge,
    /// The square root of a negative number. KUE works in real numbers only.
    NoRealRoot,
}

/// One calculation, answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Calculation {
    /// What was calculated, in words KUE can say: "17 percent of 840".
    pub expression: String,
    /// The result as KUE says it: "142.8".
    pub result: String,
    /// False when the exact value has no finite decimal (1/3) and `result` is rounded.
    pub exact: bool,
    /// What was checked before the result could be said.
    pub verification: String,
}

impl Calculation {
    /// KUE's sentence.
    pub fn sentence(&self) -> String {
        if self.exact { format!("{} is {}.", capitalise(&self.expression), self.result) }
        else { format!("{} is about {}.", capitalise(&self.expression), self.result) }
    }
}

pub fn error_sentence(e: CalcError) -> &'static str {
    match e {
        CalcError::DivideByZero => "That divides by zero, which has no answer.",
        CalcError::TooLarge => "Those numbers are too large for me to work out exactly, so I won't give an answer.",
        CalcError::NoRealRoot => "A negative number has no square root among the numbers I work in, so I won't give an answer.",
    }
}

/// Arithmetic KUE does not do. The declared capability says what the
/// calculator is — sums, percentages, whole-number powers, square roots — and
/// says "no other functions". A request that names one of those other
/// functions is answered HERE, by rule, and never handed to a model: a model
/// asked for arithmetic either invents a number or denies that KUE can count
/// at all, and both were seen on this Mac.
///
/// Deliberately narrow: each phrase is arithmetic wording and nothing else, so
/// "log in", "sine" in a sentence about music, or a file called factorial.txt
/// go on their way.
pub fn outside_the_calculator(text: &str) -> Option<String> {
    // Punctuation is trimmed from the EDGES of words only, so "5 factorial."
    // is the word and "factorial.txt" is a file name.
    let words: Vec<&str> = text.split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric())).filter(|w| !w.is_empty()).collect();
    let t = format!(" {} ", words.join(" ").to_lowercase());
    let named = [
        ("cube root", "cube roots"), ("cube roots", "cube roots"), ("cubed root", "cube roots"),
        ("nth root", "roots other than square roots"), ("fourth root", "roots other than square roots"),
        ("fifth root", "roots other than square roots"), ("third root", "roots other than square roots"),
        ("logarithm", "logarithms"), ("logarithms", "logarithms"), ("log base", "logarithms"),
        ("natural log", "logarithms"), ("log of", "logarithms"),
        ("sine of", "trigonometry"), ("cosine of", "trigonometry"), ("tangent of", "trigonometry"),
        ("factorial", "factorials"), ("standard deviation", "statistics"),
    ].into_iter().find_map(|(phrase, what)| t.contains(&format!(" {phrase} ")).then_some(what))?;
    Some(format!("{} are outside what I work out myself. I do sums, percentages, whole-number powers and square roots, \
                  in exact fractions, and I check the answer before I say it — anything else I would only be guessing at.",
                 capitalise(named)))
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

// MARK: - Exact fractions

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Fraction { n: i128, d: i128 }

fn gcd(mut a: i128, mut b: i128) -> i128 {
    a = a.abs(); b = b.abs();
    while b != 0 { let t = a % b; a = b; b = t; }
    a.max(1)
}

impl Fraction {
    fn new(n: i128, d: i128) -> Result<Fraction, CalcError> {
        if d == 0 { return Err(CalcError::DivideByZero); }
        let g = gcd(n, d);
        let (n, d) = (n / g, d / g);
        Ok(if d < 0 { Fraction { n: n.checked_neg().ok_or(CalcError::TooLarge)?, d: -d } } else { Fraction { n, d } })
    }
    fn int(n: i128) -> Fraction { Fraction { n, d: 1 } }
    /// Compares two fractions exactly: a/b vs c/d by cross-multiplication.
    /// Both denominators are positive, so the direction is preserved.
    fn cmp(self, o: Fraction) -> Result<std::cmp::Ordering, CalcError> {
        let a = self.n.checked_mul(o.d).ok_or(CalcError::TooLarge)?;
        let b = o.n.checked_mul(self.d).ok_or(CalcError::TooLarge)?;
        Ok(a.cmp(&b))
    }
    fn add(self, o: Fraction) -> Result<Fraction, CalcError> {
        let a = self.n.checked_mul(o.d).ok_or(CalcError::TooLarge)?;
        let b = o.n.checked_mul(self.d).ok_or(CalcError::TooLarge)?;
        Fraction::new(a.checked_add(b).ok_or(CalcError::TooLarge)?, self.d.checked_mul(o.d).ok_or(CalcError::TooLarge)?)
    }
    fn neg(self) -> Result<Fraction, CalcError> { Ok(Fraction { n: self.n.checked_neg().ok_or(CalcError::TooLarge)?, d: self.d }) }
    fn sub(self, o: Fraction) -> Result<Fraction, CalcError> { self.add(o.neg()?) }
    fn mul(self, o: Fraction) -> Result<Fraction, CalcError> {
        Fraction::new(self.n.checked_mul(o.n).ok_or(CalcError::TooLarge)?, self.d.checked_mul(o.d).ok_or(CalcError::TooLarge)?)
    }
    /// Raised to a whole power, exactly. The exponent is bounded so a slip
    /// cannot ask for a number with more digits than KUE can hold.
    fn pow(self, e: i128) -> Result<Fraction, CalcError> {
        if !(-64..=64).contains(&e) { return Err(CalcError::TooLarge); }
        let (mut n, d) = (Fraction::int(1), self);
        let mut k = e.abs();
        while k > 0 {
            n = n.mul(d)?;
            k -= 1;
            if k > 0 && (n.n.abs() > i128::MAX / 1_000_000 || n.d > i128::MAX / 1_000_000) { return Err(CalcError::TooLarge); }
        }
        if e < 0 { Fraction::int(1).div(n) } else { Ok(n) }
    }

    fn div(self, o: Fraction) -> Result<Fraction, CalcError> {
        if o.n == 0 { return Err(CalcError::DivideByZero); }
        Fraction::new(self.n.checked_mul(o.d).ok_or(CalcError::TooLarge)?, self.d.checked_mul(o.n).ok_or(CalcError::TooLarge)?)
    }
}

/// The largest integer whose square is at most `v`. Exact, by bisection.
fn isqrt(v: u128) -> u128 {
    if v < 2 { return v; }
    let (mut lo, mut hi) = (1u128, (v / 2) + 1);
    while lo < hi {
        let mid = lo + (hi - lo + 1) / 2;
        match mid.checked_mul(mid) {
            Some(sq) if sq <= v => lo = mid,
            _ => hi = mid - 1,
        }
    }
    lo
}

/// How many decimal places a root is worked out to before it is used.
const ROOT_PLACES: u32 = 9;

/// The square root of a fraction: exact when it is one, and otherwise the
/// value bracketed between two fractions that differ in the ninth decimal —
/// each of which KUE squares to prove the digits it will say.
///
/// Returns the value, and a sentence about how it was established when the
/// root is not exact.
fn root(x: Fraction) -> Result<(Fraction, Option<String>), CalcError> {
    if x.n < 0 { return Err(CalcError::NoRealRoot); }
    if x.n == 0 { return Ok((Fraction::int(0), None)); }
    let scale = 10i128.checked_pow(ROOT_PLACES).ok_or(CalcError::TooLarge)?;
    // n/d scaled by 10^(2 places), so the integer root carries that many decimals.
    let scaled = x.n.checked_mul(scale).ok_or(CalcError::TooLarge)?
        .checked_mul(scale).ok_or(CalcError::TooLarge)?
        .checked_div(x.d).ok_or(CalcError::TooLarge)?;
    let s = isqrt(scaled as u128) as i128;
    let lo = Fraction::new(s, scale)?;
    // Exact when the bracket's lower end squares back to the number itself.
    if lo.mul(lo)? == x { return Ok((lo, None)); }
    let hi = Fraction::new(s + 1, scale)?;
    // Proof of the digits: squaring each end brackets the number.
    if lo.mul(lo)?.cmp(x)? == std::cmp::Ordering::Greater || hi.mul(hi)?.cmp(x)? == std::cmp::Ordering::Less {
        return Err(CalcError::TooLarge);
    }
    let (lo_s, hi_s) = (decimal(lo).ok_or(CalcError::TooLarge)?.0, decimal(hi).ok_or(CalcError::TooLarge)?.0);
    let about = format!("the root lies between {lo_s} and {hi_s} — each squared brackets the number");
    // The nearer end, so the decimal KUE says is the closest at this width.
    let mid_is_hi = hi.mul(hi)?.sub(x)?.cmp(x.sub(lo.mul(lo)?)?)? == std::cmp::Ordering::Less;
    Ok((if mid_is_hi { hi } else { lo }, Some(about)))
}

/// "1,250.75" → 125075/100. Digits, at most one point, commas only between digits.
fn parse_number(text: &str) -> Option<Result<Fraction, CalcError>> {
    let plain: String = text.chars().filter(|c| *c != ',').collect();
    let (whole, frac) = plain.split_once('.').unwrap_or((&plain, ""));
    if whole.is_empty() && frac.is_empty() { return None; }
    if !whole.chars().all(|c| c.is_ascii_digit()) || !frac.chars().all(|c| c.is_ascii_digit()) { return None; }
    if whole.len() + frac.len() > 30 { return Some(Err(CalcError::TooLarge)); }
    let digits = format!("{whole}{frac}");
    let n: i128 = match digits.parse() { Ok(n) => n, Err(_) => return Some(Err(CalcError::TooLarge)) };
    let d = 10i128.checked_pow(frac.len() as u32)?;
    Some(Fraction::new(n, d))
}

// MARK: - Words to tokens

#[derive(Debug, Clone, PartialEq)]
enum Token { Num(Fraction, String), Plus, Minus, Times, Over, PercentOf, Percent, Open, Close, Power, Root }

const LEAD_INS: [&str; 22] = ["computer, ", "computer ", "kue, ", "kue ", "hey ", "ok ", "please ", "can you ", "could you ",
    "calculate ", "compute ", "work out ", "figure out ", "solve ", "what is ", "what's ", "whats ", "how much is ",
    "tell me ", "what are ", "what does ", "what do "];
const TAILS: [&str; 6] = [" please", " equals", " equal", " is equal to", " =", " for me"];

fn strip(text: &str) -> String {
    let mut t = text.trim().to_lowercase().trim_end_matches(['?', '.', '!']).trim().to_string();
    loop {
        let before = t.clone();
        for p in LEAD_INS { if let Some(r) = t.strip_prefix(p) { t = r.trim_start().to_string(); } }
        for s in TAILS { if let Some(r) = t.strip_suffix(s) { t = r.trim_end().to_string(); } }
        if t == before { return t; }
    }
}

/// "add 2 and 3", "subtract 3 from 10", "multiply 6 by 7", "divide 10 by 4" → an expression.
fn verb_form(t: &str) -> Option<String> {
    let (verb, rest) = t.split_once(' ')?;
    let pair = |sep: &str| rest.split_once(sep).map(|(a, b)| (a.trim().to_string(), b.trim().to_string()));
    Some(match verb {
        "add" => { let (a, b) = pair(" and ").or_else(|| pair(" to "))?; format!("{a} plus {b}") }
        "subtract" => { let (a, b) = pair(" from ")?; format!("{b} minus {a}") }
        "multiply" => { let (a, b) = pair(" by ").or_else(|| pair(" and "))?; format!("{a} times {b}") }
        "divide" => { let (a, b) = pair(" by ")?; format!("{a} divided by {b}") }
        _ => return None,
    })
}

fn tokens(t: &str) -> Option<Result<Vec<Token>, CalcError>> {
    // Multi-word operators first, with spaces kept around them.
    let spaced = format!(" {t} ")
        .replace(" multiplied by ", " * ").replace(" divided by ", " / ")
        .replace(" percent of ", " %of ").replace("% of ", " %of ").replace(" per cent of ", " %of ")
        .replace(" percent ", " % ").replace(" per cent ", " % ")
        .replace(" plus ", " + ").replace(" minus ", " - ").replace(" times ", " * ").replace(" x ", " * ")
        .replace(" over ", " / ").replace('×', " * ").replace('÷', " / ")
        // Roots and powers, in the ways they are said and written.
        .replace(" the square root of ", " √ ").replace(" square root of ", " √ ").replace(" square root ", " √ ")
        .replace(" the root of ", " √ ").replace(" root of ", " √ ").replace(" sqrt ", " √ ").replace("sqrt(", "√(")
        .replace(" to the power of ", " ^ ").replace(" raised to ", " ^ ").replace(" squared ", " ^ 2 ").replace(" cubed ", " ^ 3 ");
    let chars: Vec<char> = spaced.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() { i += 1; continue; }
        if c.is_ascii_digit() || c == '.' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.'
                || (chars[i] == ',' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit() && i > start)) { i += 1; }
            let text: String = chars[start..i].iter().collect();
            match parse_number(&text)? {
                Ok(f) => out.push(Token::Num(f, text)),
                Err(e) => return Some(Err(e)),
            }
            continue;
        }
        let tok = match c {
            '+' => Token::Plus, '-' | '−' => Token::Minus, '*' => Token::Times, '/' => Token::Over,
            '(' => Token::Open, ')' => Token::Close, '^' => Token::Power, '√' => Token::Root,
            '%' => {
                if chars[i + 1..].iter().collect::<String>().starts_with("of") { i += 2; Token::PercentOf } else { Token::Percent }
            }
            // Any other character — a letter, a word — means this is not arithmetic.
            _ => return None,
        };
        out.push(tok);
        i += 1;
    }
    Some(Ok(out))
}

// MARK: - Evaluation

struct Parser {
    toks: Vec<Token>,
    at: usize,
    /// How any root in the expression was established, when it was not exact.
    /// Collected as the expression is worked out, and said with the answer.
    approximated: Vec<String>,
}

impl Parser {
    fn peek(&self) -> Option<&Token> { self.toks.get(self.at) }
    fn expr(&mut self) -> Option<Result<Fraction, CalcError>> {
        let mut left = match self.term()? { Ok(v) => v, e => return Some(e) };
        while let Some(t) = self.peek().cloned() {
            let op = match t { Token::Plus => Fraction::add, Token::Minus => Fraction::sub, _ => break };
            self.at += 1;
            let right = match self.term()? { Ok(v) => v, e => return Some(e) };
            left = match op(left, right) { Ok(v) => v, Err(e) => return Some(Err(e)) };
        }
        Some(Ok(left))
    }
    fn term(&mut self) -> Option<Result<Fraction, CalcError>> {
        let mut left = match self.power()? { Ok(v) => v, e => return Some(e) };
        while let Some(t) = self.peek().cloned() {
            let op: fn(Fraction, Fraction) -> Result<Fraction, CalcError> = match t {
                Token::Times => Fraction::mul,
                Token::Over => Fraction::div,
                Token::PercentOf => |a, b| a.mul(b)?.div(Fraction::int(100)),
                _ => break,
            };
            self.at += 1;
            let right = match self.power()? { Ok(v) => v, e => return Some(e) };
            left = match op(left, right) { Ok(v) => v, Err(e) => return Some(Err(e)) };
        }
        Some(Ok(left))
    }
    /// `2 ^ 10`, right to left, with a whole-number exponent only.
    fn power(&mut self) -> Option<Result<Fraction, CalcError>> {
        let base = match self.factor()? { Ok(v) => v, e => return Some(e) };
        if self.peek() != Some(&Token::Power) { return Some(Ok(base)); }
        self.at += 1;
        let exponent = match self.power()? { Ok(v) => v, e => return Some(e) };
        if exponent.d != 1 { return None; }
        Some(base.pow(exponent.n))
    }
    fn factor(&mut self) -> Option<Result<Fraction, CalcError>> {
        let value = match self.peek()?.clone() {
            Token::Minus => { self.at += 1; return Some(self.factor()?.and_then(Fraction::neg)); }
            Token::Root => {
                self.at += 1;
                let inner = match self.factor()? { Ok(v) => v, e => return Some(e) };
                match root(inner) {
                    Ok((v, note)) => { if let Some(n) = note { self.approximated.push(n); } v }
                    Err(e) => return Some(Err(e)),
                }
            }
            Token::Num(f, _) => { self.at += 1; f }
            Token::Open => {
                self.at += 1;
                let v = match self.expr()? { Ok(v) => v, e => return Some(e) };
                if self.peek() != Some(&Token::Close) { return None; }
                self.at += 1;
                v
            }
            _ => return None,
        };
        if self.peek() == Some(&Token::Percent) {
            self.at += 1;
            return Some(value.div(Fraction::int(100)));
        }
        Some(Ok(value))
    }
}

// MARK: - Saying the result

/// The decimal KUE says, and whether it is the exact value.
fn decimal(f: Fraction) -> Option<(String, bool)> {
    let negative = f.n < 0;
    let n = f.n.unsigned_abs();
    let d = f.d as u128;
    // Exact when the denominator divides a power of ten small enough to print.
    let exact_places = (0..=12u32).find(|k| 10u128.pow(*k) % d == 0);
    let (places, scaled, exact) = match exact_places {
        Some(k) => (k, n.checked_mul(10u128.pow(k) / d)?, true),
        None => (6, n.checked_mul(2_000_000)?.checked_add(d)? / d.checked_mul(2)?, false),
    };
    let unit = 10u128.pow(places);
    let whole = group(scaled / unit);
    let frac = if places == 0 { String::new() } else {
        let s = format!("{:0width$}", scaled % unit, width = places as usize);
        s.trim_end_matches('0').to_string()
    };
    let body = if frac.is_empty() { whole } else { format!("{whole}.{frac}") };
    let zero = body.chars().all(|c| c == '0' || c == '.' || c == ',');
    Some((if negative && !zero { format!("-{body}") } else { body }, exact))
}

/// 1234567 → "1,234,567"; below 10,000 no separator.
fn group(v: u128) -> String {
    let s = v.to_string();
    if v < 10_000 { return s; }
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 { out.push(','); }
        out.push(c);
    }
    out
}

fn spoken(toks: &[Token]) -> String {
    toks.iter().map(|t| match t {
        Token::Num(_, text) => text.clone(),
        Token::Plus => "plus".into(), Token::Minus => "minus".into(), Token::Times => "times".into(),
        Token::Over => "divided by".into(), Token::PercentOf => "percent of".into(), Token::Percent => "percent".into(),
        Token::Open => "(".into(), Token::Close => ")".into(),
        Token::Power => "to the power of".into(), Token::Root => "the square root of".into(),
    }).collect::<Vec<_>>().join(" ").replace("( ", "(").replace(" )", ")")
}

/// A calculation, or None when the sentence is not one.
pub fn parse(text: &str) -> Option<Result<Calculation, CalcError>> {
    let stripped = strip(text);
    let expression = verb_form(&stripped).unwrap_or(stripped);
    let toks = match tokens(&expression)? { Ok(t) => t, Err(e) => return Some(Err(e)) };
    // A number alone is not a calculation ("what is 42").
    if !toks.iter().any(|t| !matches!(t, Token::Num(..) | Token::Open | Token::Close)) { return None; }
    if !toks.iter().any(|t| matches!(t, Token::Num(..))) { return None; }
    let mut p = Parser { toks: toks.clone(), at: 0, approximated: Vec::new() };
    let value = match p.expr()? { Ok(v) => v, Err(e) => return Some(Err(e)) };
    if p.at != p.toks.len() { return None; }

    let Some((result, exact)) = decimal(value) else { return Some(Err(CalcError::TooLarge)) };
    // Read back what will be said, and compare it with what was computed.
    let back = match parse_number(result.trim_start_matches('-')) {
        Some(Ok(b)) => if result.starts_with('-') { b.neg().ok()? } else { b },
        _ => return Some(Err(CalcError::TooLarge)),
    };
    let difference = match back.sub(value) { Ok(d) => d, Err(e) => return Some(Err(e)) };
    let holds = if exact { difference.n == 0 }
        // Rounded to six places: within half a millionth.
        else { difference.n.unsigned_abs().checked_mul(2_000_000).is_some_and(|x| x <= difference.d as u128) };
    if !holds { return None; }
    let mut verification = if exact {
        format!("exact fraction {}/{}; the stated {result} reads back as the same fraction", value.n, value.d)
    } else {
        format!("exact fraction {}/{}; the stated {result} reads back within half a millionth of it", value.n, value.d)
    };
    // A root that was not exact says how its digits were established, and the
    // answer is said as "about" even where the arithmetic after it was exact.
    let rooted = !p.approximated.is_empty();
    if rooted { verification = format!("{}; {}", p.approximated.join("; "), verification); }
    let exact = exact && !rooted;
    Some(Ok(Calculation { expression: spoken(&toks), result, exact, verification }))
}

#[cfg(test)]
mod tests_outside {
    use super::outside_the_calculator as outside;

    #[test]
    fn arithmetic_kue_does_not_do_is_said_plainly_and_never_sent_to_a_model() {
        for said in ["What's the cube root of 27?", "cube root of 27", "the logarithm of 100",
                     "what is the natural log of 8", "sine of 30 degrees", "5 factorial",
                     "the standard deviation of these numbers"] {
            let reply = outside(said).unwrap_or_else(|| panic!("{said} fell through to a model"));
            // It says what it cannot do AND what it can, and claims nothing else.
            assert!(reply.contains("outside what I work out myself"), "{said}: {reply}");
            assert!(reply.contains("square roots"), "{said}: {reply}");
            assert!(!reply.to_lowercase().contains("can't calculate"), "{said}: {reply}");
        }
        // What the calculator DOES do is not caught here: it is worked out.
        for said in ["the square root of 1159", "2 to the power of 10", "17% of 840", "75 / 5", "seven squared"] {
            assert!(outside(said).is_none(), "{said} was refused although KUE does it");
        }
        // Ordinary sentences that merely contain the letters are left alone.
        for said in ["log in to the site", "open my sine wave notes", "where is factorial.txt"] {
            assert!(outside(said).is_none(), "{said}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(text: &str) -> String {
        match parse(text) {
            Some(Ok(c)) => c.sentence(),
            Some(Err(e)) => error_sentence(e).to_string(),
            None => "NOT A CALCULATION".into(),
        }
    }

    /// The owner's own session: "75÷5" was answered, and then "the root of
    /// 1159" was not — it fell past the calculator to the model, which denied
    /// the capability. Roots and powers are part of the contract now.
    #[test]
    fn roots_and_powers_are_worked_out_here() {
        let c = parse("75 / 5").unwrap().unwrap();
        assert_eq!((c.result.as_str(), c.exact), ("15", true));

        // Not exact: said as "about", with the bracket that proves the digits.
        let c = parse("What's the root of 1159?").unwrap().unwrap();
        assert_eq!(c.result, "34.044089061", "nine decimals, every one of them proved");
        assert!(!c.exact);
        assert_eq!(c.sentence(), "The square root of 1159 is about 34.044089061.");
        assert!(c.verification.contains("the root lies between 34.044089061 and 34.044089062"), "{}", c.verification);
        assert!(c.verification.contains("each squared brackets the number"), "{}", c.verification);

        for (said, result) in [("sqrt(1000)", "31.622776602"), ("square root of 2", "1.414213562"),
                               ("the square root of 1159", "34.044089061")] {
            let c = parse(said).unwrap().unwrap();
            assert_eq!(c.result, result, "{said}");
            assert!(!c.exact, "{said}");
        }
        // A perfect square is exact, and says so.
        for (said, result) in [("sqrt(1024)", "32"), ("the square root of 6.25", "2.5"), ("sqrt(0)", "0")] {
            let c = parse(said).unwrap().unwrap();
            assert_eq!((c.result.as_str(), c.exact), (result, true), "{said}");
        }

        // Powers, exactly.
        for (said, result) in [("2^10", "1024"), ("2 to the power of 10", "1024"), ("7 squared", "49"),
                               ("3 cubed", "27"), ("2^-2", "0.25"), ("(75 / 5) + 10", "25")] {
            let c = parse(said).unwrap().unwrap();
            assert_eq!((c.result.as_str(), c.exact), (result, true), "{said}");
        }
        // Mixed: a root inside exact arithmetic is still said as "about".
        let c = parse("sqrt(2) * 2").unwrap().unwrap();
        assert!(!c.exact && c.result.starts_with("2.828427124"), "{c:?}");
    }

    /// The digits KUE says for a root are not taken on trust: the sentence
    /// names two fractions, and squaring them must bracket the number.
    #[test]
    fn a_root_is_bracketed_by_what_the_verification_names() {
        for x in [2u64, 3, 5, 7, 10, 1000, 1159, 12_345, 999_983] {
            let c = parse(&format!("sqrt({x})")).unwrap().unwrap();
            let between = c.verification.split("the root lies between ").nth(1)
                .unwrap_or_else(|| panic!("{x}: {}", c.verification));
            let (lo, rest) = between.split_once(" and ").unwrap();
            let hi = rest.split(' ').next().unwrap();
            let (lo, hi): (f64, f64) = (lo.replace(',', "").parse().unwrap(), hi.replace(',', "").parse().unwrap());
            assert!(lo * lo <= x as f64, "{x}: {lo}² is above it");
            assert!(hi * hi >= x as f64, "{x}: {hi}² is below it");
            // And what KUE says is one of the two ends it proved.
            let said: f64 = c.result.replace(',', "").parse().unwrap();
            assert!(said == lo || said == hi, "{x}: said {said}, proved {lo}/{hi}");
        }
    }

    #[test]
    fn what_arithmetic_refuses() {
        // No real answer, rather than a wrong one.
        assert!(matches!(parse("sqrt(-4)"), Some(Err(CalcError::NoRealRoot))));
        assert!(matches!(parse("the square root of -1"), Some(Err(CalcError::NoRealRoot))));
        assert_eq!(said("sqrt(-4)"), "A negative number has no square root among the numbers I work in, so I won't give an answer.");
        // Division by zero, including inside a root.
        assert!(matches!(parse("75 / 0"), Some(Err(CalcError::DivideByZero))));
        assert!(matches!(parse("sqrt(4/0)"), Some(Err(CalcError::DivideByZero))));
        // Too large to hold exactly, rather than an approximation nobody asked for.
        assert!(matches!(parse("2^200"), Some(Err(CalcError::TooLarge))));
        assert!(matches!(parse("99999999999999999999^9"), Some(Err(CalcError::TooLarge))));
        // A fractional exponent is not arithmetic KUE does.
        assert_eq!(said("2^0.5"), "NOT A CALCULATION");
        // Still not a calculation: words it does not know, and a bare number.
        for not in ["what is the meaning of 42", "root beer", "what is 42", "sqrt", "2 ^", "√"] {
            assert_eq!(said(not), "NOT A CALCULATION", "{not}");
        }
    }

    #[test]
    fn the_briefs_example_in_the_ways_it_is_said() {
        for s in ["17% of 840", "Calculate 17 percent of 840.", "calculate 17% of 840", "Computer, calculate 17 percent of 840.",
                  "what is 17% of 840?", "What's 17 percent of 840"] {
            let c = parse(s).unwrap().unwrap();
            assert_eq!(c.result, "142.8", "{s}");
            assert!(c.exact, "{s}");
        }
        assert_eq!(said("calculate 17 percent of 840"), "17 percent of 840 is 142.8.");
    }

    #[test]
    fn operators_precedence_and_parentheses() {
        assert_eq!(said("2 + 2 * 3"), "2 plus 2 times 3 is 8.");
        assert_eq!(said("(2 + 2) * 3"), "(2 plus 2) times 3 is 12.");
        assert_eq!(said("what is 12 times 7"), "12 times 7 is 84.");
        assert_eq!(said("10 divided by 4"), "10 divided by 4 is 2.5.");
        assert_eq!(said("-5 + 3"), "Minus 5 plus 3 is -2.");
        assert_eq!(said("20% of 50 + 10"), "20 percent of 50 plus 10 is 20.");
        assert_eq!(said("1,250 * 4"), "1,250 times 4 is 5000.");
        assert_eq!(said("123456 x 10"), "123456 times 10 is 1,234,560.");
        assert_eq!(said("0.1 + 0.2"), "0.1 plus 0.2 is 0.3.", "exact, not 0.30000000000000004");
        assert_eq!(said("add 2 and 2"), "2 plus 2 is 4.");
        assert_eq!(said("subtract 3 from 10"), "10 minus 3 is 7.");
        assert_eq!(said("multiply 6 by 7"), "6 times 7 is 42.");
        assert_eq!(said("divide 10 by 4"), "10 divided by 4 is 2.5.");
    }

    #[test]
    fn a_repeating_decimal_is_said_as_about_never_as_exact() {
        let c = parse("1 / 3").unwrap().unwrap();
        assert!(!c.exact);
        assert_eq!(c.sentence(), "1 divided by 3 is about 0.333333.");
        assert_eq!(said("2/3"), "2 divided by 3 is about 0.666667.");
    }

    #[test]
    fn no_answer_is_invented_for_what_has_none() {
        assert_eq!(parse("5 / 0"), Some(Err(CalcError::DivideByZero)));
        assert_eq!(parse("5 / (2 - 2)"), Some(Err(CalcError::DivideByZero)));
        assert_eq!(parse("99999999999999999999 * 99999999999999999999 * 99999999999999999999"), Some(Err(CalcError::TooLarge)));
        // Fits as a fraction, but not once it is written out as a decimal.
        assert_eq!(parse("99999999999999999999999999999999999 / 3"), Some(Err(CalcError::TooLarge)));
    }

    #[test]
    fn sentences_that_are_not_arithmetic_are_left_alone() {
        for s in ["what is the capital of France", "what is 42", "open 2 apps", "calculate my taxes", "what is level 4",
                  "open Calculator", "how much storage do I have", "2 +", "what time is it", "percent", "(", "add 2 apples and 3"] {
            assert_eq!(parse(s), None, "{s}");
        }
    }

    #[test]
    fn the_verification_is_a_real_check_of_what_is_said() {
        let c = parse("17% of 840").unwrap().unwrap();
        assert_eq!(c.verification, "exact fraction 714/5; the stated 142.8 reads back as the same fraction");
        // The formatter and the read-back agree on a range of values, including
        // negatives, grouping and rounding.
        for (s, r) in [("1000000 / 8", "125,000"), ("-7 / 4", "-1.75"), ("22 / 7", "3.142857"), ("0 - 0.5", "-0.5")] {
            assert_eq!(parse(s).unwrap().unwrap().result, r, "{s}");
        }
    }
}
