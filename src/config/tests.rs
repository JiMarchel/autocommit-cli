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

fn cfg_under(base: &str) -> PathBuf {
    PathBuf::from(base).join("autocommit").join("config.yaml")
}

#[test]
fn unix_path_follows_xdg_then_home() {
    let unix = |pairs: &[(&str, &str)]| default_path_for(env(pairs), false);
    assert_eq!(
        unix(&[("XDG_CONFIG_HOME", "/x/cfg"), ("HOME", "/home/u")]),
        Some(cfg_under("/x/cfg"))
    );
    assert_eq!(
        unix(&[("HOME", "/home/u"), ("APPDATA", "/ignored")]),
        Some(cfg_under("/home/u/.config"))
    );
    assert_eq!(
        unix(&[("AUTOCOMMIT_CONFIG", "/etc/acm.yaml"), ("HOME", "/h")]),
        Some(PathBuf::from("/etc/acm.yaml"))
    );
    assert_eq!(unix(&[]), None);
}

#[test]
fn windows_path_uses_appdata_even_when_home_is_set() {
    let win = |pairs: &[(&str, &str)]| default_path_for(env(pairs), true);
    let appdata = r"C:\Users\k\AppData\Roaming";
    // Git Bash sets HOME; PowerShell does not. Both must find the same file.
    assert_eq!(
        win(&[("APPDATA", appdata), ("HOME", "/c/Users/k")]),
        Some(cfg_under(appdata))
    );
    assert_eq!(win(&[("APPDATA", appdata)]), Some(cfg_under(appdata)));
    assert_eq!(
        win(&[("USERPROFILE", r"C:\Users\k")]),
        Some(cfg_under(&format!(
            r"C:\Users\k{}.config",
            std::path::MAIN_SEPARATOR
        )))
    );
    assert_eq!(
        win(&[("AUTOCOMMIT_CONFIG", r"D:\acm.yaml"), ("APPDATA", appdata)]),
        Some(PathBuf::from(r"D:\acm.yaml"))
    );
    assert_eq!(
        win(&[("XDG_CONFIG_HOME", r"D:\cfg"), ("APPDATA", appdata)]),
        Some(cfg_under(r"D:\cfg"))
    );
}
