use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    env, fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const WRAPPER_ICON: &[u8] = include_bytes!("../resources/cloneonce.icns");

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInspection {
    path: String,
    name: String,
    bundle_id: String,
    executable_path: Option<String>,
    icon_file: Option<String>,
    sandboxed: bool,
    compatibility: String,
    preset: String,
    valid: bool,
    warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerateShortcutConfig {
    shortcut_type: String,
    name: String,
    target_path: String,
    executable_path: Option<String>,
    bundle_id: String,
    data_path: String,
    output_dir: Option<String>,
    launch_mode: String,
    data_strategy: String,
    command_arguments: String,
    environment: String,
    erase_on_close: bool,
    menu_bar_control: bool,
    dedicated_dock_icon: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeneratedShortcut {
    name: String,
    path: String,
    bundle_id: String,
    config_path: String,
    launcher_path: String,
    data_path: String,
    created: bool,
}

#[tauri::command]
fn inspect_app_bundle(path: String) -> Result<AppInspection, String> {
    inspect_app_bundle_at(Path::new(&path))
}

#[tauri::command]
fn generate_shortcut(config: GenerateShortcutConfig) -> Result<GeneratedShortcut, String> {
    let shortcut_name = config.name.trim();
    if shortcut_name.is_empty() {
        return Err("Shortcut name is required.".into());
    }

    let target_path = PathBuf::from(config.target_path.trim());
    if config.shortcut_type != "command" && !target_path.exists() {
        return Err("Target path does not exist.".into());
    }
    if config.shortcut_type == "command" && !target_path.exists() {
        return Err("Command executable does not exist.".into());
    }

    let output_dir = config
        .output_dir
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_shortcuts_dir);
    fs::create_dir_all(&output_dir).map_err(|err| {
        format!(
            "Could not create output folder {}: {err}",
            output_dir.display()
        )
    })?;

    let bundle_path = output_dir.join(format!("{}.app", sanitize_file_name(shortcut_name)));
    if bundle_path.exists() {
        return Err(format!(
            "{} already exists. Choose a different shortcut name or output folder.",
            bundle_path.display()
        ));
    }

    let contents_dir = bundle_path.join("Contents");
    let macos_dir = contents_dir.join("MacOS");
    let resources_dir = contents_dir.join("Resources");
    fs::create_dir_all(&macos_dir).map_err(|err| err.to_string())?;
    fs::create_dir_all(&resources_dir).map_err(|err| err.to_string())?;

    let launcher_path = macos_dir.join("launch");
    let config_path = resources_dir.join("config.json");
    let icon_path = resources_dir.join("AppIcon.icns");
    fs::write(&icon_path, WRAPPER_ICON).map_err(|err| err.to_string())?;

    let app_inspection = if is_app_bundle(&target_path) {
        inspect_app_bundle_at(&target_path).ok()
    } else {
        None
    };
    let executable_path = resolve_executable_path(&config, app_inspection.as_ref())?;
    let bundle_id = if config.bundle_id.trim().is_empty() {
        format!("app.cloneonce.generated.{}", slugify(shortcut_name))
    } else {
        config.bundle_id.trim().to_string()
    };
    let data_path = resolve_data_path(&config, shortcut_name);
    let env_vars = parse_environment(&config.environment)?;
    let custom_args = parse_argument_lines(&config.command_arguments);
    let preset = app_inspection
        .as_ref()
        .map(|item| item.preset.as_str())
        .unwrap_or("Generic");

    fs::write(
        contents_dir.join("Info.plist"),
        info_plist(shortcut_name, &bundle_id),
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        &config_path,
        serde_json::to_string_pretty(&json!({
            "name": shortcut_name,
            "targetPath": target_path,
            "executablePath": executable_path,
            "bundleId": bundle_id,
            "dataPath": data_path,
            "shortcutType": config.shortcut_type,
            "launchMode": config.launch_mode,
            "dataStrategy": config.data_strategy,
            "eraseOnClose": config.erase_on_close,
            "menuBarControl": config.menu_bar_control,
            "dedicatedDockIcon": config.dedicated_dock_icon,
            "environment": env_vars,
            "arguments": custom_args,
        }))
        .map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;

    fs::write(
        &launcher_path,
        launcher_script(
            &config,
            &target_path,
            executable_path.as_deref(),
            &data_path,
            preset,
            &custom_args,
            &env_vars,
        ),
    )
    .map_err(|err| err.to_string())?;
    let mut permissions = fs::metadata(&launcher_path)
        .map_err(|err| err.to_string())?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&launcher_path, permissions).map_err(|err| err.to_string())?;
    ad_hoc_sign_bundle(&bundle_path)?;

    Ok(GeneratedShortcut {
        name: shortcut_name.to_string(),
        path: bundle_path.display().to_string(),
        bundle_id,
        config_path: config_path.display().to_string(),
        launcher_path: launcher_path.display().to_string(),
        data_path,
        created: true,
    })
}

#[tauri::command]
fn run_shortcut(path: String) -> Result<(), String> {
    run_open_command(["--", path.as_str()])
}

#[tauri::command]
fn reveal_in_finder(path: String) -> Result<(), String> {
    run_open_command(["-R", path.as_str()])
}

fn run_open_command<const N: usize>(args: [&str; N]) -> Result<(), String> {
    let status = Command::new("/usr/bin/open")
        .args(args)
        .status()
        .map_err(|err| format!("Could not run open: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("open exited with status {status}"))
    }
}

fn inspect_app_bundle_at(path: &Path) -> Result<AppInspection, String> {
    let mut warnings = Vec::new();
    if !is_app_bundle(path) {
        return Ok(AppInspection {
            path: path.display().to_string(),
            name: path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("Unknown")
                .to_string(),
            bundle_id: String::new(),
            executable_path: None,
            icon_file: None,
            sandboxed: false,
            compatibility: "unknown".into(),
            preset: "File or command".into(),
            valid: false,
            warnings: vec!["Selected item is not a macOS .app bundle.".into()],
        });
    }

    let plist_path = path.join("Contents/Info.plist");
    let plist = read_plist_json(&plist_path)?;
    let name = plist_string(&plist, "CFBundleDisplayName")
        .or_else(|| plist_string(&plist, "CFBundleName"))
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("Unknown App")
                .to_string()
        });
    let bundle_id = plist_string(&plist, "CFBundleIdentifier").unwrap_or_default();
    let executable_name = plist_string(&plist, "CFBundleExecutable");
    let executable_path = executable_name
        .as_deref()
        .map(|name| path.join("Contents/MacOS").join(name));
    let executable_exists = executable_path.as_ref().is_some_and(|item| item.exists());
    if executable_name.is_none() {
        warnings.push("Info.plist does not declare CFBundleExecutable.".into());
    } else if !executable_exists {
        warnings.push("Declared executable could not be found.".into());
    }

    let icon_file = plist_string(&plist, "CFBundleIconFile");
    let entitlements = read_code_sign_entitlements(path);
    let sandboxed = detect_sandbox(&entitlements);
    let (compatibility, preset) = infer_compatibility(&name, &bundle_id, sandboxed, &plist);
    if sandboxed {
        warnings.push("Sandboxed targets may not support custom HOME data redirection.".into());
    }
    warnings.extend(entitlement_warnings(&entitlements));
    if compatibility == "unknown" {
        warnings
            .push("No app-specific preset found. Start with HOME override or command mode.".into());
    }

    Ok(AppInspection {
        path: path.display().to_string(),
        name,
        bundle_id,
        executable_path: executable_path.map(|item| item.display().to_string()),
        icon_file,
        sandboxed,
        compatibility,
        preset,
        valid: executable_exists,
        warnings,
    })
}

