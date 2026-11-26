//! Routing graph: which sources are mixed into which outputs, and the rules
//! that make echo and feedback impossible.
//!
//! Two independent rules are enforced on every change:
//!
//! 1. **Self-hearing** – a route from a source to an output is refused when the
//!    output is heard by one of the parties whose sound is in the source
//!    (e.g. `Claude → Til Claude`, `Discord → Til Discord`). These routes are
//!    hard-locked; they can only be unlocked explicitly in advanced mode.
//! 2. **Feedback loops** – an output can be *contained in* a source (the source
//!    records the device the output plays into, or loops that device back).
//!    Together with the routes this forms a directed graph; any cycle would
//!    re-circulate audio forever (howling). A route that would close a cycle
//!    is always refused, also in advanced mode.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Something that produces or hears audio.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Party {
    /// The person using the computer (their microphone).
    User,
    /// Claude (the Claude window).
    Claude,
    /// Discord and everyone in the call.
    Discord,
    /// Any other program, by executable name (lower case).
    App(String),
    /// Everything else that plays on the computer.
    Other,
}

impl fmt::Display for Party {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Party::User => write!(f, "user"),
            Party::Claude => write!(f, "claude"),
            Party::Discord => write!(f, "discord"),
            Party::App(name) => write!(f, "app:{name}"),
            Party::Other => write!(f, "other"),
        }
    }
}

pub type SourceId = u32;
pub type OutputId = u32;

#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub id: SourceId,
    pub label: String,
    /// Whose sound this source carries.
    pub parties: BTreeSet<Party>,
    /// Outputs whose signal ends up inside this source (because the source
    /// captures or loops back the device that output plays into).
    pub contains_outputs: BTreeSet<OutputId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Output {
    pub id: OutputId,
    pub label: String,
    /// Who hears this output.
    pub heard_by: BTreeSet<Party>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Route {
    /// Gain in dB. `f32::NEG_INFINITY` is silence.
    pub gain_db: f32,
    pub muted: bool,
}

impl Default for Route {
    fn default() -> Self {
        Route { gain_db: 0.0, muted: false }
    }
}

/// Why a change was refused. `key()` is a stable identifier for the UI's
/// translation table; `Display` is an English explanation for logs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rejection {
    UnknownSource(SourceId),
    UnknownOutput(OutputId),
    /// The party would hear itself.
    SelfHearing { party: Party, locked: bool },
    /// The route would close a loop; the path is listed by labels.
    Feedback { path: Vec<String> },
}

impl Rejection {
    pub fn key(&self) -> &'static str {
        match self {
            Rejection::UnknownSource(_) | Rejection::UnknownOutput(_) => "route.unknown",
            Rejection::SelfHearing { locked: true, .. } => "route.selfHearingLocked",
            Rejection::SelfHearing { locked: false, .. } => "route.selfHearing",
            Rejection::Feedback { .. } => "route.feedback",
        }
    }
}

