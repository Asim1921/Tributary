//! Reads `robots.txt`, as described in RFC 9309.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// A site can't ask for longer than this between requests, or a project would never end.
const MAX_CRAWL_DELAY_SECONDS: f64 = 30.0;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Rule {
    allow: bool,
    pattern: String,
}

/// What a site asks of crawlers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Robots {
    rules: Vec<Rule>,
    delay_ms: Option<u64>,
    deny_all: bool,
}

/// Matches a path against a pattern, where `*` is any run of characters and a final `$`
/// means the path must end there.
fn matches(pattern: &str, path: &str) -> bool {
    let (pattern, anchored) = match pattern.strip_suffix('$') {
        Some(rest) => (rest, true),
        None => (pattern, false),
    };

    let mut pieces = pattern.split('*');
    let Some(mut rest) = pieces.next().and_then(|first| path.strip_prefix(first)) else {
        return false;
    };

    let pieces: Vec<&str> = pieces.collect();
    for (index, piece) in pieces.iter().enumerate() {
        if anchored && index + 1 == pieces.len() {
            return rest.ends_with(piece);
        }
        match rest.find(piece) {
            Some(at) => rest = &rest[at + piece.len()..],
            None => return false,
        }
    }
    !anchored || rest.is_empty()
}

/// A group of rules, and which crawlers it's for.
#[derive(Default)]
struct Group {
    agents: Vec<String>,
    rules: Vec<Rule>,
    delay_ms: Option<u64>,
}

fn read_groups(text: &str) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    let mut in_agents = false;

    for line in text.lines() {
        let line = line.split('#').next().unwrap_or_default();
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();

        if key == "user-agent" {
            if !in_agents {
                groups.push(Group::default());
            }
            in_agents = true;
            if let Some(group) = groups.last_mut() {
                group.agents.push(value.to_ascii_lowercase());
            }
            continue;
        }

        in_agents = false;
        let Some(group) = groups.last_mut() else {
            continue;
        };
        match key.as_str() {
            // An empty rule says nothing, so it has nothing to match.
            "allow" | "disallow" if !value.is_empty() => group.rules.push(Rule {
                allow: key == "allow",
                pattern: value.to_owned(),
            }),
            "crawl-delay" => {
                if let Ok(seconds) = value.parse::<f64>() {
                    let seconds = seconds.clamp(0.0, MAX_CRAWL_DELAY_SECONDS);
                    group.delay_ms = Some((seconds * 1000.0) as u64);
                }
            }
            _ => {}
        }
    }
    groups
}

impl Robots {
    /// Reads the rules for a crawler, which are those naming it, or otherwise those for
    /// everyone.
    pub fn parse(text: &str, agent: &str) -> Self {
        let agent = agent.to_ascii_lowercase();
        let groups = read_groups(text);

        let named = |group: &&Group| group.agents.iter().any(|name| name == &agent);
        let general = |group: &&Group| group.agents.iter().any(|name| name == "*");

        let chosen: Vec<&Group> = if groups.iter().any(|group| named(&group)) {
            groups.iter().filter(named).collect()
        } else {
            groups.iter().filter(general).collect()
        };

        Self {
            rules: chosen
                .iter()
                .flat_map(|group| group.rules.iter().cloned())
                .collect(),
            delay_ms: chosen.iter().find_map(|group| group.delay_ms),
            deny_all: false,
        }
    }

    /// For a site whose rules can't be read because it's in trouble. RFC 9309 says to
    /// stay away until they can be.
    pub fn deny_all() -> Self {
        Self {
            deny_all: true,
            ..Self::default()
        }
    }

    /// Whether a path may be fetched. The longest matching rule decides, and on a tie
    /// `allow` wins.
    pub fn allows(&self, path: &str) -> bool {
        if self.deny_all {
            return false;
        }

        self.rules
            .iter()
            .filter(|rule| matches(&rule.pattern, path))
            .max_by_key(|rule| (rule.pattern.len(), rule.allow))
            .is_none_or(|rule| rule.allow)
    }

