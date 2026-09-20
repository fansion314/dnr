//! Reviewed build inputs. Globs and host source paths never reach the runtime.
use anyhow::{Context, Result, ensure};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub os: String,
    pub arch: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub libc: Option<String>,
}
impl Target {
    pub fn host() -> Self {
        Self {
            os: if cfg!(target_os = "macos") {
                "darwin"
            } else {
                std::env::consts::OS
            }
            .into(),
            arch: if cfg!(target_arch = "aarch64") {
                "arm64"
            } else if cfg!(target_arch = "x86_64") {
                "x64"
            } else {
                std::env::consts::ARCH
            }
            .into(),
            libc: if cfg!(target_os = "linux") {
                Some(
                    if cfg!(target_env = "musl") {
                        "musl"
                    } else {
                        "glibc"
                    }
                    .into(),
                )
            } else {
                None
            },
        }
    }
    pub fn id(&self) -> String {
        format!(
            "{}_{}{}",
            self.os,
            self.arch,
            self.libc
                .as_ref()
                .map(|l| format!("_{l}"))
                .unwrap_or_default()
        )
    }
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            matches!(self.os.as_str(), "darwin" | "linux" | "win32"),
            "unsupported platform name: {}",
            self.os
        );
        ensure!(
            matches!(self.arch.as_str(), "x64" | "arm64"),
            "unsupported architecture: {}",
            self.arch
        );
        ensure!(
            if self.os == "linux" {
                matches!(self.libc.as_deref(), Some("glibc" | "musl"))
            } else {
                self.libc.is_none()
            },
            "Linux targets must specify glibc or musl; other targets must omit libc"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackageConfig {
    pub schema_version: u32,
    #[serde(default)]
    pub targets: BTreeMap<String, Target>,
    #[serde(default)]
    pub groups: Vec<Group>,
    #[serde(skip)]
    pub base_dir: PathBuf,
}
impl Default for PackageConfig {
    fn default() -> Self {
        Self {
            schema_version: 1,
            targets: BTreeMap::new(),
            groups: vec![],
            base_dir: PathBuf::from("."),
        }
    }
}
impl PackageConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let mut value: Self =
            serde_json::from_slice(&fs::read(path)?).context("invalid package configuration")?;
        value.base_dir = path.canonicalize()?.parent().unwrap().to_owned();
        value.validate()?;
        Ok(value)
    }
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1,
            "unsupported package configuration version"
        );
        let mut seen = std::collections::HashSet::new();
        for (id, target) in &self.targets {
            identifier(id)?;
            target.validate()?;
            ensure!(
                seen.insert(target.id()),
                "duplicate target definition: {id}"
            );
        }
        seen.clear();
        for group in &self.groups {
            identifier(&group.id)?;
            ensure!(
                seen.insert(group.id.clone()),
                "duplicate group: {}",
                group.id
            );
            patterns(&group.files)?;
            patterns(&group.native.executables)?;
            patterns(&group.native.libraries)?;
            for addon in &group.native.addons {
                ensure!(
                    crate::normalized_name(&addon.path)? == addon.path && addon.napi > 0,
                    "invalid Node-API declaration: {}",
                    addon.path
                );
            }
            for (target, mappings) in &group.variants {
                ensure!(
                    self.targets.contains_key(target),
                    "unknown target: {target}"
                );
                for mapping in mappings {
                    ensure!(
                        crate::normalized_name(&mapping.to)? == mapping.to
                            && !mapping.to.starts_with(".dnr/"),
                        "invalid variant destination"
                    );
                }
            }
        }
        ensure!(
            self.groups.is_empty() || !self.targets.is_empty(),
            "native groups require explicit targets"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub id: String,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub variants: BTreeMap<String, Vec<Mapping>>,
    #[serde(default)]
    pub native: Native,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mapping {
    pub from: PathBuf,
    pub to: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Native {
    #[serde(default)]
    pub executables: Vec<String>,
    #[serde(default)]
    pub libraries: Vec<String>,
    #[serde(default)]
    pub addons: Vec<Addon>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Addon {
    pub path: String,
    pub napi: u32,
}

pub(crate) fn identifier(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 100
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b)),
        "invalid identifier: {value}"
    );
    Ok(())
}
pub(crate) fn patterns(values: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for value in values {
        ensure!(
            crate::normalized_name(value)? == *value && !value.starts_with(".dnr/"),
            "invalid pattern: {value}"
        );
        builder.add(GlobBuilder::new(value).literal_separator(true).build()?);
    }
    Ok(builder.build()?)
}