impl fmt::Display for Rejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Rejection::UnknownSource(id) => write!(f, "unknown source {id}"),
            Rejection::UnknownOutput(id) => write!(f, "unknown output {id}"),
            Rejection::SelfHearing { party, locked } => write!(
                f,
                "{party} would hear itself{}",
                if *locked { " (locked; unlock in advanced mode)" } else { "" }
            ),
            Rejection::Feedback { path } => write!(f, "feedback loop: {}", path.join(" -> ")),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RoutingGraph {
    sources: BTreeMap<SourceId, Source>,
    outputs: BTreeMap<OutputId, Output>,
    routes: BTreeMap<(SourceId, OutputId), Route>,
    /// Advanced mode: self-hearing routes may be created (with a warning in
    /// the UI). Feedback loops stay forbidden regardless.
    self_hearing_unlocked: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Node {
    S(SourceId),
    O(OutputId),
}

impl RoutingGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn sources(&self) -> impl Iterator<Item = &Source> {
        self.sources.values()
    }
    pub fn outputs(&self) -> impl Iterator<Item = &Output> {
        self.outputs.values()
    }
    pub fn routes(&self) -> impl Iterator<Item = (SourceId, OutputId, Route)> + '_ {
        self.routes.iter().map(|(&(s, o), &r)| (s, o, r))
    }
    pub fn route(&self, s: SourceId, o: OutputId) -> Option<Route> {
        self.routes.get(&(s, o)).copied()
    }
    pub fn source(&self, id: SourceId) -> Option<&Source> {
        self.sources.get(&id)
    }
    pub fn output(&self, id: OutputId) -> Option<&Output> {
        self.outputs.get(&id)
    }
    pub fn self_hearing_unlocked(&self) -> bool {
        self.self_hearing_unlocked
    }

    /// Unlocking only changes what *future* changes may do. Locking again
    /// removes any self-hearing routes that exist.
    pub fn set_self_hearing_unlocked(&mut self, unlocked: bool) {
        self.self_hearing_unlocked = unlocked;
        if !unlocked {
            let bad: Vec<_> = self
                .routes
                .keys()
                .copied()
                .filter(|&(s, o)| self.self_hearing_party(s, o).is_some())
                .collect();
            for k in bad {
                self.routes.remove(&k);
            }
        }
    }

    pub fn add_output(&mut self, output: Output) {
        self.outputs.insert(output.id, output);
    }

    /// Adds or replaces a source. Refused if what it contains (outputs it
    /// records) would close a loop with existing routes, or if an existing
    /// route from it would now be self-hearing while locked.
    pub fn add_source(&mut self, source: Source) -> Result<(), Rejection> {
        for &o in &source.contains_outputs {
            if !self.outputs.contains_key(&o) {
                return Err(Rejection::UnknownOutput(o));
            }
        }
        let mut next = self.clone();
        next.sources.insert(source.id, source.clone());
        let ids: Vec<_> = next.routes.keys().copied().filter(|&(s, _)| s == source.id).collect();
        for (s, o) in ids {
            next.check_self_hearing(s, o)?;
        }
        if let Some(path) = next.find_cycle() {
            return Err(Rejection::Feedback { path });
        }
        *self = next;
        Ok(())
    }

    pub fn remove_source(&mut self, id: SourceId) {
        self.sources.remove(&id);
        self.routes.retain(|&(s, _), _| s != id);
    }

    pub fn remove_output(&mut self, id: OutputId) {
        self.outputs.remove(&id);
        self.routes.retain(|&(_, o), _| o != id);
        for s in self.sources.values_mut() {
            s.contains_outputs.remove(&id);
        }
    }

    fn self_hearing_party(&self, s: SourceId, o: OutputId) -> Option<Party> {
        let src = self.sources.get(&s)?;
        let out = self.outputs.get(&o)?;
        src.parties.intersection(&out.heard_by).next().cloned()
    }

    fn check_self_hearing(&self, s: SourceId, o: OutputId) -> Result<(), Rejection> {
        match self.self_hearing_party(s, o) {
            Some(party) if !self.self_hearing_unlocked => Err(Rejection::SelfHearing { party, locked: true }),
            _ => Ok(()),
        }
    }

    /// Checks a route without changing anything. `Ok(Some(party))` means the
    /// route is allowed only because self-hearing is unlocked; the UI shows a
    /// warning in that case.
    pub fn check_route(&self, s: SourceId, o: OutputId) -> Result<Option<Party>, Rejection> {
        if !self.sources.contains_key(&s) {
            return Err(Rejection::UnknownSource(s));
        }
        if !self.outputs.contains_key(&o) {
            return Err(Rejection::UnknownOutput(o));
        }
        self.check_self_hearing(s, o)?;
        if !self.routes.contains_key(&(s, o)) {
            if let Some(path) = self.path(Node::O(o), Node::S(s)) {
                let mut labels: Vec<String> = vec![self.label(Node::S(s))];
                labels.extend(path.into_iter().map(|n| self.label(n)));
                return Err(Rejection::Feedback { path: labels });
            }
        }
        Ok(self.self_hearing_party(s, o))
    }

    pub fn set_route(&mut self, s: SourceId, o: OutputId, route: Route) -> Result<Option<Party>, Rejection> {
        let warn = self.check_route(s, o)?;
        self.routes.insert((s, o), route);
        Ok(warn)
    }

    pub fn remove_route(&mut self, s: SourceId, o: OutputId) {
        self.routes.remove(&(s, o));
    }

    fn label(&self, n: Node) -> String {
        match n {
            Node::S(id) => self.sources.get(&id).map(|s| s.label.clone()).unwrap_or_default(),
            Node::O(id) => self.outputs.get(&id).map(|o| o.label.clone()).unwrap_or_default(),
        }
    }

    fn successors(&self, n: Node) -> Vec<Node> {
        match n {
            Node::S(s) => self.routes.keys().filter(|(rs, _)| *rs == s).map(|&(_, o)| Node::O(o)).collect(),
            Node::O(o) => self
                .sources
                .values()
                .filter(|src| src.contains_outputs.contains(&o))
                .map(|src| Node::S(src.id))
                .collect(),
        }
    }

    /// Path from `from` to `to` (inclusive of both), breadth-first.
    fn path(&self, from: Node, to: Node) -> Option<Vec<Node>> {
        let mut prev: BTreeMap<Node, Node> = BTreeMap::new();
        let mut queue = std::collections::VecDeque::from([from]);
        let mut seen = BTreeSet::from([from]);
        while let Some(n) = queue.pop_front() {
            if n == to {
                let mut path = vec![n];
                let mut cur = n;
                while let Some(&p) = prev.get(&cur) {
                    path.push(p);
                    cur = p;
                }
                path.reverse();
                return Some(path);
            }
            for m in self.successors(n) {
                if seen.insert(m) {
                    prev.insert(m, n);
                    queue.push_back(m);
                }
            }
        }
        None
    }

    /// Returns the labels along a cycle if the graph has one.
    pub fn find_cycle_labels(&self) -> Option<Vec<String>> {
        self.find_cycle()
    }

    fn find_cycle(&self) -> Option<Vec<String>> {
        for &s in self.sources.keys() {
            for m in self.successors(Node::S(s)) {
                if let Some(p) = self.path(m, Node::S(s)) {
                    let mut labels = vec![self.label(Node::S(s))];
                    labels.extend(p.into_iter().map(|n| self.label(n)));
                    return Some(labels);
                }
            }
        }
        None
    }

    /// True if no rule is broken. Used by tests and as a debug assertion.
    pub fn is_valid(&self) -> bool {
        if self.find_cycle().is_some() {
            return false;
        }
        self.self_hearing_unlocked
            || self.routes.keys().all(|&(s, o)| self.self_hearing_party(s, o).is_none())
    }
}