fn read_plist_json(path: &Path) -> Result<Value, String> {
    let output = Command::new("/usr/bin/plutil")
        .args([
            "-convert",
            "json",
            "-o",
            "-",
            path.to_string_lossy().as_ref(),
        ])
        .output()
        .map_err(|err| format!("Could not run plutil: {err}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|err| format!("Could not parse Info.plist: {err}"))
}

fn plist_string(plist: &Value, key: &str) -> Option<String> {
    plist
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn read_code_sign_entitlements(path: &Path) -> Result<Value, String> {
    let output = Command::new("/usr/bin/codesign")
        .args([
            "-d",
            "--entitlements",
            "-",
            "--xml",
            path.to_string_lossy().as_ref(),
        ])
        .output()
        .map_err(|err| format!("Could not run codesign: {err}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if detail.is_empty() {
            format!("codesign exited with status {}", output.status)
        } else {
            detail
        });
    }
    parse_plist_json_bytes(&output.stdout, "code-signing entitlements")
}

fn parse_plist_json_bytes(bytes: &[u8], description: &str) -> Result<Value, String> {
    let mut child = Command::new("/usr/bin/plutil")
        .args(["-convert", "json", "-o", "-", "--", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("Could not run plutil for {description}: {err}"))?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| format!("Could not open plutil input for {description}."))?
        .write_all(bytes)
        .map_err(|err| format!("Could not send {description} to plutil: {err}"))?;

    let output = child
        .wait_with_output()
        .map_err(|err| format!("Could not read plutil output for {description}: {err}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if detail.is_empty() {
            format!("Could not parse {description}.")
        } else {
            format!("Could not parse {description}: {detail}")
        });
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|err| format!("Could not parse {description}: {err}"))
}

fn detect_sandbox(entitlements: &Result<Value, String>) -> bool {
    entitlements
        .as_ref()
        .ok()
        .and_then(|plist| plist.get("com.apple.security.app-sandbox"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn uses_shared_security_groups(entitlements: &Result<Value, String>) -> bool {
    entitlements.as_ref().ok().is_some_and(|plist| {
        [
            "keychain-access-groups",
            "com.apple.security.application-groups",
        ]
        .iter()
        .any(|key| plist.get(*key).is_some_and(entitlement_has_values))
    })
}

fn entitlement_has_values(value: &Value) -> bool {
    match value {
        Value::Array(items) => !items.is_empty(),
        Value::String(item) => !item.trim().is_empty(),
        Value::Null => false,
        _ => true,
    }
}

fn entitlement_warnings(entitlements: &Result<Value, String>) -> Vec<String> {
    match entitlements {
        Err(error) => vec![format!(
            "Code-signing entitlements could not be inspected. Sandbox status and shared login storage could not be confirmed: {error}"
        )],
        Ok(_) if uses_shared_security_groups(entitlements) => vec![
            "Target uses shared Keychain or app groups. CloneOnce can isolate files and profiles, but a wrapper cannot guarantee a separate login."
                .into(),
        ],
        Ok(_) => Vec::new(),
    }
}

fn infer_compatibility(
    name: &str,
    bundle_id: &str,
    sandboxed: bool,
    plist: &Value,
) -> (String, String) {
    let haystack = format!("{} {}", name, bundle_id).to_lowercase();
    if sandboxed {
        return ("limited".into(), "Sandboxed app container".into());
    }
    let looks_like_electron_runtime = plist.get("ElectronAsarIntegrity").is_some();
    let looks_like_chromium_runtime = looks_like_electron_runtime
        || plist.get("ChromiumBaseVersion").is_some()
        || plist.get("ChromiumBaseBundleVersion").is_some()
        || plist.get("CrProductDirName").is_some();
    if looks_like_electron_runtime {
        return (
            "supported".into(),
            "Electron isolated HOME + --user-data-dir".into(),
        );
    }
    if [
        "chrome", "chromium", "brave", "edge", "vivaldi", "opera", "arc",
    ]
    .iter()
    .any(|needle| haystack.contains(needle))
        || looks_like_chromium_runtime
    {
        return ("supported".into(), "Chromium --user-data-dir".into());
    }
    if ["firefox", "librewolf", "waterfox"]
        .iter()
        .any(|needle| haystack.contains(needle))
    {
        return ("supported".into(), "Firefox -profile".into());
    }
    ("unknown".into(), "Generic HOME override".into())
}

fn resolve_executable_path(
    config: &GenerateShortcutConfig,
    inspection: Option<&AppInspection>,
) -> Result<Option<String>, String> {
    if config.shortcut_type == "fileFolder" {
        return Ok(None);
    }

    let candidate = config
        .executable_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| inspection.and_then(|item| item.executable_path.clone()))
        .unwrap_or_else(|| config.target_path.trim().to_string());

    if !Path::new(&candidate).exists() {
        return Err(format!("Executable path does not exist: {candidate}"));
    }
    Ok(Some(candidate))
}

fn resolve_data_path(config: &GenerateShortcutConfig, shortcut_name: &str) -> String {
    if config.shortcut_type == "fileFolder" || config.data_strategy == "none" {
        return String::new();
    }
    let trimmed = config.data_path.trim();
    if !trimmed.is_empty() {
        return expand_home(trimmed);
    }
    default_profile_dir(shortcut_name).display().to_string()
}

fn default_shortcuts_dir() -> PathBuf {
    home_dir().join("Applications/CloneOnce Shortcuts")
}

fn default_profile_dir(shortcut_name: &str) -> PathBuf {
    home_dir()
        .join("CloneOnce/Profiles")
        .join(sanitize_file_name(shortcut_name))
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn expand_home(value: &str) -> String {
    if value == "~" {
        return home_dir().display().to_string();
    }
    value
        .strip_prefix("~/")
        .map(|tail| home_dir().join(tail).display().to_string())
        .unwrap_or_else(|| value.to_string())
}

fn parse_argument_lines(input: &str) -> Vec<String> {
    input
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(ToOwned::to_owned)
        .collect()
}

fn parse_environment(input: &str) -> Result<BTreeMap<String, String>, String> {
    let mut env_vars = BTreeMap::new();
    for item in input
        .split(['\n', ';'])
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let (key, value) = item
            .split_once('=')
            .ok_or_else(|| format!("Environment entry must be KEY=VALUE: {item}"))?;
        let key = key.trim();
        if !is_valid_env_key(key) {
            return Err(format!("Invalid environment key: {key}"));
        }
        env_vars.insert(key.to_string(), value.trim().trim_matches('"').to_string());
    }
    Ok(env_vars)
}

fn is_valid_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn launcher_script(
    config: &GenerateShortcutConfig,
    target_path: &Path,
    executable_path: Option<&str>,
    data_path: &str,
    preset: &str,
    custom_args: &[String],
    env_vars: &BTreeMap<String, String>,
) -> String {
    if config.shortcut_type == "fileFolder" {
        return format!(
            "#!/bin/zsh\nset -u\n/usr/bin/open -- {}\n",
            shell_quote(&target_path.display().to_string())
        );
    }

    let mut script = String::from("#!/bin/zsh\nset -u\n\n");
    script.push_str(&format!(
        "TARGET={}\n",
        shell_quote(executable_path.unwrap_or(""))
    ));
    script.push_str(&format!("DATA_PATH={}\n", shell_quote(data_path)));
    script.push_str("ARGS=()\n\n");

    let strategy = config.data_strategy.as_str();
    let should_create_data = strategy != "none" && !data_path.trim().is_empty();
    let uses_system_open =
        config.launch_mode == "systemLaunch" && config.shortcut_type != "command";
    if uses_system_open {
        script.push_str("OPEN_ENV_ARGS=()\n");
    }
    if should_create_data {
        script.push_str("mkdir -p -- \"$DATA_PATH\"\n");
    }

    if strategy == "automatic" || strategy == "browserProfile" {
        let has_profile_arg = custom_args.iter().any(|arg| {
            arg.contains("user-data-dir") || arg == "-profile" || arg.starts_with("-profile ")
        });
        let preset_lower = preset.to_lowercase();
        if preset_lower.contains("electron") {
            script.push_str("HOME_PATH=\"$DATA_PATH/Home\"\n");
            script.push_str("BROWSER_PROFILE=\"$DATA_PATH/Browser\"\n");
            script.push_str("mkdir -p -- \"$HOME_PATH\" \"$HOME_PATH/.config\" \"$HOME_PATH/.cache\" \"$HOME_PATH/.local/share\" \"$HOME_PATH/.codex\" \"$BROWSER_PROFILE\"\n");
            script.push_str("export HOME=\"$HOME_PATH\"\n");
            script.push_str("export XDG_CONFIG_HOME=\"$HOME_PATH/.config\"\n");
            script.push_str("export XDG_CACHE_HOME=\"$HOME_PATH/.cache\"\n");
            script.push_str("export XDG_DATA_HOME=\"$HOME_PATH/.local/share\"\n");
            script.push_str("export CODEX_HOME=\"$HOME_PATH/.codex\"\n");
            if uses_system_open {
                script.push_str("OPEN_ENV_ARGS+=(--env \"HOME=$HOME_PATH\")\n");
                script.push_str("OPEN_ENV_ARGS+=(--env \"XDG_CONFIG_HOME=$HOME_PATH/.config\")\n");
                script.push_str("OPEN_ENV_ARGS+=(--env \"XDG_CACHE_HOME=$HOME_PATH/.cache\")\n");
                script
                    .push_str("OPEN_ENV_ARGS+=(--env \"XDG_DATA_HOME=$HOME_PATH/.local/share\")\n");
                script.push_str("OPEN_ENV_ARGS+=(--env \"CODEX_HOME=$HOME_PATH/.codex\")\n");
            }
            if !has_profile_arg {
                script.push_str("ARGS+=(\"--user-data-dir=$BROWSER_PROFILE\")\n");
            }
        } else if !has_profile_arg && preset_lower.contains("firefox") {
            script.push_str("ARGS+=(\"-profile\" \"$DATA_PATH\" \"-new-instance\")\n");
        } else if !has_profile_arg {
            script.push_str("ARGS+=(\"--user-data-dir=$DATA_PATH\")\n");
        }
    }

    if strategy == "homeOverride" {
        script.push_str("mkdir -p -- \"$DATA_PATH/Library/Application Support\" \"$DATA_PATH/Desktop\" \"$DATA_PATH/Documents\"\n");
        script.push_str("export HOME=\"$DATA_PATH\"\n");
        if uses_system_open {
            script.push_str("OPEN_ENV_ARGS+=(--env \"HOME=$DATA_PATH\")\n");
        }
    }

    for (key, value) in env_vars {
        script.push_str(&format!("export {}={}\n", key, shell_quote(value)));
        if uses_system_open {
            script.push_str(&format!(
                "OPEN_ENV_ARGS+=(--env {})\n",
                shell_quote(&format!("{key}={value}"))
            ));
        }
    }
    for arg in custom_args {
        let expanded = arg
            .replace("${DATA_PATH}", data_path)
            .replace("$DATA_PATH", data_path);
        script.push_str(&format!("ARGS+=({})\n", shell_quote(&expanded)));
    }

    if uses_system_open {
        script.push_str("\nif (( ${#ARGS[@]} )); then\n");
        script.push_str(&format!(
            "  /usr/bin/open -n \"${{OPEN_ENV_ARGS[@]}}\" {} --args \"${{ARGS[@]}}\"\n",
            shell_quote(&target_path.display().to_string())
        ));
        script.push_str("else\n");
        script.push_str(&format!(
            "  /usr/bin/open -n \"${{OPEN_ENV_ARGS[@]}}\" {}\n",
            shell_quote(&target_path.display().to_string())
        ));
        script.push_str("fi\n");
        script.push_str("STATUS=$?\n");
        if config.erase_on_close && should_create_data {
            script.push_str("# System Launch Mode does not wait for app exit, so erase-on-close is skipped here.\n");
        }
        script.push_str("exit \"$STATUS\"\n");
        return script;
    }

    script.push_str("\n\"$TARGET\" \"${ARGS[@]}\" &\n");
    script.push_str("PID=$!\nwait \"$PID\"\nSTATUS=$?\n");
    if config.erase_on_close && should_create_data {
        script.push_str("rm -rf -- \"$DATA_PATH\"\n");
    }
    script.push_str("exit \"$STATUS\"\n");
    script
}

fn info_plist(name: &str, bundle_id: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleExecutable</key>
  <string>launch</string>
  <key>CFBundleIconFile</key>
  <string>AppIcon</string>
  <key>CFBundleIdentifier</key>
  <string>{}</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>{}</string>
  <key>CFBundleDisplayName</key>
  <string>{}</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>LSMinimumSystemVersion</key>
  <string>10.15</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
"#,
        xml_escape(bundle_id),
        xml_escape(name),
        xml_escape(name)
    )
}

fn is_app_bundle(path: &Path) -> bool {
    path.is_dir()
        && path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("app"))
}

fn sanitize_file_name(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == ' ' {
                ch
            } else {
                '-'
            }
        })
        .collect();
    sanitized.trim().trim_matches('.').to_string()
}

