use nucleo_matcher::{Matcher, pattern::Pattern};

use crate::app::App;

pub struct FuzzyMatcher {}

impl FuzzyMatcher {
    pub fn new() -> Self {
        Self {}
    }

    pub fn match_indices<T: AsRef<str>>(&self, pattern: &str, items: &[T]) -> Vec<usize> {
        struct Candidate<'a> {
            index: usize,
            label: &'a str,
        }

        impl AsRef<str> for Candidate<'_> {
            fn as_ref(&self) -> &str {
                self.label
            }
        }

        let pattern = Pattern::parse(
            pattern,
            nucleo_matcher::pattern::CaseMatching::Ignore,
            nucleo_matcher::pattern::Normalization::Smart,
        );
        let mut matcher = Matcher::new(nucleo_matcher::Config::DEFAULT.match_paths());

        pattern
            .match_list(
                items.iter().enumerate().map(|(index, item)| Candidate {
                    index,
                    label: item.as_ref(),
                }),
                &mut matcher,
            )
            .into_iter()
            .map(|(candidate, _)| candidate.index)
            .collect()
    }
}

pub trait FuzzyMatcherApp {
    fn fuzzy_matcher(&self) -> FuzzyMatcher;
}

impl FuzzyMatcherApp for &'static App {
    fn fuzzy_matcher(&self) -> FuzzyMatcher {
        FuzzyMatcher::new()
    }
}

#[cfg(test)]
mod tests {
    use super::FuzzyMatcher;

    #[test]
    fn matches_each_query_term() {
        let items = [
            "github.com/acme/gitnow",
            "git.kjuulh.io/kjuulh/gitnow",
            "git.kjuulh.io/kjuulh/mire",
        ];

        let matches = FuzzyMatcher::new().match_indices("kjuulh gitnow", &items);

        assert_eq!(matches, vec![1]);
    }

    #[test]
    fn supports_fzf_style_exact_and_inverse_terms() {
        let items = [
            "github.com/acme/gitnow",
            "git.kjuulh.io/kjuulh/gitnow",
            "git.kjuulh.io/kjuulh/git-now",
        ];

        let matches = FuzzyMatcher::new().match_indices("'gitnow !github", &items);

        assert_eq!(matches, vec![1]);
    }

    #[test]
    fn keeps_duplicate_labels_selectable() {
        let items = ["same", "same"];

        let matches = FuzzyMatcher::new().match_indices("same", &items);

        assert_eq!(matches, vec![0, 1]);
    }
}