/// Fixed identifiers of the standard Claude Talk setup.
pub mod ids {
    use super::{OutputId, SourceId};
    pub const MIC: SourceId = 1;
    pub const CLAUDE: SourceId = 2;
    pub const DISCORD: SourceId = 3;
    pub const TO_DISCORD: OutputId = 101;
    pub const TO_CLAUDE: OutputId = 102;
    pub const MONITOR: OutputId = 103;
}

impl RoutingGraph {
    /// The standard setup: `Til Discord` = mic + Claude, `Til Claude` = mic +
    /// Discord. The monitor output exists but carries nothing by default (the
    /// user already hears Claude and Discord directly).
    pub fn claude_talk_default() -> Self {
        use ids::*;
        let mut g = RoutingGraph::new();
        let set = |p: &[Party]| p.iter().cloned().collect::<BTreeSet<_>>();
        g.add_output(Output { id: TO_DISCORD, label: "Til Discord".into(), heard_by: set(&[Party::Discord]) });
        g.add_output(Output { id: TO_CLAUDE, label: "Til Claude".into(), heard_by: set(&[Party::Claude]) });
        g.add_output(Output { id: MONITOR, label: "Lyt med".into(), heard_by: set(&[Party::User]) });
        let none = BTreeSet::new;
        g.add_source(Source { id: MIC, label: "Mikrofon".into(), parties: set(&[Party::User]), contains_outputs: none() })
            .expect("mic");
        g.add_source(Source { id: CLAUDE, label: "Claude".into(), parties: set(&[Party::Claude]), contains_outputs: none() })
            .expect("claude");
        g.add_source(Source { id: DISCORD, label: "Discord".into(), parties: set(&[Party::Discord]), contains_outputs: none() })
            .expect("discord");
        for (s, o) in [(MIC, TO_DISCORD), (CLAUDE, TO_DISCORD), (MIC, TO_CLAUDE), (DISCORD, TO_CLAUDE)] {
            g.set_route(s, o, Route::default()).expect("default route");
        }
        g
    }
}

