mod common;

#[test]
fn invalid_config_values_fail_naming_the_field() {
    let cases = [
        ("model_idle_timeout = \"soon\"\n", "`model_idle_timeout`"),
        ("service_idle_timeout = \"0s\"\n", "`service_idle_timeout`"),
        ("index_debounce = 30\n", "index_debounce"),
        ("store_folder = \"sync/trove\"\n", "`store_folder`"),
        ("store_folder = \"\"\n", "`store_folder`"),
        ("store_dir = \".trove\"\n", "`store_dir`"),
    ];

    for (config, field) in cases {
        let data_dir = tempfile::tempdir().unwrap();
        std::fs::write(data_dir.path().join("config.toml"), config).unwrap();

        let output = common::trove(data_dir.path())
            .arg("status")
            .output()
            .unwrap();

        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(!output.status.success(), "{config:?} was accepted");
        assert!(stderr.contains(field), "{config:?} gave {stderr}");
    }
}
