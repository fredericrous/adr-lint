//! `[architecture]` parsing and validation through the library's public
//! surface, and the shared loader.

use adr_lint::config::{self, Config, ConfigError};
use adr_lint::contract::Contract;

const VALID: &str = r#"
[[architecture.layers]]
name = "domain"
paths = ["app/lib/crm/domain.ts"]
modules = ["crate::contract", "ledger"]

[[architecture.layers]]
name = "application"
paths = ["app/lib/**/*.ts", "!app/lib/**/*.server.ts", "!app/lib/crm/domain.ts"]

[[architecture.only_in]]
imports = ["@effect/sql", "rusqlite"]
paths = ["app/lib/services/**"]
modules = ["crate::ledger"]

[architecture.ambient]
modules = ["crate::ledger::clock"]
paths = ["pkg/config"]

[architecture.cycles]
check = false
"#;

fn contract_of(text: &str) -> Result<Option<Contract>, String> {
    Config::from_text(text)
        .contract()
        .map_err(|error| error.to_string())
}

#[test]
fn a_valid_contract_is_parsed_into_typed_values() {
    let contract = contract_of(VALID).unwrap().expect("a contract");
    assert_eq!(contract.layers.len(), 2);
    assert_eq!(contract.layers[0].name, "domain");
    assert_eq!(contract.layers[0].modules[1].text, "ledger");
    assert_eq!(contract.layers[1].paths[1].text, "!app/lib/**/*.server.ts");
    assert_eq!(contract.only_in[0].imports.len(), 2);
    assert_eq!(contract.only_in[0].paths[0].text, "app/lib/services/**");
    assert_eq!(contract.only_in[0].modules[0].text, "crate::ledger");
    assert_eq!(contract.ambient.modules[0].text, "crate::ledger::clock");
    assert_eq!(contract.ambient.paths[0].text, "pkg/config");
    assert!(!contract.check_cycles);
}

#[test]
fn cycles_check_defaults_to_true() {
    let text = "[[architecture.layers]]\nname = \"a\"\npaths = [\"a/**\"]\n";
    assert!(contract_of(text).unwrap().unwrap().check_cycles);
    let text = format!("{text}[architecture.cycles]\n");
    assert!(contract_of(&text).unwrap().unwrap().check_cycles);
}

#[test]
fn no_architecture_section_yields_no_contract() {
    let cli_only = "[[cli.programs]]\nbuild = [\"x\"]\nrun = [\"y\"]\n";
    assert_eq!(contract_of(cli_only), Ok(None));
    assert_eq!(contract_of(""), Ok(None));
}

#[test]
fn a_path_in_two_layers_names_both_entries_and_lines() {
    let text = "[[architecture.layers]]\nname = \"inner\"\npaths = [\n  \"src/a.ts\",\n]\n\n[[architecture.layers]]\nname = \"outer\"\npaths = [\n  \"src/**/*.ts\",\n]\n";
    let error = contract_of(text).unwrap_err();
    assert!(error.contains("adr-lint.toml:4"), "{error}");
    assert!(error.contains("adr-lint.toml:10"), "{error}");
    assert!(
        error.contains("`inner`") && error.contains("`outer`"),
        "{error}"
    );
}

#[test]
fn an_equal_path_in_two_layers_is_rejected() {
    let text = "[[architecture.layers]]\nname = \"a\"\npaths = [\"x/**\"]\n\n[[architecture.layers]]\nname = \"b\"\npaths = [\"x/**\"]\n";
    let error = contract_of(text).unwrap_err();
    assert!(
        error.contains("adr-lint.toml:3") && error.contains("adr-lint.toml:7"),
        "{error}"
    );
}

#[test]
fn a_module_in_two_layers_names_both_entries_and_lines() {
    let text = "[[architecture.layers]]\nname = \"inner\"\nmodules = [\n  \"crate::ledger\",\n]\n\n[[architecture.layers]]\nname = \"outer\"\nmodules = [\n  \"cli\",\n  \"crate::ledger\",\n]\n";
    let error = contract_of(text).unwrap_err();
    assert!(error.contains("adr-lint.toml:4"), "{error}");
    assert!(error.contains("adr-lint.toml:11"), "{error}");
}

#[test]
fn exclude_globs_decide_which_layer_a_path_is_in() {
    let contract = contract_of(VALID).unwrap().unwrap();
    let layer = |path: &str| {
        contract
            .layer_of_path(path)
            .unwrap()
            .map(|layer| layer.name.clone())
    };
    assert_eq!(
        layer("app/lib/services/db.ts"),
        Some("application".to_string())
    );
    assert_eq!(layer("app/lib/services/db.server.ts"), None);
    assert_eq!(layer("app/lib/crm/domain.ts"), Some("domain".to_string()));
    assert_eq!(layer("other/file.ts"), None);
}

#[test]
fn modules_match_by_longest_prefix_on_segment_boundaries() {
    let text = "[[architecture.layers]]\nname = \"inner\"\nmodules = [\"crate::ledger\"]\n\n[[architecture.layers]]\nname = \"outer\"\nmodules = [\"crate::ledger::clock\"]\n";
    let contract = contract_of(text).unwrap().unwrap();
    let name = |module: &str| contract.layer_of_module(module).map(|l| l.name.as_str());
    assert_eq!(name("crate::ledger::store"), Some("inner"));
    assert_eq!(name("crate::ledger::clock::now"), Some("outer"));
    assert_eq!(name("crate::ledgers"), None);
}

#[test]
fn a_bad_contract_names_its_line() {
    let error = contract_of("[architecture]\nbogus = 1\n").unwrap_err();
    assert!(error.contains("adr-lint.toml:2"), "{error}");
    let error = contract_of("[[architecture.only_in]]\nimports = [\"x\"]\n").unwrap_err();
    assert!(error.contains("adr-lint.toml:1"), "{error}");
}

#[test]
fn the_loader_reads_the_file_or_returns_an_empty_configuration() {
    let dir = std::env::temp_dir().join(format!("adr-lint-config-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    assert_eq!(config::load(&dir).unwrap().text(), "");
    std::fs::write(dir.join(config::FILE), VALID).unwrap();
    assert!(config::validate(&dir).unwrap().is_some());
    std::fs::write(dir.join(config::FILE), "[architecture]\nbogus = 1\n").unwrap();
    assert!(matches!(
        config::validate(&dir),
        Err(ConfigError::Contract(_))
    ));
    std::fs::remove_dir_all(&dir).unwrap();
}