#[cfg(test)]
mod tests {
    use super::ids::*;
    use super::*;

    #[test]
    fn default_setup_is_valid_and_routes_as_specified() {
        let g = RoutingGraph::claude_talk_default();
        assert!(g.is_valid());
        assert!(g.route(MIC, TO_DISCORD).is_some());
        assert!(g.route(CLAUDE, TO_DISCORD).is_some());
        assert!(g.route(MIC, TO_CLAUDE).is_some());
        assert!(g.route(DISCORD, TO_CLAUDE).is_some());
        assert!(g.route(CLAUDE, TO_CLAUDE).is_none());
        assert!(g.route(DISCORD, TO_DISCORD).is_none());
    }

    #[test]
    fn claude_to_claude_and_discord_to_discord_are_locked() {
        let mut g = RoutingGraph::claude_talk_default();
        let e = g.set_route(CLAUDE, TO_CLAUDE, Route::default()).unwrap_err();
        assert_eq!(e, Rejection::SelfHearing { party: Party::Claude, locked: true });
        assert_eq!(e.key(), "route.selfHearingLocked");
        let e = g.set_route(DISCORD, TO_DISCORD, Route::default()).unwrap_err();
        assert_eq!(e, Rejection::SelfHearing { party: Party::Discord, locked: true });
    }

    #[test]
    fn unlock_allows_with_warning_and_relock_removes() {
        let mut g = RoutingGraph::claude_talk_default();
        g.set_self_hearing_unlocked(true);
        assert_eq!(g.set_route(CLAUDE, TO_CLAUDE, Route::default()), Ok(Some(Party::Claude)));
        g.set_self_hearing_unlocked(false);
        assert!(g.route(CLAUDE, TO_CLAUDE).is_none());
        assert!(g.is_valid());
    }

    #[test]
    fn loopback_of_own_output_is_feedback_even_when_unlocked() {
        let mut g = RoutingGraph::claude_talk_default();
        g.set_self_hearing_unlocked(true);
        // A source that records the virtual cable carrying "Til Discord".
        g.add_source(Source {
            id: 10,
            label: "Optag kabel".into(),
            parties: BTreeSet::new(),
            contains_outputs: BTreeSet::from([TO_DISCORD]),
        })
        .unwrap();
        let e = g.set_route(10, TO_DISCORD, Route::default()).unwrap_err();
        match e {
            Rejection::Feedback { path } => {
                assert_eq!(path.first().map(String::as_str), Some("Optag kabel"));
                assert_eq!(path.last().map(String::as_str), Some("Optag kabel"));
            }
            other => panic!("expected feedback, got {other:?}"),
        }
        // Routing it elsewhere is fine…
        g.set_route(10, TO_CLAUDE, Route::default()).unwrap();
        // …but then Til Claude must not feed anything that ends in the cable source.
        g.add_source(Source {
            id: 11,
            label: "Optag Til Claude".into(),
            parties: BTreeSet::new(),
            contains_outputs: BTreeSet::from([TO_CLAUDE]),
        })
        .unwrap();
        assert!(matches!(g.set_route(11, TO_DISCORD, Route::default()), Err(Rejection::Feedback { .. })));
        assert!(g.is_valid());
    }

