use ast_grep_config::{from_yaml_string, GlobalRules};
use ast_grep_language::SupportLang;

fn main() {
    let text = std::fs::read_to_string(r"test.yml").unwrap();
    println!("--- full file ---");
    match from_yaml_string::<SupportLang>(&text, &GlobalRules::default()) {
        Ok(r) => println!("ok: {} rules", r.len()),
        Err(e) => println!("full err: {e}"),
    }
    // try each any branch alone
    let _branches = [
        r"export { \$\ }",
        r"export default function \(\$\$) { \$\$ }",
    ];
}
