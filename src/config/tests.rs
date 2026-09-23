use super::*;

fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: Vec<(String, String)> = pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    move |k| map.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone())
}

#[test]
fn parses_yaml() {
    let f = FileConfig::parse("api_key: abc\nmodel: m1\nbase_url: http://x\n").unwrap();
    assert_eq!(f.api_key.as_deref(), Some("abc"));
    assert_eq!(f.model.as_deref(), Some("m1"));
    assert_eq!(f.base_url.as_deref(), Some("http://x"));
}

#[test]
fn empty_file_is_default() {
    assert_eq!(FileConfig::parse("").unwrap(), FileConfig::default());
    assert_eq!(
        FileConfig::parse("# only a comment\n").unwrap(),
        FileConfig::default()
    );
}

#[test]
fn unknown_keys_are_rejected() {
    let err = FileConfig::parse("apikey: abc\n").unwrap_err().to_string();
    assert!(err.contains("apikey"), "{err}");
}

#[test]
fn precedence_cli_env_file_default() {
    let file = FileConfig {
        api_key: Some("file-key".into()),
        model: Some("file-model".into()),
        base_url: Some("http://file".into()),
    };
    let c = Config::resolve(None, &file, env(&[])).unwrap();
    assert_eq!(c.api_key, "file-key");
    assert_eq!(c.model, "file-model");
    assert_eq!(c.base_url, "http://file");

    let e = env(&[
        ("GEMINI_API_KEY", "env-key"),
        ("AUTOCOMMIT_MODEL", "env-model"),
        ("GEMINI_BASE_URL", "http://env"),
    ]);
    let c = Config::resolve(None, &file, &e).unwrap();
    assert_eq!(c.api_key, "env-key");
    assert_eq!(c.model, "env-model");
    assert_eq!(c.base_url, "http://env");

    let c = Config::resolve(Some("cli-model"), &file, &e).unwrap();
    assert_eq!(c.model, "cli-model");
}

#[test]
fn defaults_apply_and_blank_values_are_ignored() {
    let file = FileConfig {
        api_key: Some("k".into()),
        model: Some("  ".into()),
        base_url: None,
    };
    let c = Config::resolve(None, &file, env(&[("AUTOCOMMIT_MODEL", "")])).unwrap();
    assert_eq!(c.model, crate::gemini::DEFAULT_MODEL);
    assert_eq!(c.base_url, crate::gemini::DEFAULT_BASE_URL);
}

#[test]
fn missing_key_error_mentions_both_sources() {
    let err = Config::resolve(
        None,
        &FileConfig::default(),
        env(&[("GEMINI_API_KEY", " ")]),
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("GEMINI_API_KEY"), "{err}");
    assert!(err.contains("api_key"), "{err}");
}

#[test]
fn default_path_follows_xdg() {
    let p = default_path(env(&[("XDG_CONFIG_HOME", "/x/cfg"), ("HOME", "/home/u")])).unwrap();
    assert_eq!(p, std::path::PathBuf::from("/x/cfg/autocommit/config.yaml"));
    let p = default_path(env(&[("HOME", "/home/u")])).unwrap();
    assert_eq!(
        p,
        std::path::PathBuf::from("/home/u/.config/autocommit/config.yaml")
    );
    let p = default_path(env(&[
        ("AUTOCOMMIT_CONFIG", "/etc/acm.yaml"),
        ("HOME", "/h"),
    ]))
    .unwrap();
    assert_eq!(p, std::path::PathBuf::from("/etc/acm.yaml"));
}
