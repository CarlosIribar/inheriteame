use crate::model::{Analysis, Diagnostic, Edit, Severity};
use tree_sitter::{Node, Parser};

pub struct Inspection {
    pub analysis: Analysis,
    pub class_name: Option<String>,
    pub ignored: bool,
}

/// Only leading comments can disable a file. An annotation ends the header.
pub fn header_ignored(source: &str) -> bool {
    let mut rest = source.trim_start_matches('\u{feff}').trim_start();
    loop {
        let (comment, tail) = if let Some(line) = rest.strip_prefix("//") {
            line.split_once('\n').unwrap_or((line, ""))
        } else if let Some(block) = rest.strip_prefix("/*") {
            let Some(pair) = block.split_once("*/") else {
                return false;
            };
            pair
        } else {
            return false;
        };
        if comment
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            .any(|word| word == "apex-inherited-sharing-ignore")
        {
            return true;
        }
        rest = tail.trim_start();
    }
}

pub fn inspect(source: &str) -> Inspection {
    let mut result = Inspection {
        analysis: Analysis::default(),
        class_name: None,
        ignored: header_ignored(source),
    };
    if result.ignored {
        return result;
    }
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_sfapex::apex::LANGUAGE.into())
        .expect("compatible grammar");
    let tree = parser
        .parse(source, None)
        .expect("parser without cancellation");
    let root = tree.root_node();
    let mut cursor = root.walk();
    let class = root
        .named_children(&mut cursor)
        .find(|node| node.kind() == "class_declaration");
    if let Some(class) = class {
        result.class_name = class
            .child_by_field_name("name")
            .map(|n| source[n.byte_range()].to_owned());
    }
    if root.has_error() {
        result.analysis.diagnostics.push(Diagnostic {
            code: "PARSE_ERROR",
            severity: Severity::Error,
            start_byte: 0,
            end_byte: source.len(),
            message: "Apex could not be parsed; the file was left unchanged.".into(),
            suggestion: None,
        });
        return result;
    }
    collect_missing_sharing(root, &mut result.analysis.edits);
    result
}

fn collect_missing_sharing(node: Node<'_>, edits: &mut Vec<Edit>) {
    if node.kind() == "class_declaration" {
        let mut cursor = node.walk();
        let modifiers = node
            .named_children(&mut cursor)
            .find(|child| child.kind() == "modifiers");
        if !modifiers.is_some_and(has_sharing) {
            let mut cursor = node.walk();
            if let Some(keyword) = node
                .children(&mut cursor)
                .find(|child| child.kind() == "class")
            {
                edits.push(Edit {
                    start_byte: keyword.start_byte(),
                    end_byte: keyword.start_byte(),
                    replacement: "inherited sharing ".into(),
                    rule_id: "inherited-sharing",
                });
            }
        }
    }
    for index in 0..node.child_count() {
        if let Some(child) = node.child(index) {
            collect_missing_sharing(child, edits);
        }
    }
}

fn has_sharing(node: Node<'_>) -> bool {
    if matches!(
        node.kind(),
        "with_sharing" | "without_sharing" | "inherited_sharing"
    ) {
        return true;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor).any(has_sharing)
}

pub fn analyze(source: &str) -> Analysis {
    inspect(source).analysis
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply_edits;

    #[test]
    fn inserts_into_every_class_declaration_and_is_idempotent() {
        for prefix in [
            "public",
            "global virtual",
            "public abstract",
            "@IsTest\nprivate",
            "public\r\nvirtual",
        ] {
            let source = format!("/* José */\r\n{prefix} class Example {{ class Inner {{}} }}");
            let result = inspect(&source);
            assert!(result.analysis.diagnostics.is_empty(), "{source}");
            assert_eq!(result.class_name.as_deref(), Some("Example"));
            assert_eq!(result.analysis.edits.len(), 2);
            let fixed = apply_edits(&source, &result.analysis.edits).unwrap();
            assert_eq!(
                fixed,
                source
                    .replace("class Example", "inherited sharing class Example")
                    .replace("class Inner", "inherited sharing class Inner")
            );
            assert!(analyze(&fixed).diagnostics.is_empty(), "{fixed}");
            assert!(analyze(&fixed).edits.is_empty());
        }
    }

    #[test]
    fn repairs_nested_classes_independently_of_explicit_outer_and_inner_modes() {
        let source = "public with sharing class Outer {\n\
            private class Missing {\n\
                protected inherited sharing class Explicit {}\n\
                class DeepMissing {}\n\
            }\n\
            public without sharing class Boundary {}\n\
        }";
        let result = inspect(source);
        assert_eq!(result.class_name.as_deref(), Some("Outer"));
        assert_eq!(result.analysis.edits.len(), 2);
        let fixed = apply_edits(source, &result.analysis.edits).unwrap();
        assert!(fixed.contains("private inherited sharing class Missing"));
        assert!(fixed.contains("class Explicit {}"));
        assert!(fixed.contains("inherited sharing class DeepMissing"));
        assert!(fixed.contains("public without sharing class Boundary"));
        assert!(analyze(&fixed).edits.is_empty());
    }

    #[test]
    fn preserves_modes_and_other_declarations() {
        for source in [
            "public with sharing class A {}",
            "public without sharing class A {}",
            "public inherited sharing virtual class A {}",
            "public WITH SHARING class A {}",
            "public interface A {}",
            "public enum A { One }",
            "public with sharing class A { inherited sharing class Inner {} }",
        ] {
            let result = analyze(source);
            assert!(
                result.diagnostics.is_empty(),
                "{source}: {:?}",
                result.diagnostics
            );
            assert!(result.edits.is_empty(), "{source}");
        }
    }

    #[test]
    fn header_marker_is_explicit_and_leading() {
        for header in [
            "// apex-inherited-sharing-ignore: reason\n",
            "/* apex-inherited-sharing-ignore */",
            "/**\n * apex-inherited-sharing-ignore\n */",
            "\u{feff} // license\n// apex-inherited-sharing-ignore\n",
        ] {
            assert!(inspect(&format!("{header}@IsTest public class A {{}}")).ignored);
        }
        for source in [
            "// license\npublic class A {}",
            "public class A { String s = 'apex-inherited-sharing-ignore'; }",
            "public class A { /* apex-inherited-sharing-ignore */ }",
            "@IsTest /* apex-inherited-sharing-ignore */ public class A {}",
            "// not-apex-inherited-sharing-ignore\npublic class A {}",
        ] {
            assert!(!inspect(source).ignored, "{source}");
            assert_eq!(analyze(source).edits.len(), 1, "{source}");
        }
    }

    #[test]
    fn invalid_source_has_no_edits() {
        let result = analyze("public class Broken {");
        assert!(result.edits.is_empty());
        assert_eq!(result.diagnostics[0].code, "PARSE_ERROR");
    }
}