    #[test]
    fn whole_system_loopback_cannot_reach_claude_or_discord() {
        let mut g = RoutingGraph::claude_talk_default();
        g.add_source(Source {
            id: 20,
            label: "Al lyd på pc'en".into(),
            parties: BTreeSet::from([Party::Claude, Party::Discord, Party::Other]),
            contains_outputs: BTreeSet::from([MONITOR]),
        })
        .unwrap();
        assert!(matches!(g.set_route(20, TO_CLAUDE, Route::default()), Err(Rejection::SelfHearing { .. })));
        assert!(matches!(g.set_route(20, TO_DISCORD, Route::default()), Err(Rejection::SelfHearing { .. })));
        assert!(matches!(g.set_route(20, MONITOR, Route::default()), Err(Rejection::Feedback { .. })));
    }

    #[test]
    fn adding_a_source_that_closes_a_loop_is_refused() {
        let mut g = RoutingGraph::claude_talk_default();
        // Replacing the mic with something that contains "Til Discord" while
        // the mic is routed to "Til Discord" would loop.
        let e = g
            .add_source(Source {
                id: MIC,
                label: "Mikrofon".into(),
                parties: BTreeSet::from([Party::User]),
                contains_outputs: BTreeSet::from([TO_DISCORD]),
            })
            .unwrap_err();
        assert!(matches!(e, Rejection::Feedback { .. }));
        assert_eq!(g, RoutingGraph::claude_talk_default());
    }

    mod props {
        use super::*;
        use proptest::prelude::*;
        use proptest::prelude::prop;

        #[derive(Clone, Debug)]
        enum Op {
            Route(u32, u32),
            Unroute(u32, u32),
            AddSource(u32, Vec<u32>, Vec<u8>),
            Unlock(bool),
        }

        fn op() -> impl Strategy<Value = Op> {
            prop_oneof![
                (0u32..6, 0u32..4).prop_map(|(s, o)| Op::Route(s, o)),
                (0u32..6, 0u32..4).prop_map(|(s, o)| Op::Unroute(s, o)),
                (0u32..6, prop::collection::vec(0u32..4, 0..3), prop::collection::vec(0u8..4, 0..3))
                    .prop_map(|(s, c, p)| Op::AddSource(s, c, p)),
                any::<bool>().prop_map(Op::Unlock),
            ]
        }

        fn party(i: u8) -> Party {
            match i {
                0 => Party::User,
                1 => Party::Claude,
                2 => Party::Discord,
                _ => Party::Other,
            }
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(512))]
            #[test]
            fn any_sequence_of_changes_keeps_the_graph_loop_and_echo_free(ops in prop::collection::vec(op(), 1..60)) {
                let mut g = RoutingGraph::new();
                for o in 0..4u32 {
                    g.add_output(Output { id: 100 + o, label: format!("o{o}"), heard_by: BTreeSet::from([party(o as u8)]) });
                }
                for op in ops {
                    let before = g.clone();
                    let result = match op {
                        Op::Route(s, o) => g.set_route(s, 100 + o, Route::default()).map(|_| ()),
                        Op::Unroute(s, o) => { g.remove_route(s, 100 + o); Ok(()) }
                        Op::AddSource(s, c, p) => g.add_source(Source {
                            id: s,
                            label: format!("s{s}"),
                            parties: p.into_iter().map(party).collect(),
                            contains_outputs: c.into_iter().map(|o| 100 + o).collect(),
                        }),
                        Op::Unlock(u) => { g.set_self_hearing_unlocked(u); Ok(()) }
                    };
                    // A refused change leaves the graph untouched.
                    if result.is_err() { prop_assert_eq!(&g, &before); }
                    prop_assert!(g.is_valid(), "graph became invalid: {:?}", g);
                }
            }
        }
    }
}
