<script lang="ts">
  import { onMount } from "svelte";
  import type { Component } from "svelte";
  import AppWindow from "@lucide/svelte/icons/app-window";
  import Earth from "@lucide/svelte/icons/earth";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import PackageOpen from "@lucide/svelte/icons/package-open";
  import SquareTerminal from "@lucide/svelte/icons/square-terminal";
  import { invoke } from "@tauri-apps/api/core";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";

  type ScreenId =
    | "library"
    | "type"
    | "app"
    | "identity"
    | "runtime"
    | "advanced"
    | "review";

  type ShortcutType = "appInstance" | "browserProfile" | "fileFolder" | "command";
  type LaunchMode = "dockShortcut" | "systemLaunch" | "command";
  type DataStrategy = "automatic" | "browserProfile" | "homeOverride" | "none";
  type IconComponent = Component<{ size?: number; strokeWidth?: number; "aria-hidden"?: boolean }>;

  interface AppInspection {
    path: string;
    name: string;
    bundleId: string;
    executablePath: string | null;
    iconFile: string | null;
    sandboxed: boolean;
    compatibility: "supported" | "limited" | "unknown";
    preset: string;
    valid: boolean;
    warnings: string[];
  }

  interface GeneratedShortcut {
    name: string;
    path: string;
    bundleId: string;
    configPath: string;
    launcherPath: string;
    dataPath: string;
    created: boolean;
  }

  interface ShortcutDraft {
    shortcutType: ShortcutType;
    name: string;
    targetPath: string;
    executablePath: string;
    bundleId: string;
    dataPath: string;
    outputDir: string;
    launchMode: LaunchMode;
    dataStrategy: DataStrategy;
    commandArguments: string;
    environment: string;
    eraseOnClose: boolean;
    menuBarControl: boolean;
    dedicatedDockIcon: boolean;
  }

  const screens: { id: ScreenId; label: string; name: string }[] = [
    { id: "library", label: "Library", name: "01 Library Home" },
    { id: "type", label: "Type", name: "02 Shortcut Type" },
    { id: "app", label: "App", name: "03 App Setup" },
    { id: "identity", label: "Identity", name: "04 Identity and Icon" },
    { id: "runtime", label: "Runtime", name: "05 Runtime and Storage" },
    { id: "advanced", label: "Advanced", name: "06 Advanced Options" },
    { id: "review", label: "Review", name: "07 Review and Approval" },
  ];

  const shortcutTypes: {
    id: ShortcutType;
    icon: IconComponent;
    title: string;
    body: string;
    badge: string;
  }[] = [
    {
      id: "appInstance",
      icon: AppWindow,
      title: "App Instance",
      body: "Standalone launcher for a macOS app with optional data separation.",
      badge: "best",
    },
    {
      id: "browserProfile",
      icon: Earth,
      title: "Browser Profile",
      body: "Preset profile handling for Chrome, Chromium, Edge, Firefox, and similar apps.",
      badge: "fast",
    },
    {
      id: "fileFolder",
      icon: FolderOpen,
      title: "File or Folder",
      body: "Open a file, folder, or document using the default macOS app.",
      badge: "simple",
    },
    {
      id: "command",
      icon: SquareTerminal,
      title: "Command",
      body: "Wrap an executable or script with arguments and environment variables.",
      badge: "advanced",
    },
  ];

  const launchModes: {
    id: LaunchMode;
    title: string;
    body: string;
    badge: string;
  }[] = [
    {
      id: "dockShortcut",
      title: "Dock Shortcut Mode",
      body: "Launches the target executable from this wrapper. Dock ownership is best-effort in this MVP.",
      badge: "recommended",
    },
    {
      id: "systemLaunch",
      title: "System Launch Mode",
      body: "Uses macOS open behavior. Some arguments or env values may not apply.",
      badge: "fallback",
    },
    {
      id: "command",
      title: "Command Mode",
      body: "Runs the selected executable or script with explicit arguments.",
      badge: "advanced",
    },
  ];

  const dataStrategies: {
    id: DataStrategy;
    title: string;
    body: string;
    badge: string;
  }[] = [
    {
      id: "automatic",
      title: "Automatic for this app",
      body: "Use the detected preset for known browser families.",
      badge: "auto",
    },
    {
      id: "browserProfile",
      title: "Browser profile folder",
      body: "Use Chrome/Firefox profile flags against the selected path.",
      badge: "profile",
    },
    {
      id: "homeOverride",
      title: "HOME path override",
      body: "Give non-sandboxed apps a separate home folder.",
      badge: "HOME",
    },
    {
      id: "none",
      title: "No data split",
      body: "Keep only identity and runtime controls.",
      badge: "none",
    },
  ];

  let activeScreen: ScreenId = "library";
  let busy = false;
  let statusMessage = "Ready. Original app files are never modified.";
  let errorMessage = "";
  let inspection: AppInspection | null = null;
  let generated: GeneratedShortcut | null = null;

  let draft: ShortcutDraft = {
    shortcutType: "appInstance",
    name: "",
    targetPath: "",
    executablePath: "",
    bundleId: "app.cloneonce.shortcut",
    dataPath: "",
    outputDir: "",
    launchMode: "dockShortcut",
    dataStrategy: "automatic",
    commandArguments: "",
    environment: "",
    eraseOnClose: false,
    menuBarControl: false,
    dedicatedDockIcon: true,
  };

  $: activeIndex = screens.findIndex((screen) => screen.id === activeScreen);
  $: selectedShortcutType =
    shortcutTypes.find((item) => item.id === draft.shortcutType) ?? shortcutTypes[0];
  $: selectedDataStrategy =
    dataStrategies.find((item) => item.id === draft.dataStrategy) ?? dataStrategies[0];
  $: selectedLaunchMode =
    launchModes.find((item) => item.id === draft.launchMode) ?? launchModes[0];
  $: SelectedIcon = selectedShortcutType.icon;
  $: ResultIcon = generated ? PackageOpen : selectedShortcutType.icon;
  $: compatibilityLabel = inspection?.compatibility ?? "unknown";
  $: isAppLikeShortcut =
    draft.shortcutType === "appInstance" || draft.shortcutType === "browserProfile";
  $: usesDataPath = isAppLikeShortcut && draft.dataStrategy !== "none";
  $: displayedDataPath = usesDataPath ? draft.dataPath || defaultProfilePath() : "Not used";
  $: canGenerate =
    draft.name.trim().length > 0 &&
    draft.targetPath.trim().length > 0 &&
    (draft.shortcutType === "fileFolder" || draft.shortcutType === "command" || !!inspection?.valid);

  onMount(() => {
    void inspectTarget({ quiet: true });
  });

  function go(screen: ScreenId) {
    activeScreen = screen;
    errorMessage = "";
  }

  function touchDraft() {
    generated = null;
  }

  function next() {
    const nextScreen = screens[Math.min(activeIndex + 1, screens.length - 1)];
    go(nextScreen.id);
  }

  function back() {
    const previousScreen = screens[Math.max(activeIndex - 1, 0)];
    go(previousScreen.id);
  }

  function newRecipe() {
    generated = null;
    inspection = null;
    errorMessage = "";
    statusMessage = "Choose a target to begin.";
    draft = {
      shortcutType: "appInstance",
      name: "",
      targetPath: "",
      executablePath: "",
      bundleId: "app.cloneonce.shortcut",
      dataPath: "",
      outputDir: "",
      launchMode: "dockShortcut",
      dataStrategy: "automatic",
      commandArguments: "",
      environment: "",
      eraseOnClose: false,
      menuBarControl: false,
      dedicatedDockIcon: true,
    };
    activeScreen = "type";
  }

  function selectShortcutType(id: ShortcutType) {
    touchDraft();
    draft.shortcutType = id;
    errorMessage = "";
    if (id === "browserProfile") {
      draft.dataStrategy = "browserProfile";
      draft.launchMode = "dockShortcut";
    }
    if (id === "command") {
      draft.launchMode = "command";
      draft.dataStrategy = "none";
      draft.targetPath = "";
      draft.executablePath = "";
      draft.commandArguments = "";
      draft.bundleId = "app.cloneonce.command";
      draft.name = "Command Shortcut";
      inspection = null;
    }
    if (id === "fileFolder") {
      draft.dataStrategy = "none";
      draft.launchMode = "systemLaunch";
      draft.targetPath = "";
      draft.executablePath = "";
      draft.name = "File Shortcut";
      draft.bundleId = "app.cloneonce.file-shortcut";
      inspection = null;
    }
  }

  async function chooseTarget(kind: "app" | "command" | "file" | "folder" = targetDialogKind()) {
    errorMessage = "";
    try {
      const selected = await openDialog({
        multiple: false,
        directory: kind === "folder",
        filters: kind === "app" ? [{ name: "Applications", extensions: ["app"] }] : undefined,
      });
      if (typeof selected !== "string") return;
      applySelectedTarget(selected);
      touchDraft();
      if (draft.shortcutType === "command") draft.executablePath = selected;
      if (isAppLikeShortcut) {
        await inspectTarget();
      }
    } catch (error) {
      errorMessage = readableError(error);
    }
  }

  function targetDialogKind(): "app" | "command" | "file" | "folder" {
    if (isAppLikeShortcut) return "app";
    if (draft.shortcutType === "command") return "command";
    return "file";
  }

  function applySelectedTarget(path: string) {
    draft.targetPath = path;
    if (draft.shortcutType === "fileFolder" && shouldReplaceAutoName()) {
      draft.name = `${stripExtension(lastPathPart(path))} Shortcut`;
      draft.bundleId = `app.cloneonce.${slugify(draft.name)}`;
    }
    if (draft.shortcutType === "command" && shouldReplaceAutoName()) {
      draft.name = `${stripExtension(lastPathPart(path))} Command`;
      draft.bundleId = `app.cloneonce.${slugify(draft.name)}`;
    }
  }

  async function chooseDataPath() {
    await chooseFolder((path) => {
      touchDraft();
      draft.dataPath = path;
    });
  }

  async function chooseOutputDir() {
    await chooseFolder((path) => {
      touchDraft();
      draft.outputDir = path;
    });
  }

  async function chooseFolder(apply: (path: string) => void) {
    errorMessage = "";
    try {
      const selected = await openDialog({ multiple: false, directory: true });
      if (typeof selected === "string") apply(selected);
    } catch (error) {
      errorMessage = readableError(error);
    }
  }

  async function inspectTarget(options: { quiet?: boolean } = {}) {
    if (!draft.targetPath.trim() || draft.shortcutType === "command" || draft.shortcutType === "fileFolder") {
      return;
    }
    busy = true;
    errorMessage = "";
    try {
      const result = await invoke<AppInspection>("inspect_app_bundle", {
        path: draft.targetPath,
      });
      inspection = result;
      draft.executablePath = result.executablePath ?? "";
      if (result.name && shouldReplaceAutoName()) {
        const friendly = `${result.name} Shortcut`;
        draft.name = friendly;
        draft.bundleId = `app.cloneonce.${slugify(friendly)}`;
        draft.dataPath = `~/CloneOnce/Profiles/${friendly}`;
      }
      if (result.preset.toLowerCase().includes("firefox")) {
        draft.dataStrategy = "browserProfile";
      }
      if (result.sandboxed) {
        draft.dataStrategy = "none";
      }
      statusMessage = result.valid
        ? `${result.name} inspected. ${result.preset} preset is ready.`
        : "Inspection finished with warnings.";
    } catch (error) {
      inspection = null;
      if (!options.quiet) errorMessage = readableError(error);
    } finally {
      busy = false;
    }
  }

  async function generateShortcut() {
    busy = true;
    errorMessage = "";
    generated = null;
    try {
      const result = await invoke<GeneratedShortcut>("generate_shortcut", {
        config: {
          shortcutType: draft.shortcutType,
          name: draft.name,
          targetPath: draft.targetPath,
          executablePath: draft.executablePath || null,
          bundleId: draft.bundleId,
          dataPath: draft.dataPath,
          outputDir: draft.outputDir || null,
          launchMode: draft.launchMode,
          dataStrategy: draft.dataStrategy,
          commandArguments: draft.commandArguments,
          environment: draft.environment,
          eraseOnClose: draft.eraseOnClose,
          menuBarControl: draft.menuBarControl,
          dedicatedDockIcon: draft.dedicatedDockIcon,
        },
      });
      generated = result;
      activeScreen = "review";
      statusMessage = `${result.name} created at ${result.path}`;
    } catch (error) {
      errorMessage = readableError(error);
    } finally {
      busy = false;
    }
  }

  async function runGenerated() {
    if (!generated) return;
    await callSimpleCommand("run_shortcut", generated.path, "Shortcut launched.");
  }

  async function revealGenerated() {
    if (!generated) return;
    await callSimpleCommand("reveal_in_finder", generated.path, "Revealed in Finder.");
  }

  async function callSimpleCommand(command: string, path: string, success: string) {
    busy = true;
    errorMessage = "";
    try {
      await invoke(command, { path });
      statusMessage = success;
    } catch (error) {
      errorMessage = readableError(error);
    } finally {
      busy = false;
    }
  }

  function badgeClass(value: string) {
    if (value === "supported") return "good";
    if (value === "limited") return "warn";
    return "neutral";
  }

  function slugify(value: string) {
    return value
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-|-$/g, "");
  }

  function defaultProfilePath() {
    return `~/CloneOnce/Profiles/${draft.name || selectedShortcutType.title}`;
  }

  function shouldReplaceAutoName() {
    return ["", "File Shortcut", "Command Shortcut"].includes(draft.name.trim());
  }

  function lastPathPart(path: string) {
    return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
  }

  function stripExtension(value: string) {
    return value.replace(/\.app$/i, "").replace(/\.[^.]+$/, "");
  }

  function readableError(error: unknown) {
    if (typeof error === "string") return error;
    if (error instanceof Error) return error.message;
    return "Unknown error.";
  }
