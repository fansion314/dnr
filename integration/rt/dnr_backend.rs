//! Parse host options only before the entry point; application arguments are opaque.
pub fn take_backend(args: &mut Vec<String>) -> Result<String, String> {
    let mut backend = None;
    let mut i = 0;
    while i < args.len() {
        let value = if args[i] == "--backend" {
            if i + 1 == args.len() {
                return Err("--backend requires auto, system-cef or webview".into());
            }
            let value = args.remove(i + 1);
            args.remove(i);
            value
        } else if let Some(value) = args[i].strip_prefix("--backend=") {
            let value = value.to_owned();
            args.remove(i);
            value
        } else if matches!(args[i].as_str(), "--no-code-cache" | "--no-transpile-cache") {
            i += 1;
            continue;
        } else {
            break;
        };
        if !matches!(value.as_str(), "auto" | "system-cef" | "webview") {
            return Err(format!(
                "unknown backend {value:?}; expected auto, system-cef or webview"
            ));
        }
        if backend.replace(value).is_some() {
            return Err("--backend may only be specified once".into());
        }
    }
    Ok(backend.unwrap_or_else(|| "auto".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn preserves_application_arguments() {
        for input in [
            args(&["app.ts", "--backend", "webview", "--type=renderer"]),
            args(&[
                "--package",
                "app.dnp",
                "--entry",
                "main.ts",
                "--",
                "--backend=webview",
            ]),
            args(&["tree", "--backend=webview"]),
        ] {
            let mut actual = input.clone();
            assert_eq!(take_backend(&mut actual).unwrap(), "auto");
            assert_eq!(actual, input);
        }
    }

    #[test]
    fn selects_backend_among_cache_options() {
        let mut input = args(&[
            "--no-code-cache",
            "--backend",
            "webview",
            "--no-transpile-cache",
            "app.ts",
            "--backend=other",
        ]);
        assert_eq!(take_backend(&mut input).unwrap(), "webview");
        assert_eq!(
            input,
            args(&[
                "--no-code-cache",
                "--no-transpile-cache",
                "app.ts",
                "--backend=other"
            ])
        );
        for value in ["auto", "webview", "system-cef"] {
            let mut input = vec![
                format!("--backend={value}"),
                "--package".into(),
                "app.dnp".into(),
            ];
            assert_eq!(take_backend(&mut input).unwrap(), value);
            assert_eq!(input, args(&["--package", "app.dnp"]));
        }
    }

    #[test]
    fn rejects_invalid_or_repeated_options() {
        for values in [
            vec!["--backend"],
            vec!["--backend="],
            vec!["--backend", "dual"],
            vec!["--backend=auto", "--backend=webview"],
        ] {
            assert!(take_backend(&mut args(&values)).is_err());
        }
    }
}