    /// How long the site asks crawlers to wait between requests.
    pub fn delay(&self) -> Option<Duration> {
        self.delay_ms.map(Duration::from_millis)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn robots(text: &str) -> Robots {
        Robots::parse(text, "tributary")
    }

    #[test]
    fn nothing_written_means_everything_is_allowed() {
        assert!(robots("").allows("/anything"));
        assert!(Robots::default().allows("/anything"));
    }

    #[test]
    fn a_disallowed_folder_is_refused() {
        let rules = robots("User-agent: *\nDisallow: /private/\n");

        assert!(!rules.allows("/private/secret.html"));
        assert!(rules.allows("/public.html"));
        assert!(rules.allows("/private"), "the folder itself isn't covered");
    }

    #[test]
    fn an_empty_disallow_allows_everything() {
        assert!(robots("User-agent: *\nDisallow:\n").allows("/any"));
    }

    #[test]
    fn disallowing_the_root_refuses_everything() {
        assert!(!robots("User-agent: *\nDisallow: /\n").allows("/any"));
    }

    #[test]
    fn the_longest_rule_wins() {
        let rules = robots("User-agent: *\nDisallow: /a/\nAllow: /a/open/\n");

        assert!(!rules.allows("/a/closed.html"));
        assert!(rules.allows("/a/open/page.html"));
    }

    #[test]
    fn allow_wins_a_tie() {
        assert!(robots("User-agent: *\nDisallow: /a\nAllow: /a\n").allows("/a"));
    }

    #[test]
    fn rules_naming_this_crawler_replace_the_general_ones() {
        let text = "User-agent: *\nDisallow: /\n\nUser-agent: Tributary\nDisallow: /only/\n";
        let rules = robots(text);

        assert!(rules.allows("/page.html"));
        assert!(!rules.allows("/only/page.html"));
    }

    #[test]
    fn rules_for_other_crawlers_do_not_apply() {
        assert!(robots("User-agent: other\nDisallow: /\n").allows("/page.html"));
    }

    #[test]
    fn several_agents_can_share_a_group() {
        let rules = robots("User-agent: other\nUser-agent: tributary\nDisallow: /a\n");
        assert!(!rules.allows("/a"));
    }

    #[test]
    fn groups_for_the_same_crawler_add_up() {
        let text = "User-agent: *\nDisallow: /a\n\nUser-agent: other\nDisallow: /b\n\n\
                    User-agent: *\nDisallow: /c\n";
        let rules = robots(text);

        assert!(!rules.allows("/a"));
        assert!(rules.allows("/b"));
        assert!(!rules.allows("/c"));
    }

    #[test]
    fn wildcards_and_ends_are_understood() {
        let rules = robots("User-agent: *\nDisallow: /*.pdf$\nDisallow: /tmp*/cache\n");

        assert!(!rules.allows("/files/a.pdf"));
        assert!(rules.allows("/files/a.pdf.html"));
        assert!(!rules.allows("/tmp1/cache/x"));
        assert!(rules.allows("/tmp1/other"));
    }

    #[test]
    fn comments_and_odd_spacing_are_ignored() {
        let rules = robots("# About\nUSER-AGENT : *  # everyone\n  disallow:/a # here\n");
        assert!(!rules.allows("/a"));
    }

    #[test]
    fn rules_before_any_agent_mean_nothing() {
        assert!(robots("Disallow: /\n").allows("/page.html"));
    }

    #[test]
    fn the_requested_delay_is_read_and_limited() {
        let delay = |text| robots(text).delay();

        assert_eq!(
            delay("User-agent: *\nCrawl-delay: 2\n"),
            Some(Duration::from_secs(2))
        );
        assert_eq!(
            delay("User-agent: *\nCrawl-delay: 0.5\n"),
            Some(Duration::from_millis(500))
        );
        assert_eq!(
            delay("User-agent: *\nCrawl-delay: 99999\n"),
            Some(Duration::from_secs(30))
        );
        assert_eq!(delay("User-agent: *\nCrawl-delay: soon\n"), None);
        assert_eq!(delay("User-agent: *\n"), None);
    }

    #[test]
    fn a_site_in_trouble_is_left_alone() {
        assert!(!Robots::deny_all().allows("/"));
    }
}
