<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
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
  };
  type ThemeSummary = { id: string; name: string; ruleCount: number };

  const exampleTheme = JSON.stringify(
    {
      schemaVersion: 1,
      id: "midnight-work",
      name: "Midnight Work",
      unsupported: "warn",
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
  let wallpaperPath = $state("");
  let wallpaperMessage = $state<{ kind: "ok" | "error"; text: string } | null>(null);

  const setWallpaper = async () => {
    busy = true;
    try {
      await invoke("set_wallpaper", { path: wallpaperPath });
      wallpaperMessage = { kind: "ok", text: "Sfondo applicato; l'originale è salvato su disco" };
    } catch (cause) {
      wallpaperMessage = { kind: "error", text: describe(cause) };
    } finally {
      busy = false;
    }
  };

  const revertWallpaper = async () => {
    busy = true;
    try {
      const result = await invoke<{ wasChanged: boolean; failed: string[] }>("revert_wallpaper");
      wallpaperMessage = result.failed.length
        ? { kind: "error", text: `Ripristino incompleto: ${result.failed.join("; ")}` }
        : {
            kind: "ok",
            text: result.wasChanged ? "Sfondo originale ripristinato" : "Nessuna modifica da annullare",
          };
    } catch (cause) {
      wallpaperMessage = { kind: "error", text: describe(cause) };
    } finally {
      busy = false;
    }
  };

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
      themeMessage = { kind: "ok", text: "Tema applicato a questa finestra" };
    });

  const revert = () =>
    run(async () => {
      const result = await invoke<{ wasStyled: boolean }>("revert_theme");
      report = null;
      themeMessage = {
        kind: "ok",
        text: result.wasStyled ? "Stile originale ripristinato" : "Nessuna modifica da annullare",
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
          <h2>Il workspace è pronto per iniziare</h2>
          <p>
            Veneer è in fase di sviluppo. Al momento non applica modifiche al
            sistema né alle finestre di altre applicazioni.
          </p>
        </div>
        <span class="badge">SCAFFOLD</span>
      </section>

      <section class="panel" id="themes" aria-label="Anteprima tema">
        <div class="section-heading">
          <div>
            <p class="eyebrow">ANTEPRIMA</p>
            <h2>Prova un tema</h2>
          </div>
          <span class="muted">Si applica solo alla finestra di Veneer</span>
        </div>
        <textarea
          class="theme-input"
          spellcheck="false"
          aria-label="Tema JSON"
          bind:value={themeText}
        ></textarea>
        <div class="actions">
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

      <section class="panel" aria-label="Sfondo">
        <div class="section-heading">
          <div>
            <p class="eyebrow">SFONDO</p>
            <h2>Sfondo statico</h2>
          </div>
          <span class="muted">Tutti i monitor · annullabile, anche dopo un crash</span>
        </div>
        <input
          class="path-input"
          type="text"
          placeholder="C:\Immagini\sfondo.jpg"
          aria-label="Percorso immagine"
          bind:value={wallpaperPath}
        />
        <div class="actions">
          <button type="button" class="primary" onclick={setWallpaper} disabled={busy || !wallpaperPath.trim()}>
            Imposta
          </button>
          <button type="button" onclick={revertWallpaper} disabled={busy}>Ripristina</button>
        </div>
        {#if wallpaperMessage}
          <p class={wallpaperMessage.kind === "error" ? "error" : "ok"} role="status">
            {wallpaperMessage.text}
          </p>
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
            <span class="module-state">IN SVILUPPO</span>
          </div>
          <h3>Cornici delle finestre</h3>
          <p>Attributi DWM sulle sole finestre controllate dall’app.</p>
          <div class="card-bottom"><span>API documentate</span><span>↗</span></div>
        </article>

        <article class="module-card">
          <div class="card-top">
            <span class="module-icon blue">⌁</span>
            <span class="module-state">IN SVILUPPO</span>
          </div>
          <h3>Temi e regole</h3>
          <p>Configurazioni dichiarative validate prima dell’applicazione.</p>
          <div class="card-bottom"><span>JSON versionato</span><span>↗</span></div>
        </article>

        <article class="module-card unavailable">
          <div class="card-top">
            <span class="module-icon amber">◉</span>
            <span class="module-state">DA VERIFICARE</span>
          </div>
          <h3>Filtri globali</h3>
          <p>
            Le API pubbliche verificate non garantiscono filtri arbitrari per
            schermo o monitor.
          </p>
          <div class="card-bottom"><span>Possibili limiti Windows</span><span>!</span></div>
        </article>

        <article class="module-card unavailable">
          <div class="card-top">
            <span class="module-icon green">▧</span>
            <span class="module-state">DA VERIFICARE</span>
          </div>
          <h3>Sfondi animati</h3>
          <p>
            L’interfaccia documentata supporta immagini statiche e slideshow,
            non renderer animati agganciati al desktop.
          </p>
          <div class="card-bottom"><span>Possibili limiti Windows</span><span>!</span></div>
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
