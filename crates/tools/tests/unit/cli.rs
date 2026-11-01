use super::*;

#[test]
fn web_commands_need_no_prepared_game_or_input_directory() {
    assert!(matches!(
        Arguments::try_parse_from(["sims-tools", "build-web"])
            .unwrap()
            .command,
        Task::BuildWeb
    ));
    assert!(matches!(
        Arguments::try_parse_from(["sims-tools", "serve"])
            .unwrap()
            .command,
        Task::Serve { port: 8080 }
    ));
    assert!(matches!(
        Arguments::try_parse_from(["sims-tools", "serve", "--port", "8765"])
            .unwrap()
            .command,
        Task::Serve { port: 8765 }
    ));
}

#[test]
fn publisher_bundle_command_and_web_client_selectors_are_removed() {
    for arguments in [
        vec!["bundle-client", "Complete Collection"],
        vec!["build-web", "--client", ".client"],
        vec!["serve", "--client", ".client"],
        vec!["build-web", "Complete Collection"],
        vec!["build-web", "--profile", "custom.json"],
        vec!["serve", "--skip-intro"],
    ] {
        assert!(Arguments::try_parse_from(std::iter::once("sims-tools").chain(arguments)).is_err());
    }
}

#[test]
fn generic_debug_compilation_is_separate_from_web_publication() {
    let parsed = Arguments::try_parse_from([
        "sims-tools",
        "compile",
        "example.exe",
        "--output",
        "example.wasm",
        "--root",
        "0x401000",
        "--module",
        "0x10000000=example.dll",
    ])
    .unwrap();
    let Task::Compile {
        executable,
        output,
        root,
        module,
    } = parsed.command
    else {
        panic!("expected debug compilation");
    };
    assert_eq!(executable, PathBuf::from("example.exe"));
    assert_eq!(output, PathBuf::from("example.wasm"));
    assert_eq!(root, [0x401000]);
    assert_eq!(module[0].base, 0x10000000);
    assert_eq!(module[0].path, PathBuf::from("example.dll"));
}