fn slugify(value: &str) -> String {
    let slug: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    slug.split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".into();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn ad_hoc_sign_bundle(bundle_path: &Path) -> Result<(), String> {
    let output = Command::new("/usr/bin/codesign")
        .args([
            "--force",
            "--deep",
            "--sign",
            "-",
            bundle_path.to_string_lossy().as_ref(),
        ])
        .output()
        .map_err(|err| format!("Could not run codesign: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Could not sign generated app: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            inspect_app_bundle,
            generate_shortcut,
            run_shortcut,
            reveal_in_finder
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_config() -> GenerateShortcutConfig {
        GenerateShortcutConfig {
            shortcut_type: "appInstance".into(),
            name: "Test Shortcut".into(),
            target_path: "/Applications/Test.app".into(),
            executable_path: Some("/Applications/Test.app/Contents/MacOS/Test".into()),
            bundle_id: "app.cloneonce.test".into(),
            data_path: "/tmp/cloneonce-test-profile".into(),
            output_dir: None,
            launch_mode: "dockShortcut".into(),
            data_strategy: "automatic".into(),
            command_arguments: String::new(),
            environment: String::new(),
            erase_on_close: false,
            menu_bar_control: false,
            dedicated_dock_icon: true,
        }
    }

    #[test]
    fn no_data_strategy_resolves_to_empty_data_path() {
        let mut config = base_config();
        config.data_strategy = "none".into();

        assert_eq!(resolve_data_path(&config, "Test Shortcut"), "");
    }

    #[test]
    fn system_launch_mode_uses_macos_open() {
        let mut config = base_config();
        config.launch_mode = "systemLaunch".into();
        config.data_strategy = "homeOverride".into();
        let env_vars = BTreeMap::from([("CLONEONCE_TEST".into(), "1".into())]);
        let args = vec!["--flag".into()];

        let script = launcher_script(
            &config,
            Path::new("/Applications/Test.app"),
            Some("/Applications/Test.app/Contents/MacOS/Test"),
            "/tmp/cloneonce-test-profile",
            "Generic HOME override",
            &args,
            &env_vars,
        );

        assert!(script.contains("/usr/bin/open -n"));
        assert!(script.contains("OPEN_ENV_ARGS+=(--env \"HOME=$DATA_PATH\")"));
        assert!(script.contains("OPEN_ENV_ARGS+=(--env 'CLONEONCE_TEST=1')"));
        assert!(script.contains("--args \"${ARGS[@]}\""));
        assert!(!script.contains("\"$TARGET\" \"${ARGS[@]}\""));
    }

    #[test]
    fn sandbox_detection_requires_a_true_boolean_entitlement() {
        let enabled = Ok(json!({ "com.apple.security.app-sandbox": true }));
        let disabled = Ok(json!({ "com.apple.security.app-sandbox": false }));
        let missing = Ok(json!({ "keychain-access-groups": ["TEAM.example"] }));
        let malformed = Ok(json!({ "com.apple.security.app-sandbox": "true" }));
        let read_error = Err("codesign failed".into());

        assert!(detect_sandbox(&enabled));
        assert!(!detect_sandbox(&disabled));
        assert!(!detect_sandbox(&missing));
        assert!(!detect_sandbox(&malformed));
        assert!(!detect_sandbox(&read_error));
    }

    #[test]
    fn false_sandbox_entitlement_keeps_electron_preset_supported() {
        let entitlements = Ok(json!({ "com.apple.security.app-sandbox": false }));
        let plist = json!({ "ElectronAsarIntegrity": {} });

        let sandboxed = detect_sandbox(&entitlements);
        let (compatibility, preset) =
            infer_compatibility("ChatGPT", "com.openai.codex", sandboxed, &plist);

        assert!(!sandboxed);
        assert_eq!(compatibility, "supported");
        assert_eq!(preset, "Electron isolated HOME + --user-data-dir");
    }

    #[test]
    fn shared_security_group_detection_covers_keychain_and_application_groups() {
        let keychain = Ok(json!({ "keychain-access-groups": ["TEAM.example"] }));
        let application = Ok(json!({ "com.apple.security.application-groups": ["TEAM.example"] }));
        let empty_groups = Ok(json!({
            "keychain-access-groups": [],
            "com.apple.security.application-groups": []
        }));

        assert!(uses_shared_security_groups(&keychain));
        assert!(uses_shared_security_groups(&application));
        assert!(!uses_shared_security_groups(&empty_groups));
    }

    #[test]
    fn sandbox_detection_parses_codesign_style_xml_plists() {
        let enabled = parse_plist_json_bytes(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "https://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>com.apple.security.app-sandbox</key><true/>
<key>keychain-access-groups</key><array><string>TEAM.example</string></array>
</dict></plist>"#,
            "test entitlements",
        );
        let disabled = parse_plist_json_bytes(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>com.apple.security.app-sandbox</key><false/>
</dict></plist>"#,
            "test entitlements",
        );
        let malformed = parse_plist_json_bytes(b"not a plist", "test entitlements");

        assert!(detect_sandbox(&enabled));
        assert!(uses_shared_security_groups(&enabled));
        assert!(entitlement_warnings(&enabled)
            .iter()
            .any(|warning| warning.contains("cannot guarantee a separate login")));
        assert!(!detect_sandbox(&disabled));
        assert!(malformed.is_err());
        assert!(!detect_sandbox(&malformed));
        assert!(entitlement_warnings(&malformed)
            .iter()
            .any(|warning| warning.contains("could not be inspected")));
    }

    #[test]
    fn electron_automatic_dock_launch_isolates_home_xdg_codex_and_browser_profile() {
        let config = base_config();
        let env_vars = BTreeMap::new();
        let args = Vec::new();

        let script = launcher_script(
            &config,
            Path::new("/Applications/Codex.app"),
            Some("/Applications/Codex.app/Contents/MacOS/Codex"),
            "/tmp/codex-profile",
            "Electron isolated HOME + --user-data-dir",
            &args,
            &env_vars,
        );

        assert!(script.contains("HOME_PATH=\"$DATA_PATH/Home\""));
        assert!(script.contains("BROWSER_PROFILE=\"$DATA_PATH/Browser\""));
        assert!(script.contains("\"$HOME_PATH/.codex\""));
        assert!(script.contains("export HOME=\"$HOME_PATH\""));
        assert!(script.contains("export XDG_CONFIG_HOME=\"$HOME_PATH/.config\""));
        assert!(script.contains("export XDG_CACHE_HOME=\"$HOME_PATH/.cache\""));
        assert!(script.contains("export XDG_DATA_HOME=\"$HOME_PATH/.local/share\""));
        assert!(script.contains("export CODEX_HOME=\"$HOME_PATH/.codex\""));
        assert!(script.contains("ARGS+=(\"--user-data-dir=$BROWSER_PROFILE\")"));
        assert!(!script.contains("OPEN_ENV_ARGS"));
    }

    #[test]
    fn persistent_electron_launcher_never_removes_its_profile() {
        let mut config = base_config();
        config.erase_on_close = false;

        let script = launcher_script(
            &config,
            Path::new("/Applications/ChatGPT.app"),
            Some("/Applications/ChatGPT.app/Contents/MacOS/ChatGPT"),
            "/Users/test/CloneOnce/Profiles/ChatGPT2",
            "Electron isolated HOME + --user-data-dir",
            &[],
            &BTreeMap::new(),
        );

        assert!(!script.contains("rm -rf"));
    }

    #[test]
    fn electron_browser_profile_system_launch_propagates_isolated_environment() {
        let mut config = base_config();
        config.launch_mode = "systemLaunch".into();
        config.data_strategy = "browserProfile".into();
        let env_vars = BTreeMap::new();
        let args = Vec::new();

        let script = launcher_script(
            &config,
            Path::new("/Applications/Codex.app"),
            Some("/Applications/Codex.app/Contents/MacOS/Codex"),
            "/tmp/codex-profile",
            "Electron isolated HOME + --user-data-dir",
            &args,
            &env_vars,
        );

        assert!(script.contains("OPEN_ENV_ARGS+=(--env \"HOME=$HOME_PATH\")"));
        assert!(script.contains("OPEN_ENV_ARGS+=(--env \"XDG_CONFIG_HOME=$HOME_PATH/.config\")"));
        assert!(script.contains("OPEN_ENV_ARGS+=(--env \"XDG_CACHE_HOME=$HOME_PATH/.cache\")"));
        assert!(script.contains("OPEN_ENV_ARGS+=(--env \"XDG_DATA_HOME=$HOME_PATH/.local/share\")"));
        assert!(script.contains("OPEN_ENV_ARGS+=(--env \"CODEX_HOME=$HOME_PATH/.codex\")"));
        assert!(script.contains("ARGS+=(\"--user-data-dir=$BROWSER_PROFILE\")"));
        assert!(script.contains("/usr/bin/open -n"));
    }

    #[test]
    fn command_shortcut_generation_writes_bundle_and_rejects_duplicate() {
        let temp_dir = env::temp_dir().join(format!(
            "cloneonce-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock before UNIX epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir).expect("create temp output dir");

        let mut config = base_config();
        config.shortcut_type = "command".into();
        config.name = "Echo Command".into();
        config.target_path = "/bin/echo".into();
        config.executable_path = Some("/bin/echo".into());
        config.bundle_id = "app.cloneonce.test.echo".into();
        config.output_dir = Some(temp_dir.display().to_string());
        config.launch_mode = "command".into();
        config.data_strategy = "none".into();
        config.command_arguments = "hello cloneonce".into();

        let generated = generate_shortcut(config.clone()).expect("generate command shortcut");
        let bundle_path = PathBuf::from(&generated.path);
        let launcher_path = bundle_path.join("Contents/MacOS/launch");
        let config_path = bundle_path.join("Contents/Resources/config.json");

        assert!(bundle_path.exists());
        assert!(launcher_path.exists());
        assert!(config_path.exists());
        assert_eq!(generated.data_path, "");
        assert!(fs::read_to_string(&launcher_path)
            .expect("read launcher")
            .contains("ARGS+=('hello cloneonce')"));
        assert!(fs::read_to_string(&config_path)
            .expect("read config")
            .contains(r#""dataPath": """#));

        let duplicate_error = generate_shortcut(config)
            .expect_err("duplicate shortcut name should fail")
            .to_string();
        assert!(duplicate_error.contains("already exists"));

        fs::remove_dir_all(temp_dir).expect("remove temp output dir");
    }
}
