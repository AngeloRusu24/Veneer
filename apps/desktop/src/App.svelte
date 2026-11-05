<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";

  type RuntimeStatus = {
    appVersion: string;
    platform: string;
    integration: "scaffold" | "window_frames";
  };

  let status = $state<RuntimeStatus | null>(null);
  let error = $state<string | null>(null);

  type ApplyReport = {
    themeId: string;
    matchedRules: string[];
    applied: string[];
    skipped: { property: string; requiredBuild: number }[];
    failed: { property: string; hresult: number }[];
    warnings: string[];
    wallpaper: "notRequested" | "applied" | "reverted" | "failed";
    settings: { name: string; status: "notRequested" | "applied" | "reverted" | "failed" }[];
  };
  type ThemeSummary = { id: string; name: string; ruleCount: number };

  const exampleTheme = JSON.stringify(
    {
      schemaVersion: 1,
      id: "midnight-work",
      name: "Midnight Work",
      unsupported: "warn",
      appearance: { apps: "dark", system: "dark" },
      rules: [
        {
          id: "veneer-window",
          when: { processName: "veneer-desktop.exe" },
          apply: {
            windowFrame: {
              colorMode: "dark",
              borderColor: "#52627A",
              captionColor: "#101116",
              textColor: "#E8EAF0",
              cornerPreference: "round",
              backdrop: "mica",
            },
          },
        },
      ],
    },
    null,
    2,
  );

  let themeText = $state(exampleTheme);
  let themeMessage = $state<{ kind: "ok" | "error"; text: string } | null>(null);
  let report = $state<ApplyReport | null>(null);
  let busy = $state(false);
  let autostart = $state(false);
  let appsMode = $state("dark");
  let systemMode = $state("dark");
  let cursorRole = $state("arrow");

  function updateTheme(change: (theme: Record<string, unknown>) => void, message: string) {
    let theme: Record<string, unknown>;
    try {
      theme = JSON.parse(themeText);
      if (typeof theme !== "object" || theme === null || Array.isArray(theme)) {
        throw new Error("Il tema deve essere un oggetto JSON");
      }
    } catch (cause) {
      throw new Error(`JSON del tema non valido: ${describe(cause)}`);
    }
    change(theme);
    themeText = JSON.stringify(theme, null, 2);
    report = null;
    themeMessage = { kind: "ok", text: message };
  }

  const setAppearance = (key: "apps" | "system", value: string) =>
    run(async () => {
      updateTheme((theme) => {
        const appearance =
          typeof theme.appearance === "object" && theme.appearance !== null
            ? { ...(theme.appearance as Record<string, unknown>) }
            : {};
        if (value) appearance[key] = value;
        else delete appearance[key];
        if (Object.keys(appearance).length) theme.appearance = appearance;
        else delete theme.appearance;
      }, "Modalità inserita nel tema; premi Applica");
      if (key === "apps") appsMode = value;
      else systemMode = value;
    });

  const chooseCursor = () =>
    run(async () => {
      const path = await open({
        multiple: false,
        filters: [{ name: "Cursori Windows", extensions: ["cur", "ani"] }],
      });
      if (!path) return;
      updateTheme((theme) => {
        const cursors =
          typeof theme.cursors === "object" && theme.cursors !== null
            ? { ...(theme.cursors as Record<string, unknown>) }
            : {};
        cursors[cursorRole] = path;
        theme.cursors = cursors;
      }, "Cursore inserito nel tema; premi Applica");
    });
  onMount(async () => {
    try {
      autostart = await invoke<boolean>("get_autostart");
    } catch {
      autostart = false;
    }
  });

  const toggleAutostart = () =>
    run(async () => {
      autostart = await invoke<boolean>("set_autostart", { enabled: !autostart });
    });

  function describe(cause: unknown): string {
    return cause instanceof Error ? cause.message : String(cause);
  }

  async function run(action: () => Promise<void>) {
    busy = true;
    try {
      await action();
    } catch (cause) {
      report = null;
      themeMessage = { kind: "error", text: describe(cause) };
    } finally {
      busy = false;
    }
  }

  const chooseWallpaper = () =>
    run(async () => {
      const path = await open({
        multiple: false,
        filters: [{ name: "Immagini", extensions: ["jpg", "jpeg", "png", "bmp", "tif", "tiff"] }],
      });
      if (!path) return;
      updateTheme((theme) => {
        theme.wallpaper = { path };
      }, "Sfondo inserito nel tema; premi Applica");
    });

  const validate = () =>
    run(async () => {
      const summary = await invoke<ThemeSummary>("validate_theme", { theme: themeText });
      report = null;
      themeMessage = {
        kind: "ok",
        text: `Tema valido: ${summary.name} (${summary.ruleCount} regole)`,
      };
    });

  const apply = () =>
    run(async () => {
      report = await invoke<ApplyReport>("apply_theme", { theme: themeText });
      themeMessage = { kind: "ok", text: "Tema applicato; le modifiche globali sono attive" };
    });

  const revert = () =>
    run(async () => {
      const result = await invoke<{
        frame: {
          wasStyled: boolean;
          failed: { property: string; hresult: number }[];
        };
        wallpaper: { wasChanged: boolean; failed: string[] };
        settings: { name: string; wasChanged: boolean; failed: string[] }[];
      }>("revert_theme");
      report = null;
      const failures = [
        ...result.frame.failed.map((item) => `cornice ${item.property} (${item.hresult})`),
        ...result.wallpaper.failed.map((item) => `sfondo: ${item}`),
        ...result.settings.flatMap((item) => item.failed.map((failure) => `${item.name}: ${failure}`)),
      ];
      themeMessage = {
        kind: failures.length ? "error" : "ok",
        text: failures.length
          ? `Ripristino incompleto: ${failures.join("; ")}`
          : result.frame.wasStyled ||
              result.wallpaper.wasChanged ||
              result.settings.some((item) => item.wasChanged)
            ? "Stato originale ripristinato"
            : "Nessuna modifica da annullare",
      };
    });

  onMount(async () => {
    try {
      status = await invoke<RuntimeStatus>("get_runtime_status");
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    }
  });