</script>

<svelte:head>
  <title>CloneOnce</title>
</svelte:head>

<main class="app-shell">
  <section class="window">
    <header class="topbar">
      <div class="brand">
        <img src="/cloneonce-app-icon.svg" alt="" />
        <strong>Recipes</strong>
        <span>Shortcut recipe workspace</span>
      </div>
      <nav class="tabs" aria-label="CloneOnce screens">
        {#each screens as screen}
          <button
            type="button"
            class:active={activeScreen === screen.id}
            aria-current={activeScreen === screen.id ? "page" : undefined}
            on:click={() => go(screen.id)}
          >
            {screen.label}
          </button>
        {/each}
      </nav>
      <div class="top-actions">
        <button type="button" on:click={newRecipe}>New</button>
        <button class="primary" type="button" disabled={!canGenerate || busy} on:click={generateShortcut}>
          Create Bundle
        </button>
      </div>
    </header>

    <div class="workspace">
      <aside class="sidebar panel" aria-label="Templates and shortcuts">
        <p class="label">Templates</p>
        <div class="template-list">
          {#each shortcutTypes as item}
            {@const Icon = item.icon}
            <button
              type="button"
              class="template"
              class:active={draft.shortcutType === item.id}
              on:click={() => selectShortcutType(item.id)}
            >
              <span class="glyph"><Icon size={19} strokeWidth={2.5} aria-hidden={true} /></span>
              <span>
                <strong>{item.title}</strong>
                <small>{item.body.split(".")[0]}</small>
              </span>
              <em>{item.badge}</em>
            </button>
          {/each}
        </div>

        <div class="recent-list">
          <p class="label">Generated</p>
          {#if generated}
            <button type="button" class="recent-item" on:click={() => go("review")}>
              <span><PackageOpen size={16} strokeWidth={2.5} aria-hidden={true} /></span><b>{generated.name}</b><small>Created</small>
            </button>
          {:else}
            <div class="empty-mini">No shortcuts created in this session.</div>
          {/if}
        </div>

        <div class="support-card">
          <strong>Local by default</strong>
          <p>No telemetry or background daemon. Generated wrappers are basic local bundles.</p>
        </div>
      </aside>

      <section class="main panel" aria-label="Active screen">
        <div class="screen-head">
          <div>
            <p class="eyebrow">{screens[activeIndex]?.name}</p>
            {#if activeScreen === "library"}
              <h1>Shortcut library</h1>
              <p>Manage generated app bundles and start common sessions without reopening the builder.</p>
            {:else if activeScreen === "type"}
              <h1>Choose shortcut type</h1>
              <p>Pick the generated bundle shape before choosing an app, file, folder, or command.</p>
            {:else if activeScreen === "app"}
              <h1>App setup</h1>
              <p>Choose the original target and confirm the launch behavior CloneOnce will write.</p>
            {:else if activeScreen === "identity"}
              <h1>Identity and icon</h1>
              <p>Give this shortcut a clear Dock name and bundle identity.</p>
            {:else if activeScreen === "runtime"}
              <h1>Runtime and storage</h1>
              <p>Choose profile isolation and the output folder for the generated app bundle.</p>
            {:else if activeScreen === "advanced"}
              <h1>Advanced options</h1>
              <p>Expose low-level launch fields only after safe defaults are selected.</p>
            {:else}
              <h1>Review and approval</h1>
              <p>Confirm the recipe and guide the first-run macOS approval flow.</p>
            {/if}
          </div>
          <div class="status-score">
            {#if activeScreen === "library"}
              <strong>{generated ? "Created" : "Ready"}</strong><span>local only</span>
            {:else if activeScreen === "type"}
              <strong>4 types</strong><span>available</span>
            {:else if activeScreen === "app"}
              <strong class={badgeClass(compatibilityLabel)}>{compatibilityLabel}</strong><span>{inspection?.preset ?? "inspect target"}</span>
            {:else if activeScreen === "identity"}
              <strong>Preview</strong><span>final scale</span>
            {:else if activeScreen === "runtime"}
              <strong>{usesDataPath ? selectedDataStrategy.badge : "none"}</strong><span>data strategy</span>
            {:else if activeScreen === "advanced"}
              <strong>Expert</strong><span>optional</span>
            {:else}
              <strong>{generated ? "Created" : "Ready"}</strong><span>before save</span>
            {/if}
          </div>
        </div>

        <div class="screen-body">
          {#if activeScreen === "library"}
            {#if generated}
              <section class="hero-card">
                <div>
                  <p class="field-title">Generated shortcut</p>
                  <h2>{generated.name}</h2>
                  <p>{generated.path}</p>
                </div>
                <div class="big-icon"><PackageOpen size={42} strokeWidth={2.3} aria-hidden={true} /></div>
                <div class="button-stack">
                  <button class="primary" type="button" disabled={busy} on:click={runGenerated}>
                    Open Shortcut
                  </button>
                  <button type="button" disabled={busy} on:click={revealGenerated}>Reveal</button>
                </div>
              </section>
            {:else}
              <section class="empty-state">
                <img src="/cloneonce-app-icon.svg" alt="" />
                <div>
                  <p class="field-title">No shortcut yet</p>
                  <h2>Create your first wrapper</h2>
                  <p>Choose a macOS app, command, file, or folder. CloneOnce will inspect it and write a local .app bundle.</p>
                </div>
                <button class="primary" type="button" on:click={() => go("type")}>Start Recipe</button>
              </section>
            {/if}
            <section class="note-card">
              <b>Compatibility presets are local</b>
              <span>Known browser families use safer launch defaults. Untested apps start with a limited fallback.</span>
            </section>
          {:else if activeScreen === "type"}
            <section>
              <p class="field-title">Shortcut type</p>
              <div class="type-grid">
                {#each shortcutTypes as item}
                  {@const Icon = item.icon}
                  <button
                    class="type-card"
                    class:active={draft.shortcutType === item.id}
                    type="button"
                    on:click={() => selectShortcutType(item.id)}
                  >
                    <span class="glyph"><Icon size={19} strokeWidth={2.5} aria-hidden={true} /></span>
                    <span><b>{item.title}</b><small>{item.body}</small></span>
                    <em>{item.badge}</em>
                  </button>
                {/each}
              </div>
            </section>
            <section class="two-col">
              <div class="note-card"><b>No background service</b><span>Generated shortcuts launch only when used.</span></div>
              <div class="note-card"><b>Original app untouched</b><span>The wrapper owns name, icon, configuration, and launch recipe.</span></div>
            </section>
          {:else if activeScreen === "app"}
            <section class="target-card">
              <div class="glyph large"><SelectedIcon size={25} strokeWidth={2.4} aria-hidden={true} /></div>
              <div>
                <p class="field-title">Target</p>
                <h2>{inspection?.name ?? (draft.shortcutType === "command" ? "Command executable" : "Choose target")}</h2>
                <p>{draft.targetPath || "No target selected"}</p>
              </div>
              <div class="button-stack">
                {#if draft.shortcutType === "fileFolder"}
                  <button type="button" on:click={() => chooseTarget("file")}>Choose File</button>
                  <button type="button" on:click={() => chooseTarget("folder")}>Choose Folder</button>
                {:else}
                  <button type="button" on:click={() => chooseTarget()}>
                    {draft.shortcutType === "command" ? "Choose Executable" : "Choose App"}
                  </button>
                  {#if isAppLikeShortcut}
                    <button type="button" disabled={busy} on:click={() => inspectTarget()}>
                      Inspect
                    </button>
                  {/if}
                {/if}
              </div>
            </section>
            {#if isAppLikeShortcut}
              <section>
                <p class="field-title">Launch mode</p>
                <div class="type-grid">
                  {#each launchModes.filter((mode) => mode.id !== "command") as mode}
                    <button
                      type="button"
                      class="type-card compact"
                      class:active={draft.launchMode === mode.id}
                      on:click={() => {
                        touchDraft();
                        draft.launchMode = mode.id;
                      }}
                    >
                      <span><b>{mode.title}</b><small>{mode.body}</small></span>
                      <em>{mode.badge}</em>
                    </button>
                  {/each}
                </div>
              </section>
            {:else}
              <section class="note-card">
                <b>{draft.shortcutType === "command" ? "Command mode" : "System open mode"}</b>
                <span>
                  {draft.shortcutType === "command"
                    ? "The generated bundle runs the selected executable with the arguments from Advanced Options."
                    : "The generated bundle asks macOS to open the selected file or folder with its default app."}
                </span>
              </section>
            {/if}
            <section class="form-grid">
              {#if draft.shortcutType !== "fileFolder"}
                <label>
                  <span>Executable</span>
                  <input bind:value={draft.executablePath} on:input={touchDraft} placeholder="/Contents/MacOS/App" />
                </label>
              {:else}
                <label>
                  <span>Open item</span>
                  <input value={draft.targetPath || "No file or folder selected"} readonly />
                </label>
              {/if}
              <label>
                <span>Bundle id</span>
                <input bind:value={draft.bundleId} on:input={touchDraft} />
              </label>
              {#if isAppLikeShortcut}
                <label class="full">
                  <span>Detected preset</span>
                  <input value={inspection?.preset ?? "No preset yet"} readonly />
                </label>
              {/if}
            </section>
          {:else if activeScreen === "identity"}
            <section class="identity-layout">
              <div class="icon-stage">
                <img class="app-icon-preview" src="/cloneonce-app-icon.svg" alt="" />
                <p>Generated wrappers use the CloneOnce app icon until custom icon compositing is implemented.</p>
              </div>
              <div class="form-grid">
                <label class="full"><span>Shortcut name</span><input bind:value={draft.name} on:input={touchDraft} /></label>
                <label class="full"><span>Bundle id</span><input bind:value={draft.bundleId} on:input={touchDraft} /></label>
                <label class="full">
                  <span>Identity notes</span>
                  <textarea readonly>Name and bundle id are written into the generated shortcut. Custom icon compositing is intentionally not exposed until it is implemented.</textarea>
                </label>
              </div>
            </section>
          {:else if activeScreen === "runtime"}
            {#if isAppLikeShortcut}
              <section>
                <p class="field-title">Data storage mode</p>
                <div class="type-grid">
                  {#each dataStrategies as strategy}
                    <button
                      type="button"
                      class="type-card compact"
                      class:active={draft.dataStrategy === strategy.id}
                      on:click={() => {
                        touchDraft();
                        draft.dataStrategy = strategy.id;
                      }}
                    >
                      <span><b>{strategy.title}</b><small>{strategy.body}</small></span>
                      <em>{strategy.badge}</em>
                    </button>
                  {/each}
                </div>
              </section>
            {:else}
              <section class="note-card">
                <b>No separate data profile</b>
                <span>{draft.shortcutType === "command" ? "Command shortcuts run the selected executable and do not create a managed data folder." : "File and folder shortcuts only ask macOS to open the selected item."}</span>
              </section>
            {/if}
            <section class="form-grid">
              {#if usesDataPath}
                <label class="full">
                  <span>Data storage path</span>
                  <div class="input-row">
                    <input bind:value={draft.dataPath} on:input={touchDraft} placeholder={defaultProfilePath()} />
                    <button type="button" on:click={chooseDataPath}>...</button>
                  </div>
                </label>
              {:else}
                <div class="full path-note">
                  <span>Data storage path</span>
                  <b>Not used for this recipe</b>
                  <small>Choose a data-splitting strategy above to attach an isolated profile folder.</small>
                </div>
              {/if}
              <label class="full">
                <span>Output folder</span>
                <div class="input-row">
                  <input bind:value={draft.outputDir} on:input={touchDraft} placeholder="~/Applications/CloneOnce Shortcuts" />
                  <button type="button" on:click={chooseOutputDir}>...</button>
                </div>
              </label>
            </section>
            {#if usesDataPath}
              <section class="toggle-grid">
                <label><input type="checkbox" bind:checked={draft.eraseOnClose} on:change={touchDraft} /><span>Erase on close</span><small>Delete data path after process exits</small></label>
              </section>
            {/if}
          {:else if activeScreen === "advanced"}
            <section class="form-grid">
              <label class="full"><span>Environment variables</span><input bind:value={draft.environment} on:input={touchDraft} placeholder="KEY=VALUE; KEY2=VALUE2" /></label>
              <label class="full">
                <span>Command arguments, one per line</span>
                <textarea bind:value={draft.commandArguments} on:input={touchDraft}></textarea>
              </label>
            </section>
            <section class="note-card"><b>Basic wrapper limit</b><span>This MVP writes a zsh launcher bundle. It does not expose menu bar icons, Dock effects, or Info.plist overrides yet.</span></section>
            <section class="note-card warn"><b>Advanced fields can break recipes</b><span>Arguments are shell-quoted by the generator. Use one argument per line.</span></section>
          {:else}
            <section class="review-grid">
              <div class="review-summary">
                <div class="mega-icon small">
                  <ResultIcon size={44} strokeWidth={2.25} aria-hidden={true} />
                </div>
                <h2>{draft.name}</h2>
                <p>Basic wrapper app with {selectedDataStrategy.title.toLowerCase()}.</p>
                {#if generated}
                  <div class="button-row">
                    <button type="button" disabled={busy} on:click={revealGenerated}>Reveal</button>
                    <button class="primary" type="button" disabled={busy} on:click={runGenerated}>Open Shortcut</button>
                  </div>
                {:else}
                  <button class="primary" type="button" disabled={!canGenerate || busy} on:click={generateShortcut}>
                    Create Bundle
                  </button>
                {/if}
              </div>
              <div class="recipe-lines">
                <div><span>Target</span><b>{draft.targetPath}</b></div>
                <div><span>Bundle</span><b>{draft.bundleId}</b></div>
                <div><span>Data path</span><b>{displayedDataPath}</b></div>
                <div><span>Method</span><b>{inspection?.preset ?? selectedLaunchMode.title}</b></div>
                <div><span>Output</span><b>{generated?.path ?? (draft.outputDir || "~/Applications/CloneOnce Shortcuts")}</b></div>
              </div>
            </section>
            <section>
              <p class="field-title">First-run approval</p>
              <div class="approval-list">
                <article><b>1</b><span><strong>Create the shortcut</strong><small>Writes a local .app wrapper with config.</small></span><button disabled={!!generated || !canGenerate || busy} on:click={generateShortcut}>Create</button></article>
                <article><b>2</b><span><strong>Reveal in Finder</strong><small>Inspect the generated bundle location.</small></span><button disabled={!generated || busy} on:click={revealGenerated}>Reveal</button></article>
                <article><b>3</b><span><strong>Run the shortcut once</strong><small>macOS may ask for first-run approval.</small></span><button disabled={!generated || busy} on:click={runGenerated}>Run</button></article>
              </div>
            </section>
          {/if}
        </div>

        <footer class="screen-footer">
          <span>{busy ? "Working..." : statusMessage}</span>
          <div>
            <button type="button" disabled={activeIndex === 0} on:click={back}>Back</button>
            <button class="primary" type="button" disabled={activeIndex === screens.length - 1} on:click={next}>Continue</button>
          </div>
        </footer>
      </section>

      <aside class="inspector panel" aria-label="Live inspector">
        <section class="preview-card">
          <div class="preview-icon"><ResultIcon size={36} strokeWidth={2.35} aria-hidden={true} /></div>
          <h2>{draft.name || selectedShortcutType.title}</h2>
          <p>{generated ? "Created" : selectedLaunchMode.title}</p>
        </section>

        <section class="recipe-card">
          <p class="field-title">Launch recipe</p>
          <div><span>Shortcut</span><b>{draft.name || "Untitled"}</b></div>
          <div><span>Preset</span><b>{inspection?.preset ?? selectedDataStrategy.title}</b></div>
          <div><span>Mode</span><b>{selectedLaunchMode.title}</b></div>
          <div><span>Data</span><b>{usesDataPath ? selectedDataStrategy.title : "No data split"}</b></div>
        </section>

        <section class="checks-card">
          <p class="field-title">Checks</p>
          <div class={badgeClass(compatibilityLabel)}>Compatibility: {compatibilityLabel}</div>
          <div class="good">Original app remains read only.</div>
          <div class="warn">Basic wrapper only; no compiled runner yet.</div>
          {#if inspection?.warnings.length}
            {#each inspection.warnings as warning}
              <div class="warn">{warning}</div>
            {/each}
          {/if}
          {#if errorMessage}
            <div class="danger">{errorMessage}</div>
          {/if}
        </section>

        {#if generated}
          <section class="success-card">
            <p class="field-title">Generated</p>
            <b>{generated.name}</b>
            <span>{generated.path}</span>
            <div class="button-row">
              <button type="button" on:click={revealGenerated}>Reveal</button>
              <button class="primary" type="button" on:click={runGenerated}>Run</button>
            </div>
          </section>
        {/if}
      </aside>
    </div>
  </section>
</main>

<style>
  :global(*) {
    box-sizing: border-box;
  }

  :global(html),
  :global(body) {
    margin: 0;
    min-height: 100%;
    color: #15171c;
    background:
      linear-gradient(145deg, rgba(255, 255, 255, 0.94), rgba(242, 245, 249, 0.78) 46%, rgba(226, 235, 245, 0.88)),
      #f2f5f9;
    font-family: ui-sans-serif, -apple-system, BlinkMacSystemFont, "SF Pro Text", "Segoe UI", sans-serif;
  }

  :global(body) {
    min-height: 100vh;
    font-size: 14px;
    line-height: 1.42;
  }

  button,
  input,
  textarea {
    font: inherit;
  }

  button {
    min-height: 38px;
    border: 1px solid rgba(38, 48, 61, 0.14);
    border-radius: 7px;
    background: #fff;
    color: #15171c;
    cursor: pointer;
    font-weight: 850;
    padding: 0 13px;
  }

  button:disabled {
    cursor: default;
    opacity: 0.48;
  }

  .primary {
    color: #fff;
    border-color: #2563eb;
    background: #2563eb;
    box-shadow: 0 12px 24px rgba(37, 99, 235, 0.22);
  }

  .app-shell {
    min-height: 100vh;
    padding: 0;
  }

  .window {
    width: 100%;
    min-height: 100vh;
    border: 0;
    border-radius: 0;
    overflow: hidden;
    box-shadow: none;
    background: rgba(250, 252, 255, 0.9);
    display: grid;
    grid-template-rows: 64px 1fr;
  }

  .topbar {
    min-width: 0;
    display: grid;
    grid-template-columns: minmax(220px, 1fr) auto auto;
    align-items: center;
    gap: 14px;
    padding: 0 20px;
    border-bottom: 1px solid rgba(38, 48, 61, 0.14);
    background: rgba(255, 255, 255, 0.7);
  }

  .brand {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .brand img {
    width: 30px;
    height: 30px;
    border-radius: 7px;
    box-shadow: 0 7px 16px rgba(26, 35, 48, 0.18);
  }

  .brand strong {
    white-space: nowrap;
  }

  .brand span {
    color: #738091;
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tabs {
    display: flex;
    gap: 5px;
    padding: 4px;
    border: 1px solid rgba(38, 48, 61, 0.14);
    border-radius: 8px;
    background: #eef2f7;
  }

  .tabs button {
    min-height: 32px;
    border: 0;
    background: transparent;
    color: #485464;
    font-size: 12px;
    padding: 0 11px;
    box-shadow: none;
  }

  .tabs button.active {
    background: #fff;
    color: #1d4ed8;
    box-shadow: 0 1px 6px rgba(26, 35, 48, 0.1);
  }

  .top-actions {
    display: flex;
    gap: 8px;
  }

  .workspace {
    min-width: 0;
    display: grid;
    grid-template-columns: 276px minmax(540px, 1fr) 348px;
    gap: 16px;
    padding: 16px;
  }

  .panel {
    min-width: 0;
    border: 1px solid rgba(38, 48, 61, 0.14);
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.8);
    box-shadow: 0 12px 28px rgba(26, 35, 48, 0.1);
  }

  .sidebar {
    padding: 14px;
    background: rgba(239, 243, 248, 0.82);
    display: grid;
    grid-template-rows: auto auto 1fr auto;
    gap: 14px;
  }

  .label,
  .eyebrow,
  .field-title {
    margin: 0;
    color: #738091;
    font-size: 11px;
    font-weight: 900;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .template-list,
  .recent-list {
    display: grid;
    gap: 8px;
    align-content: start;
  }

  .template {
    width: 100%;
    min-height: 62px;
    display: grid;
    grid-template-columns: 36px 1fr auto;
    gap: 10px;
    align-items: center;
    text-align: left;
    background: rgba(255, 255, 255, 0.66);
    border-color: transparent;
    padding: 10px;
  }

  .template.active,
  .template:hover {
    background: #fff;
    border-color: rgba(37, 99, 235, 0.26);
  }

  .template strong,
  .template small {
    display: block;
  }

  .template small,
  .recent-item small,
  .support-card p,
  .note-card span,
  .type-card small,
  .target-card p,
  .review-summary p,
  .preview-card p {
    color: #485464;
    font-size: 12px;
  }

  .template em,
  .type-card em {
    padding: 4px 8px;
    border-radius: 99px;
    background: #e8edf4;
    color: #738091;
    font-size: 11px;
    font-style: normal;
    font-weight: 900;
  }

  .glyph {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border-radius: 8px;
    background: linear-gradient(145deg, #3b82f6, #2563eb);
    color: #fff;
    font-weight: 950;
  }

  .glyph.large {
    width: 54px;
    height: 54px;
    font-size: 20px;
  }

  .recent-item {
    width: 100%;
    min-height: 40px;
    display: grid;
    grid-template-columns: 28px 1fr auto;
    gap: 8px;
    align-items: center;
    padding: 6px;
    text-align: left;
    border-color: transparent;
    background: transparent;
  }

  .recent-item span {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 7px;
    background: #087a4c;
    color: #fff;
    font-size: 12px;
    font-weight: 900;
  }

  .empty-mini {
    padding: 12px;
    border: 1px dashed rgba(38, 48, 61, 0.18);
    border-radius: 8px;
    color: #738091;
    font-size: 12px;
    font-weight: 750;
  }

  .support-card,
  .note-card {
    padding: 14px;
    border: 1px solid rgba(38, 48, 61, 0.14);
    border-radius: 8px;
    background: #fff;
  }

  .note-card.warn {
    border-color: rgba(180, 83, 9, 0.24);
    background: #fff7ed;
  }

  .main {
    display: grid;
    grid-template-rows: auto 1fr auto;
    overflow: hidden;
  }

  .screen-head {
    min-width: 0;
    display: flex;
    justify-content: space-between;
    gap: 20px;
    padding: 22px;
    border-bottom: 1px solid rgba(38, 48, 61, 0.14);
  }

  h1,
  h2,
  p {
    margin-top: 0;
  }

  h1 {
    margin-bottom: 4px;
    font-size: 24px;
    line-height: 1.1;
    letter-spacing: 0;
  }

  h2 {
    margin-bottom: 5px;
    font-size: 21px;
    line-height: 1.15;
  }

  .screen-head p {
    margin-bottom: 0;
    color: #485464;
  }

  .status-score {
    min-width: 124px;
    text-align: right;
  }

  .status-score strong {
    display: block;
    color: #2563eb;
    font-size: 22px;
    line-height: 1;
    text-transform: capitalize;
  }

  .status-score span {
    display: block;
    color: #738091;
    font-size: 12px;
    font-weight: 850;
  }

  .status-score .good { color: #087a4c; }
  .status-score .warn { color: #b45309; }
  .status-score .neutral { color: #738091; }

  .screen-body {
    min-width: 0;
    padding: 18px 22px;
    display: grid;
    gap: 18px;
    align-content: start;
    overflow: auto;
  }

  .screen-footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 16px;
    padding: 14px 22px;
    border-top: 1px solid rgba(38, 48, 61, 0.14);
    background: #f5f8fc;
    color: #738091;
    font-size: 12px;
  }

  .screen-footer div {
    display: flex;
    gap: 8px;
  }

  .hero-card,
  .target-card,
  .empty-state {
    display: grid;
    grid-template-columns: 1fr auto auto;
    align-items: center;
    gap: 18px;
    padding: 20px;
    border: 1px solid rgba(38, 48, 61, 0.14);
    border-radius: 8px;
    background: linear-gradient(145deg, #fff, #f8fbff);
  }

  .empty-state {
    grid-template-columns: 72px 1fr auto;
  }

  .empty-state img {
    width: 58px;
    height: 58px;
    border-radius: 12px;
    box-shadow: 0 10px 20px rgba(26, 35, 48, 0.14);
  }

  .empty-state p {
    margin-bottom: 0;
    color: #485464;
  }

  .target-card {
    grid-template-columns: 54px 1fr auto;
  }

  .big-icon,
  .mega-icon {
    display: grid;
    place-items: center;
    color: #fff;
    font-weight: 950;
    background: linear-gradient(145deg, #3b82f6, #2563eb);
    box-shadow: 0 10px 20px rgba(37, 99, 235, 0.2);
  }

  .big-icon {
    width: 86px;
    height: 86px;
    border-radius: 22px;
    font-size: 38px;
  }

  .type-grid,
  .two-col,
  .toggle-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
  }

  .type-card {
    position: relative;
    min-height: 112px;
    display: grid;
    grid-template-columns: 42px 1fr;
    gap: 12px;
    padding: 16px 112px 16px 16px;
    text-align: left;
    background: #fff;
  }

  .type-card.compact {
    grid-template-columns: 1fr;
  }

  .type-card.active {
    border-color: rgba(37, 99, 235, 0.35);
    box-shadow: 0 0 0 3px rgba(37, 99, 235, 0.1);
  }

  .type-card b,
  .type-card small {
    display: block;
    min-width: 0;
  }

  .type-card em {
    position: absolute;
    top: 16px;
    right: 14px;
  }

  .button-stack {
    display: grid;
    gap: 8px;
  }

  .form-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
  }

  .form-grid .full {
    grid-column: 1 / -1;
  }

  label {
    display: grid;
    gap: 6px;
  }

  label > span {
    color: #485464;
    font-size: 12px;
    font-weight: 850;
  }

  input,
  textarea {
    width: 100%;
    border: 1px solid rgba(38, 48, 61, 0.14);
    border-radius: 7px;
    background: #fff;
    color: #15171c;
    padding: 10px 12px;
    outline: none;
  }

  input:focus,
  textarea:focus {
    border-color: rgba(37, 99, 235, 0.7);
    box-shadow: 0 0 0 3px rgba(37, 99, 235, 0.12);
  }

  textarea {
    min-height: 102px;
    resize: vertical;
  }

  .input-row {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 8px;
  }

  .path-note {
    display: grid;
    gap: 6px;
    padding: 12px;
    border: 1px solid rgba(38, 48, 61, 0.14);
    border-radius: 8px;
    background: #f8fbff;
  }

  .path-note > span {
    color: #485464;
    font-size: 12px;
    font-weight: 850;
  }

  .path-note b {
    font-size: 14px;
  }

  .path-note small {
    color: #738091;
    font-size: 12px;
  }

  .identity-layout,
  .review-grid {
    display: grid;
    grid-template-columns: 260px 1fr;
    gap: 18px;
  }

  .icon-stage,
  .review-summary {
    display: grid;
    justify-items: center;
    align-content: start;
    gap: 14px;
    padding: 18px;
    border: 1px solid rgba(38, 48, 61, 0.14);
    border-radius: 8px;
    background: #fff;
  }

  .review-summary {
    text-align: center;
  }

  .app-icon-preview {
    width: 128px;
    height: 128px;
    border-radius: 28px;
    box-shadow: 0 16px 34px rgba(26, 35, 48, 0.18);
  }

  .icon-stage p {
    margin-bottom: 0;
    color: #485464;
    text-align: center;
  }

  .mega-icon {
    position: relative;
    width: 128px;
    height: 128px;
    border-radius: 30px;
    background: linear-gradient(145deg, #ff8a34, #ff6b17);
    font-size: 62px;
  }

  .mega-icon.small {
    width: 96px;
    height: 96px;
    border-radius: 24px;
    font-size: 44px;
  }

  .toggle-grid label {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: center;
    gap: 10px;
    padding: 14px;
    border: 1px solid rgba(38, 48, 61, 0.14);
    border-radius: 8px;
    background: #fff;
  }

  .toggle-grid input {
    width: 18px;
    height: 18px;
  }

  .toggle-grid span,
  .toggle-grid small {
    grid-column: 2;
  }

  .toggle-grid small {
    color: #738091;
  }

  .recipe-lines {
    display: grid;
    align-content: start;
    padding: 16px;
    border: 1px solid rgba(38, 48, 61, 0.14);
    border-radius: 8px;
    background: #fff;
  }

  .recipe-lines div,
  .recipe-card div {
    display: grid;
    grid-template-columns: 116px 1fr;
    gap: 12px;
    padding: 11px 0;
    border-bottom: 1px solid rgba(38, 48, 61, 0.14);
  }

  .recipe-lines div:last-child,
  .recipe-card div:last-child {
    border-bottom: 0;
  }

  .recipe-lines span,
  .recipe-card span {
    color: #738091;
    font-size: 12px;
    font-weight: 850;
  }

  .recipe-lines b,
  .recipe-card b {
    font-size: 13px;
    overflow-wrap: anywhere;
  }

  .approval-list {
    display: grid;
    gap: 8px;
  }

  .approval-list article {
    display: grid;
    grid-template-columns: 34px 1fr auto;
    gap: 12px;
    align-items: center;
    padding: 11px;
    border: 1px solid rgba(38, 48, 61, 0.14);
    border-radius: 8px;
    background: #fff;
  }

  .approval-list b {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 99px;
    background: #e8f0ff;
    color: #2563eb;
    font-size: 12px;
  }

  .approval-list strong,
  .approval-list small {
    display: block;
  }

  .approval-list small {
    color: #738091;
  }

  .inspector {
    padding: 22px 24px;
    display: grid;
    justify-items: center;
    align-content: start;
    gap: 16px;
    overflow: hidden;
  }

  .preview-card,
  .recipe-card,
  .checks-card,
  .success-card {
    width: 100%;
    max-width: 300px;
    padding: 16px;
    border: 1px solid rgba(38, 48, 61, 0.14);
    border-radius: 8px;
    background: #fff;
  }

  .preview-card {
    text-align: center;
  }

  .preview-icon {
    display: grid;
    place-items: center;
    width: 78px;
    height: 78px;
    margin: 0 auto 12px;
    border-radius: 18px;
    background: linear-gradient(145deg, #3b82f6, #2563eb);
    color: #fff;
    font-size: 34px;
    font-weight: 950;
    box-shadow: 0 12px 24px rgba(37, 99, 235, 0.2);
  }

  .checks-card {
    display: grid;
    gap: 8px;
  }

  .checks-card div {
    display: grid;
    grid-template-columns: 10px 1fr;
    gap: 8px;
    padding: 10px;
    border: 1px solid rgba(38, 48, 61, 0.14);
    border-radius: 8px;
    color: #485464;
    font-size: 12px;
  }

  .checks-card div::before {
    content: "";
    width: 10px;
    height: 10px;
    margin-top: 4px;
    border-radius: 99px;
    background: #94a3b8;
  }

  .checks-card .good::before { background: #087a4c; }
  .checks-card .warn::before { background: #d97706; }
  .checks-card .danger {
    border-color: rgba(194, 65, 12, 0.24);
    background: #fff7ed;
  }
  .checks-card .danger::before { background: #c2410c; }

  .success-card {
    display: grid;
    gap: 8px;
  }

  .success-card span {
    color: #485464;
    font-size: 12px;
    overflow-wrap: anywhere;
  }

  .button-row {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    justify-content: center;
  }

  @media (max-width: 1180px) {
    .workspace {
      grid-template-columns: 240px minmax(0, 1fr);
    }

    .inspector {
      grid-column: 1 / -1;
      grid-template-columns: repeat(3, minmax(0, 1fr));
      align-items: start;
    }
  }

  @media (max-width: 820px) {
    .app-shell {
      padding: 0;
    }

    .window {
      min-height: 100vh;
      grid-template-rows: auto 1fr;
    }

    .topbar,
    .workspace,
    .identity-layout,
    .review-grid,
    .type-grid,
    .two-col,
    .toggle-grid,
    .form-grid,
    .inspector {
      grid-template-columns: 1fr;
    }

    .tabs,
    .top-actions {
      width: 100%;
      overflow: visible;
    }

    .tabs {
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }

    .tabs button {
      min-width: 0;
      padding: 0 8px;
    }

    .top-actions {
      display: grid;
      grid-template-columns: 1fr 1fr;
    }

    .screen-head,
    .screen-footer,
    .hero-card,
    .target-card,
    .empty-state {
      grid-template-columns: 1fr;
      text-align: left;
    }

    .input-row {
      grid-template-columns: 1fr;
    }

    .status-score {
      text-align: left;
    }
  }
</style>
