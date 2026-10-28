<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  type RuntimeStatus = {
    appVersion: string;
    platform: string;
    integration: "scaffold";
  };

  let status = $state<RuntimeStatus | null>(null);
  let error = $state<string | null>(null);

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
      <a class="nav-item disabled" href="#themes" aria-disabled="true">
        <span class="nav-icon">◧</span>
        Temi
        <span class="nav-soon">PRESTO</span>
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
