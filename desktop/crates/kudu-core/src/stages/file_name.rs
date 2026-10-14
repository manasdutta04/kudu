use crate::correction_learner::is_stopword;
use crate::pipeline::{TextContext, TextStage};
use crate::text::{lower_first, lowercase, upper_first};
use regex::Regex;
use std::sync::LazyLock;

/// Extensions recognized after a spoken "dot".
const EXTENSIONS: &[&str] = &[
    "ts", "tsx", "js", "jsx", "mjs", "py", "swift", "json", "md", "mdx", "rs", "go", "java", "kt", "rb", "css", "scss",
    "html", "yml", "yaml", "sh", "txt", "cpp", "c", "h", "vue", "php", "sql", "toml", "env", "csv", "pdf", "png",
    "jpg", "svg", "xml", "lock", "dart", "cs", "ex", "zig", "lua", "ini", "plist", "docx", "xlsx",
];

static PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"(?i)((?:\b[A-Za-z][A-Za-z0-9'’]*[ ]+){{1,4}})dot ({})\b", EXTENSIONS.join("|"))).unwrap()
});

/// Words that end a file name going backwards: verbs and small words before it (besides the
/// everyday stopwords).
const BOUNDARY: &[&str] = &[
    "open", "opened", "fix", "fixed", "edit", "check", "update", "updated", "see", "run", "delete", "remove", "rename",
    "create", "created", "move", "save", "look", "change", "changed", "add", "added", "test", "review", "push",
    "pushed", "commit", "close", "import", "copy", "send", "share", "upload", "download", "attach", "attached",
    "write", "modify", "refactor", "inside", "file", "called", "named", "new", "old", "under", "go", "please", "read",
    "using",
];

fn is_boundary(lowercased: &str) -> bool {
    is_stopword(lowercased) || BOUNDARY.contains(&lowercased)
}

/// Names that are conventionally written in capitals.
fn conventional(joined: &str) -> Option<&'static str> {
    Some(match joined {
        "readme" => "README",
        "license" => "LICENSE",
        "changelog" => "CHANGELOG",
        "contributing" => "CONTRIBUTING",
        "dockerfile" => "Dockerfile",
        "makefile" => "Makefile",
        _ => return None,
    })
}

/// File names said aloud are written as file names: "fix the use case dot ts" → "fix the
/// useCase.ts", "open read me dot md" → "open README.md", "package dot json" → "package.json".
///
/// The name is the one to three words before "dot", stopping at a verb or small word
/// ("open", "the", "in"), joined in camelCase the way code names files.
#[derive(Clone, Copy, Debug, Default)]
pub struct FileNameStage;

impl TextStage for FileNameStage {
    fn apply(&self, text: &str, _context: &TextContext) -> String {
        let mut result = text.to_owned();
        let matches: Vec<_> = PATTERN
            .captures_iter(text)
            .filter_map(|captures| {
                Some((captures.get(0)?.range(), captures.get(1)?.as_str(), captures.get(2)?.as_str()))
            })
            .collect();
        // From the end, so earlier ranges stay valid.
        for (whole, words, extension) in matches.into_iter().rev() {
            let candidates: Vec<&str> = words.split(' ').filter(|word| !word.is_empty()).collect();
            // Walk back from "dot" while the words look like part of the name.
            let mut name: Vec<&str> = Vec::new();
            for &word in candidates.iter().rev() {
                let lowercased = lowercase(word);
                // "read me" is a name, even though "read" is a verb.
                if lowercased == "read" && name.first().is_some_and(|first| lowercase(first) == "me") {
                    name.insert(0, word);
                    break;
                }
                if is_boundary(&lowercased) && !(lowercased == "me" && name.is_empty()) {
                    break;
                }
                name.insert(0, word);
                if name.len() == 3 {
                    break;
                }
            }
            if name.is_empty() {
                continue;
            }
            let file = format!("{}.{}", join(&name), lowercase(extension));
            // Replace from the first name word to the extension.
            let Some(offset) = text[whole.clone()].rfind(&name.join(" ")) else {
                continue;
            };
            result.replace_range(whole.start + offset..whole.end, &file);
        }
        result
    }
}

fn join(words: &[&str]) -> String {
    let joined: String = words.iter().map(|word| lowercase(word)).collect();
    if let Some(conventional) = conventional(&joined) {
        return conventional.to_owned();
    }
    if let [word] = words {
        return (*word).to_owned();
    }
    words
        .iter()
        .enumerate()
        .map(|(index, word)| if index == 0 { lower_first(word) } else { upper_first(word) })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(text: &str) -> String {
        FileNameStage.apply(text, &TextContext::default())
    }

    #[test]
    fn writes_spoken_file_names() {
        assert_eq!(apply("Okay, so fix the use case dot ts."), "Okay, so fix the useCase.ts.");
        assert_eq!(apply("open user service dot ts"), "open userService.ts");
        assert_eq!(
            apply("check package dot json and the api client dot py"),
            "check package.json and the apiClient.py"
        );
        assert_eq!(apply("I attached the report dot pdf"), "I attached the report.pdf");
    }

    #[test]
    fn knows_conventional_names() {
        assert_eq!(apply("update the read me dot md"), "update the README.md");
    }

    #[test]
    fn leaves_other_dots_alone() {
        assert_eq!(apply("connect the dots on the map"), "connect the dots on the map");
        assert_eq!(apply("polka dot dress"), "polka dot dress");
    }
}
