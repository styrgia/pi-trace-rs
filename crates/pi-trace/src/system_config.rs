use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    path::{Component, Path, PathBuf},
};
use trace_core::native::Provider;

#[derive(Clone, Debug)]
pub struct StoreConfig {
    pub provider: Provider,
    pub path: PathBuf,
}
#[derive(Clone, Debug)]
pub struct SystemConfig {
    pub stores: Vec<StoreConfig>,
    pub disabled: Vec<Provider>,
}
fn absolute(cwd: &Path, path: &Path) -> PathBuf {
    let joined = if path.is_absolute() {
        path.to_owned()
    } else {
        cwd.join(path)
    };
    let mut result = PathBuf::new();
    for part in joined.components() {
        match part {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {}
            p => result.push(p.as_os_str()),
        }
    }
    result
}
pub fn canonical(path: &Path) -> PathBuf {
    if let Ok(p) = path.canonicalize() {
        return p;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => canonical(parent).join(name),
        _ => path.to_owned(),
    }
}
impl SystemConfig {
    pub fn resolve(
        args: &[OsString],
        cwd: &Path,
        env: &BTreeMap<String, String>,
    ) -> Result<Self, String> {
        let mut roots: BTreeMap<Provider, Vec<PathBuf>> = BTreeMap::new();
        let mut disabled = BTreeSet::new();
        let mut args = args.iter();
        while let Some(option) = args.next() {
            let value = args
                .next()
                .and_then(|v| v.to_str())
                .ok_or("System options require a value.")?;
            match option.to_str() {
                Some("--root") => {
                    let (name, path) = value.split_once('=').ok_or("Use --root provider=path.")?;
                    let provider: Provider = name.parse()?;
                    if path.is_empty()
                        || path.contains('\0')
                        || (provider == Provider::Opencode && path == ":memory:")
                    {
                        return Err("Select a nonempty on-disk root.".into());
                    }
                    roots
                        .entry(provider)
                        .or_default()
                        .push(canonical(&absolute(cwd, Path::new(path))));
                }
                Some("--disable-provider") => {
                    disabled.insert(value.parse::<Provider>()?);
                }
                _ => return Err("Unknown system option.".into()),
            }
        }
        let home = env.get("HOME").filter(|v| !v.is_empty()).map(PathBuf::from);
        let inherited = |key: &str, suffix: &str| -> Result<PathBuf, String> {
            let path = env
                .get(key)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .or_else(|| home.as_ref().map(|h| h.join(suffix)))
                .ok_or_else(|| {
                    "HOME is required for an unconfigured provider default.".to_owned()
                })?;
            Ok(absolute(cwd, &path))
        };
        let mut stores = Vec::new();
        for provider in [
            Provider::Claude,
            Provider::Codex,
            Provider::Opencode,
            Provider::Pi,
        ] {
            if disabled.contains(&provider) {
                if roots.contains_key(&provider) {
                    return Err("A provider cannot be configured and disabled.".into());
                }
                continue;
            }
            let selected = if let Some(paths) = roots.remove(&provider) {
                paths
            } else {
                vec![match provider {
                    Provider::Codex => inherited("CODEX_HOME", ".codex")?,
                    Provider::Pi => {
                        if let Some(path) = env
                            .get("PI_CODING_AGENT_SESSION_DIR")
                            .filter(|p| !p.is_empty())
                        {
                            absolute(cwd, Path::new(path))
                        } else {
                            inherited("PI_CODING_AGENT_DIR", ".pi/agent")?.join("sessions")
                        }
                    }
                    Provider::Claude => inherited("CLAUDE_CONFIG_DIR", ".claude")?.join("projects"),
                    Provider::Opencode => {
                        let directory =
                            inherited("XDG_DATA_HOME", ".local/share")?.join("opencode");
                        match env.get("OPENCODE_DB").filter(|p| !p.is_empty()) {
                            None => directory,
                            Some(p) if p == ":memory:" => {
                                return Err(
                                    "Memory-only OpenCode databases are unavailable.".into()
                                );
                            }
                            Some(p) => {
                                let path = absolute(&directory, Path::new(p));
                                if !Path::new(p).is_absolute()
                                    && (!path.starts_with(&directory)
                                        || !canonical(&path).starts_with(canonical(&directory)))
                                {
                                    return Err("Relative OPENCODE_DB must remain inside its data directory.".into());
                                }
                                path
                            }
                        }
                    }
                }]
            };
            let selected: BTreeSet<_> = selected.into_iter().map(|p| canonical(&p)).collect();
            stores.extend(
                selected
                    .into_iter()
                    .map(|path| StoreConfig { provider, path }),
            );
        }
        if stores.len() > 16 {
            return Err("System mode supports at most 16 configured stores.".into());
        }
        Ok(Self {
            stores,
            disabled: disabled.into_iter().collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn env(home: &Path) -> BTreeMap<String, String> {
        BTreeMap::from([("HOME".into(), home.to_string_lossy().into_owned())])
    }
    fn args(items: &[&str]) -> Vec<OsString> {
        items.iter().map(OsString::from).collect()
    }
    #[test]
    fn defaults_inheritance_overrides_and_spaces() {
        let dir = tempfile::tempdir().unwrap();
        let mut e = env(dir.path());
        e.insert("CODEX_HOME".into(), "relative home".into());
        e.insert("PI_CODING_AGENT_DIR".into(), "relative pi".into());
        e.insert("CLAUDE_CONFIG_DIR".into(), "claude home".into());
        e.insert("XDG_DATA_HOME".into(), "data home".into());
        let c = SystemConfig::resolve(&[], dir.path(), &e).unwrap();
        assert_eq!(c.stores.len(), 4);
        assert!(
            c.stores.iter().any(
                |s| s.provider == Provider::Codex && s.path == dir.path().join("relative home")
            )
        );
        assert!(c.stores.iter().any(
            |s| s.provider == Provider::Pi && s.path == dir.path().join("relative pi/sessions")
        ));
        e.insert(
            "PI_CODING_AGENT_SESSION_DIR".into(),
            "custom sessions".into(),
        );
        let c = SystemConfig::resolve(
            &args(&[
                "--root",
                "pi=override with spaces",
                "--root",
                "pi=second",
                "--root",
                "pi=./second",
            ]),
            dir.path(),
            &e,
        )
        .unwrap();
        assert_eq!(
            c.stores
                .iter()
                .filter(|s| s.provider == Provider::Pi)
                .count(),
            2
        );
        assert!(
            c.stores
                .iter()
                .any(|s| s.path == dir.path().join("override with spaces"))
        );
    }
    #[test]
    fn reject_conflicts_unknowns_memory_and_store_overflow() {
        let dir = tempfile::tempdir().unwrap();
        for a in [
            vec!["--root", "bad=x"],
            vec!["--disable-provider", "bad"],
            vec!["--root", "pi=x", "--disable-provider", "pi"],
            vec!["--root", "pi="],
            vec!["--root", "opencode=:memory:"],
            vec!["--unknown", "value"],
            vec!["--root"],
        ] {
            assert!(SystemConfig::resolve(&args(&a), dir.path(), &env(dir.path())).is_err());
        }
        let mut a = Vec::new();
        for i in 0..17 {
            a.push(OsString::from("--root"));
            a.push(format!("pi={i}").into());
        }
        assert!(SystemConfig::resolve(&a, dir.path(), &env(dir.path())).is_err());
    }
    #[test]
    fn selected_database_precedence_and_containment() {
        let dir = tempfile::tempdir().unwrap();
        let mut e = env(dir.path());
        e.insert("XDG_DATA_HOME".into(), "data".into());
        e.insert("OPENCODE_DB".into(), "channel.db".into());
        let c = SystemConfig::resolve(&[], dir.path(), &e).unwrap();
        assert!(
            c.stores
                .iter()
                .any(|s| s.path == dir.path().join("data/opencode/channel.db"))
        );
        e.insert("OPENCODE_DB".into(), "../escape.db".into());
        assert!(SystemConfig::resolve(&[], dir.path(), &e).is_err());
        e.insert(
            "OPENCODE_DB".into(),
            dir.path().join("outside.db").to_string_lossy().into_owned(),
        );
        assert!(SystemConfig::resolve(&[], dir.path(), &e).is_ok());
        e.insert("OPENCODE_DB".into(), ":memory:".into());
        assert!(SystemConfig::resolve(&[], dir.path(), &e).is_err());
        assert!(
            SystemConfig::resolve(&args(&["--disable-provider", "opencode"]), dir.path(), &e)
                .is_ok()
        );
    }
    #[cfg(unix)]
    #[test]
    fn canonical_aliases_and_relative_database_symlink_escape() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("real")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("real"), dir.path().join("alias")).unwrap();
        let c = SystemConfig::resolve(
            &args(&["--root", "pi=real", "--root", "pi=alias"]),
            dir.path(),
            &env(dir.path()),
        )
        .unwrap();
        assert_eq!(
            c.stores
                .iter()
                .filter(|s| s.provider == Provider::Pi)
                .count(),
            1
        );
        std::fs::create_dir_all(dir.path().join("data/opencode")).unwrap();
        std::os::unix::fs::symlink(
            dir.path().join("real"),
            dir.path().join("data/opencode/escape"),
        )
        .unwrap();
        let mut e = env(dir.path());
        e.insert("XDG_DATA_HOME".into(), "data".into());
        e.insert("OPENCODE_DB".into(), "escape/future.db".into());
        assert!(SystemConfig::resolve(&[], dir.path(), &e).is_err());
    }
    #[test]
    fn fully_resolved_launcher_configuration_does_not_require_home() {
        let dir = tempfile::tempdir().unwrap();
        let config = SystemConfig::resolve(
            &args(&[
                "--root",
                "pi=selected",
                "--disable-provider",
                "codex",
                "--disable-provider",
                "claude",
                "--disable-provider",
                "opencode",
            ]),
            dir.path(),
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(config.stores.len(), 1);
        assert_eq!(config.stores[0].path, dir.path().join("selected"));
    }
    #[test]
    fn memory_literal_is_only_special_for_opencode() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(":memory:")).unwrap();
        for provider in ["pi", "codex", "claude"] {
            let root = format!("{provider}=:memory:");
            let config =
                SystemConfig::resolve(&args(&["--root", &root]), dir.path(), &env(dir.path()))
                    .unwrap();
            assert!(
                config
                    .stores
                    .iter()
                    .any(|s| s.provider.as_str() == provider
                        && s.path == dir.path().join(":memory:"))
            );
        }
        assert!(
            SystemConfig::resolve(
                &args(&["--root", "opencode=:memory:"]),
                dir.path(),
                &env(dir.path())
            )
            .is_err()
        );
    }
}
