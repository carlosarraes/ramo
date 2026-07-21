use std::fs;
use std::io;
use std::path::Path;

use super::model::Annotation;

pub fn write_markdown(annotations: &[Annotation], path: &Path) -> io::Result<()> {
    let content = format_markdown(annotations);
    fs::write(path, content)
}

pub fn print_markdown(annotations: &[Annotation]) {
    print!("{}", format_markdown(annotations));
}

pub fn format_markdown(annotations: &[Annotation]) -> String {
    if annotations.is_empty() {
        return String::from("## Review Comments\n\nNo comments.\n");
    }

    let mut out = String::from("## Review Comments\n\n");

    for annotation in annotations {
        out.push_str(&format!(
            "### {}:{}\n",
            annotation.file, annotation.display_range
        ));

        for ctx_line in annotation.diff_context.lines() {
            out.push_str(&format!("> {}\n", ctx_line));
        }
        out.push('\n');

        out.push_str(&annotation.comment);
        out.push_str("\n\n");
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_single_annotation() {
        let annotations = vec![Annotation {
            file: "src/main.rs".to_string(),
            flat_start: 0,
            flat_end: 2,
            display_range: "10-12(new)".to_string(),
            diff_context: "+    println!(\"hello\");\n+    println!(\"world\");".to_string(),
            comment: "These should be combined into one println.".to_string(),
        }];

        let md = format_markdown(&annotations);
        assert!(md.contains("### src/main.rs:10-12(new)"));
        assert!(md.contains("> +    println!(\"hello\");"));
        assert!(md.contains("These should be combined"));
    }

    #[test]
    fn test_empty_annotations() {
        let md = format_markdown(&[]);
        assert!(md.contains("No comments"));
    }
}