#[derive(Clone, Debug)]
pub(crate) struct Detection {
    pub kind: &'static str,
    pub os: Option<&'static str>,
    pub arch: Option<&'static str>,
}
pub(crate) fn detect(path: &Path) -> Result<Option<Detection>> {
    let mut file = fs::File::open(path)?;
    let mut header = [0u8; 64];
    let n = file.read(&mut header)?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let kind = if name.ends_with(".node") {
        "addon"
    } else if name.ends_with(".dll")
        || name.ends_with(".dylib")
        || name.ends_with(".so")
        || name.contains(".so.")
    {
        "library"
    } else {
        "executable"
    };
    let mut detection = Detection {
        kind,
        os: None,
        arch: None,
    };
    if n >= 20 && &header[..4] == b"\x7fELF" {
        detection.os = Some("linux");
        let machine = if header[5] == 1 {
            u16::from_le_bytes([header[18], header[19]])
        } else {
            u16::from_be_bytes([header[18], header[19]])
        };
        detection.arch = match machine {
            62 => Some("x64"),
            183 => Some("arm64"),
            _ => None,
        };
    } else if n >= 8
        && matches!(
            &header[..4],
            [0xcf, 0xfa, 0xed, 0xfe] | [0xce, 0xfa, 0xed, 0xfe]
        )
    {
        detection.os = Some("darwin");
        detection.arch = match u32::from_le_bytes(header[4..8].try_into().unwrap()) {
            0x1000007 => Some("x64"),
            0x100000c => Some("arm64"),
            _ => None,
        };
    } else if n >= 8
        && matches!(
            &header[..4],
            [0xca, 0xfe, 0xba, 0xbe] | [0xca, 0xfe, 0xba, 0xbf]
        )
    {
        let count = u32::from_be_bytes(header[4..8].try_into().unwrap());
        // Java class files share CAFEBABE, but their version field is not a fat-architecture count.
        if count == 0 || count > 64 {
            return Ok(None);
        }
        detection.os = Some("darwin");
    } else if n >= 64 && &header[..2] == b"MZ" {
        let offset = u32::from_le_bytes(header[60..64].try_into().unwrap());
        file.seek(SeekFrom::Start(offset.into()))?;
        let mut pe = [0; 6];
        if file.read_exact(&mut pe).is_err() || &pe[..4] != b"PE\0\0" {
            return Ok(None);
        }
        detection.os = Some("win32");
        detection.arch = match u16::from_le_bytes([pe[4], pe[5]]) {
            0x8664 => Some("x64"),
            0xaa64 => Some("arm64"),
            _ => None,
        };
    } else {
        #[cfg(unix)]
        let executable = {
            use std::os::unix::fs::PermissionsExt;
            file.metadata()?.permissions().mode() & 0o111 != 0
        };
        #[cfg(not(unix))]
        let executable = false;
        if kind == "executable" && !executable && !(n >= 2 && &header[..2] == b"#!") {
            return Ok(None);
        }
    }
    Ok(Some(detection))
}

/// Produce a proposal only. In particular, a scan does not claim to infer ABI or resource closure.
pub fn scan(directory: &Path) -> Result<(PackageConfig, Vec<String>)> {
    let mut entries = BTreeMap::new();
    crate::collect(
        directory,
        "",
        &mut entries,
        &[],
        Path::new("/nonexistent-dnr-scan-output"),
    )?;
    let mut groups: BTreeMap<String, Group> = BTreeMap::new();
    let mut metadata: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    let mut targets = BTreeMap::new();
    let mut notes = vec!["Review group resources, libc and Node-API versions before packaging. Scanning does not execute files or collect shared-library dependencies.".into()];
    for (name, source) in &entries {
        if !fs::symlink_metadata(source)?.is_file() {
            continue;
        }
        let components: Vec<_> = name.split('/').collect();
        let root = components
            .iter()
            .rposition(|s| *s == "node_modules")
            .map(|i| {
                let end = if components.get(i + 1).is_some_and(|s| s.starts_with('@')) {
                    i + 3
                } else {
                    i + 2
                };
                components[..end.min(components.len())].join("/")
            })
            .unwrap_or_else(|| {
                Path::new(name)
                    .parent()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            });
        let package = metadata.entry(root.clone()).or_insert_with(|| {
            fs::read(directory.join(&root).join("package.json"))
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or(serde_json::Value::Null)
        });
        let bin = package.get("bin");
        let is_bin = bin
            .into_iter()
            .flat_map(|value| {
                if let Some(object) = value.as_object() {
                    object
                        .values()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                } else {
                    value.as_str().into_iter().collect()
                }
            })
            .any(|bin| crate::normalized_name(&format!("{root}/{bin}")).is_ok_and(|p| p == *name));
        let found = detect(source)?.or_else(|| {
            is_bin.then_some(Detection {
                kind: "executable",
                os: None,
                arch: None,
            })
        });
        let Some(found) = found else {
            continue;
        };
        let next = groups.len() + 1;
        let group = groups.entry(root.clone()).or_insert_with(|| Group {
            id: format!("native_{next}"),
            files: vec![if root.is_empty() {
                "**".into()
            } else {
                format!("{root}/**")
            }],
            variants: BTreeMap::new(),
            native: Native::default(),
        });
        match found.kind {
            "addon" => {
                group.native.addons.push(Addon {
                    path: name.clone(),
                    napi: 1,
                });
                notes.push(format!(
                    "{name}: replace the suggested napi=1 with the producer's actual requirement"
                ));
            }
            "library" => group.native.libraries.push(name.clone()),
            _ => group.native.executables.push(name.clone()),
        }
        let host = Target::host();
        let one = |key: &str| -> Option<&str> {
            let value = package.get(key)?;
            value
                .as_str()
                .or_else(|| {
                    value
                        .as_array()
                        .filter(|v| v.len() == 1)
                        .and_then(|v| v[0].as_str())
                })
                .filter(|v| !v.starts_with('!'))
        };
        let os = found
            .os
            .or_else(|| one("os"))
            .unwrap_or(&host.os)
            .to_owned();
        let target = Target {
            arch: found
                .arch
                .or_else(|| one("cpu"))
                .unwrap_or(&host.arch)
                .into(),
            libc: if os == "linux" {
                Some(
                    one("libc")
                        .or(host.libc.as_deref())
                        .unwrap_or("glibc")
                        .into(),
                )
            } else {
                None
            },
            os,
        };
        targets.insert(target.id(), target);
        notes.push(format!(
            "{name}: {} candidate, group {}",
            found.kind, group.id
        ));
    }
    if targets.len() > 1 {
        notes.push("Multiple targets found: move platform-specific members into variants; common files must work on every declared target.".into());
    }
    Ok((
        PackageConfig {
            targets,
            groups: groups.into_values().collect(),
            ..Default::default()
        },
        notes,
    ))
}