</script>

<svelte:head>
  <title>Veneer — Personalizzazione desktop</title>
  <meta
    name="description"
    content="Configurazione di temi desktop basata su API Windows supportate."
  />
</svelte:head>

<main class="shell">
  <aside class="sidebar">
    <a class="brand" href="/" aria-label="Veneer, home">
      <span class="brand-mark">V</span>
      <span>veneer</span>
    </a>

    <p class="nav-label">WORKSPACE</p>
    <nav aria-label="Navigazione principale">
      <a class="nav-item active" href="#overview" aria-current="page">
        <span class="nav-icon">◈</span>
        Panoramica
      </a>
      <a class="nav-item" href="#themes">
        <span class="nav-icon">◧</span>
        Temi
      </a>
      <a class="nav-item disabled" href="#rules" aria-disabled="true">
        <span class="nav-icon">⌘</span>
        Regole
        <span class="nav-soon">PRESTO</span>
      </a>
    </nav>

    <div class="sidebar-footer">
      <span class="status-dot"></span>
      <span>Modalità sviluppo</span>
    </div>
  </aside>

  <section class="content" id="overview">
    <header class="topbar">
      <span class="breadcrumb">Workspace <span>/</span> Panoramica</span>
      <span class="version">{status ? `v${status.appVersion}` : "v…"}</span>
    </header>

    <div class="page">
      <div class="intro">
        <p class="eyebrow">IL TUO DESKTOP, CON CURA</p>
        <h1>Un ambiente che<br />ti somiglia.</h1>
        <p class="lead">
          Personalizzazione trasparente, reversibile e basata solo su API
          Windows documentate.
        </p>
      </div>

      <section class="notice" aria-label="Stato del progetto">
        <div class="notice-icon">✳</div>
        <div>
          <h2>Personalizzazione reversibile</h2>
          <p>
            Cornici e impostazioni globali vengono applicate tramite API
            Windows documentate; le modifiche globali vengono ripristinate
            quando annulli o esci.
          </p>
        </div>
        <span class="badge">MVP</span>
      </section>

      <section class="panel" id="themes" aria-label="Anteprima tema">
        <div class="section-heading">
          <div>
            <p class="eyebrow">ANTEPRIMA</p>
            <h2>Prova un tema</h2>
          </div>
          <span class="muted">Cornice della finestra + impostazioni globali selezionate</span>
        </div>
        <textarea
          class="theme-input"
          spellcheck="false"
          aria-label="Tema JSON"
          bind:value={themeText}
        ></textarea>
        <div class="actions">
          <label>
            Modalità app
            <select
              value={appsMode}
              disabled={busy}
              onchange={(event) => setAppearance("apps", event.currentTarget.value)}
            >
              <option value="">Non modificare</option>
              <option value="light">Chiara</option>
              <option value="dark">Scura</option>
            </select>
          </label>
          <label>
            Modalità Windows
            <select
              value={systemMode}
              disabled={busy}
              onchange={(event) => setAppearance("system", event.currentTarget.value)}
            >
              <option value="">Non modificare</option>
              <option value="light">Chiara</option>
              <option value="dark">Scura</option>
            </select>
          </label>
          <label>
            Ruolo cursore
            <select bind:value={cursorRole} disabled={busy}>
              <option value="arrow">Freccia</option>
              <option value="ibeam">Testo</option>
              <option value="wait">Attesa</option>
              <option value="appStarting">Avvio app</option>
              <option value="hand">Mano</option>
              <option value="help">Aiuto</option>
              <option value="cross">Precisione</option>
              <option value="no">Non disponibile</option>
              <option value="sizeAll">Sposta</option>
              <option value="sizeNs">Ridimensiona verticale</option>
              <option value="sizeWe">Ridimensiona orizzontale</option>
              <option value="sizeNesw">Ridimensiona diagonale ↗</option>
              <option value="sizeNwse">Ridimensiona diagonale ↘</option>
              <option value="upArrow">Freccia alternativa</option>
            </select>
          </label>
          <button type="button" onclick={chooseCursor} disabled={busy}>Scegli cursore…</button>
        </div>
        <div class="actions">
          <button type="button" onclick={chooseWallpaper} disabled={busy}>Scegli sfondo…</button>
          <button type="button" onclick={validate} disabled={busy}>Valida</button>
          <button type="button" class="primary" onclick={apply} disabled={busy}>Applica</button>
          <button type="button" onclick={revert} disabled={busy}>Annulla</button>
        </div>
        {#if themeMessage}
          <p class={themeMessage.kind === "error" ? "error" : "ok"} role="status">
            {themeMessage.text}
          </p>
        {/if}
        {#if report}
          <ul class="report">
            <li>Regole attive: {report.matchedRules.join(", ") || "nessuna"}</li>
            <li>Applicate: {report.applied.join(", ") || "nessuna"}</li>
            {#if report.wallpaper === "applied"}
              <li>Sfondo impostato</li>
            {:else if report.wallpaper === "reverted"}
              <li>Sfondo precedente ripristinato</li>
            {:else if report.wallpaper === "failed"}
              <li class="warn">Sfondo non impostato (vedi avvisi)</li>
            {/if}
            {#each report.settings as setting}
              {#if setting.status !== "notRequested"}
                <li class:warn={setting.status === "failed"}>
                  {setting.name}: {setting.status}
                </li>
              {/if}
            {/each}
            {#each report.skipped as item}
              <li class="warn">Saltata {item.property}: richiede build {item.requiredBuild}</li>
            {/each}
            {#each report.failed as item}
              <li class="error">Rifiutata da Windows: {item.property} ({item.hresult})</li>
            {/each}
            {#each report.warnings as warning}
              <li class="warn">{warning}</li>
            {/each}
          </ul>
        {/if}
      </section>

      <label class="autostart">
        <input
          type="checkbox"
          checked={autostart}
          disabled={busy}
          onchange={toggleAutostart}
        />
        Avvia Veneer con Windows (ridotto nella tray)
      </label>

      <div class="section-heading">
        <div>
          <p class="eyebrow">CAPACITÀ</p>
          <h2>Moduli del progetto</h2>
        </div>
        <span class="muted">Disponibilità in questa build</span>
      </div>

      <div class="module-grid">
        <article class="module-card">
          <div class="card-top">
            <span class="module-icon violet">▱</span>
            <span class="module-state">MVP</span>
          </div>
          <h3>Cornici delle finestre</h3>
          <p>Attributi DWM sulle sole finestre controllate dall’app.</p>
          <div class="card-bottom"><span>API documentate</span><span>↗</span></div>
        </article>

        <article class="module-card">
          <div class="card-top">
            <span class="module-icon blue">⌁</span>
            <span class="module-state">MVP</span>
          </div>
          <h3>Temi e regole</h3>
          <p>Configurazioni dichiarative validate prima dell’applicazione.</p>
          <div class="card-bottom"><span>JSON versionato</span><span>↗</span></div>
        </article>

        <article class="module-card">
          <div class="card-top">
            <span class="module-icon violet">◉</span>
            <span class="module-state">MVP</span>
          </div>
          <h3>Aspetto e cursori</h3>
          <p>Modalità chiara/scura e cursori globali, con ripristino automatico.</p>
          <div class="card-bottom"><span>Una modalità è best-effort</span><span>!</span></div>
        </article>

        <article class="module-card unavailable">
          <div class="card-top">
            <span class="module-icon amber">◉</span>
            <span class="module-state">NON OFFERTI</span>
          </div>
          <h3>Filtri globali</h3>
          <p>
            Le API pubbliche verificate non garantiscono filtri arbitrari per
            schermo o monitor.
          </p>
          <div class="card-bottom"><span>Nessuna API documentata</span><span>!</span></div>
        </article>

        <article class="module-card unavailable">
          <div class="card-top">
            <span class="module-icon green">▧</span>
            <span class="module-state">STATICI SOLI</span>
          </div>
          <h3>Sfondi</h3>
          <p>
            Sfondi statici supportati. GIF, video e shader richiederebbero API
            desktop non documentate e non sono offerti.
          </p>
          <div class="card-bottom"><span>Animazioni non offerte</span><span>!</span></div>
        </article>
      </div>

      <footer class="runtime">
        {#if error}
          <span class="error">Backend non disponibile: {error}</span>
        {:else if status}
          <span><span class="status-dot"></span> Backend connesso</span>
          <span>{status.platform} · integrazione {status.integration}</span>
        {:else}
          <span>Connessione al backend…</span>
        {/if}
      </footer>
    </div>
  </section>
</main>
